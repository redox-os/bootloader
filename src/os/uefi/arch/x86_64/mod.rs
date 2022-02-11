use core::{mem, ops::{ControlFlow, Try}, ptr, slice};
use std::proto::Protocol;
use std::vec::Vec;
use uefi::status::{Result, Status};
use uefi::guid::GuidKind;
use uefi::memory::MemoryType;
use uefi::system::SystemTable;
use uefi::text::TextInputKey;

use crate::{
    KernelArgs,
    Os,
    OsKey,
    OsVideoMode,
    logger::LOGGER,
};

use super::super::{
    disk::DiskEfi,
    display::{EdidActive, Output},
};

use self::memory_map::{MemoryMapIter, memory_map};
use self::paging::paging_enter;
use self::video_mode::VideoModeIter;

mod memory_map;
mod paging;
mod video_mode;

static PHYS_OFFSET: u64 = 0xFFFF800000000000;

static mut RSDPS_AREA: Option<Vec<u8>> = None;

unsafe fn exit_boot_services(key: usize) {
    let handle = std::handle();
    let uefi = std::system_table();

    let _ = (uefi.BootServices.ExitBootServices)(handle, key);
}

struct Invalid;

fn validate_rsdp(address: usize, v2: bool) -> core::result::Result<usize, Invalid> {
    #[repr(packed)]
    #[derive(Clone, Copy, Debug)]
    struct Rsdp {
        signature: [u8; 8], // b"RSD PTR "
        chksum: u8,
        oem_id: [u8; 6],
        revision: u8,
        rsdt_addr: u32,
        // the following fields are only available for ACPI 2.0, and are reserved otherwise
        length: u32,
        xsdt_addr: u64,
        extended_chksum: u8,
        _rsvd: [u8; 3],
    }
    // paging is not enabled at this stage; we can just read the physical address here.
    let rsdp_bytes = unsafe { core::slice::from_raw_parts(address as *const u8, core::mem::size_of::<Rsdp>()) };
    let rsdp = unsafe { (rsdp_bytes.as_ptr() as *const Rsdp).as_ref::<'static>().unwrap() };

    log::debug!("RSDP: {:?}", rsdp);

    if rsdp.signature != *b"RSD PTR " {
        return Err(Invalid);
    }
    let mut base_sum = 0u8;
    for base_byte in &rsdp_bytes[..20] {
        base_sum = base_sum.wrapping_add(*base_byte);
    }
    if base_sum != 0 {
        return Err(Invalid);
    }

    if rsdp.revision == 2 {
        let mut extended_sum = 0u8;
        for byte in rsdp_bytes {
            extended_sum = extended_sum.wrapping_add(*byte);
        }

        if extended_sum != 0 {
            return Err(Invalid);
        }
    }

    let length = if rsdp.revision == 2 { rsdp.length as usize } else { core::mem::size_of::<Rsdp>() };

    Ok(length)
}

fn find_acpi_table_pointers() {
    let rsdps_area = unsafe {
        RSDPS_AREA = Some(Vec::new());
        RSDPS_AREA.as_mut().unwrap()
    };

    let cfg_tables = std::system_table().config_tables();

    for (address, v2) in cfg_tables.iter().find_map(|cfg_table| {
        if cfg_table.VendorGuid.kind() == GuidKind::Acpi {
            Some((cfg_table.VendorTable, false))
        } else if cfg_table.VendorGuid.kind() == GuidKind::Acpi2 {
            Some((cfg_table.VendorTable, true))
        } else {
            None
        }
    }) {
        match validate_rsdp(address, v2) {
            Ok(length) => {
                let align = 8;

                rsdps_area.extend(&u32::to_ne_bytes(length as u32));
                rsdps_area.extend(unsafe { core::slice::from_raw_parts(address as *const u8, length) });
                rsdps_area.resize(((rsdps_area.len() + (align - 1)) / align) * align, 0u8);
            }
            Err(_) => log::warn!("Found RSDP that was not valid at {:p}", address as *const u8),
        }
    }
}

pub struct OsEfi {
    st: &'static SystemTable,
}

fn status_to_result(status: Status) -> Result<usize> {
    match status.branch() {
        ControlFlow::Continue(ok) => Ok(ok),
        ControlFlow::Break(err) => Err(err),
    }
}

impl Os<
    DiskEfi,
    MemoryMapIter,
    VideoModeIter
> for OsEfi {
    fn name(&self) -> &str {
        "x86_64/UEFI"
    }

    fn alloc_zeroed_page_aligned(&self, size: usize) -> *mut u8 {
        assert!(size != 0);

        let page_size = self.page_size();
        let pages = (size + page_size - 1) / page_size;

        let ptr = {
            let mut ptr = 0;
            status_to_result(
                (self.st.BootServices.AllocatePages)(
                    0, // AllocateAnyPages
                    MemoryType::EfiRuntimeServicesData, // Keeps this memory out of free space list
                    pages,
                    &mut ptr
                )
            ).unwrap();
            ptr as *mut u8
        };

        assert!(!ptr.is_null());
        unsafe { ptr::write_bytes(ptr, 0, pages * page_size) };
        ptr
    }

    fn page_size(&self) -> usize {
        4096
    }

    fn filesystem(&self) -> redoxfs::FileSystem<DiskEfi> {
        for (i, block_io) in DiskEfi::all().into_iter().enumerate() {
            if !block_io.0.Media.LogicalPartition {
                continue;
            }

            match redoxfs::FileSystem::open(block_io, Some(0)) {
                Ok(ok) => return ok,
                Err(err) => match err.errno {
                    // Ignore header not found error
                    syscall::ENOENT => (),
                    // Print any other errors
                    _ => log::error!("Failed to open RedoxFS on block I/O {}: {}", i, err),
                }
            }
        }
        panic!("Failed to find RedoxFS");
    }

    fn memory(&self) -> MemoryMapIter {
        MemoryMapIter::new()
    }

    fn video_modes(&self) -> VideoModeIter {
        VideoModeIter::new()
    }

    fn set_video_mode(&self, mode: &mut OsVideoMode) {
        let output = Output::one().unwrap();
        status_to_result(
            (output.0.SetMode)(output.0, mode.id)
        ).unwrap();

        // Update frame buffer base
        mode.base = output.0.Mode.FrameBufferBase as u64;
    }

    fn best_resolution(&self) -> Option<(u32, u32)> {
        //TODO: get this per output
        match EdidActive::one() {
            Ok(efi_edid) => {
                let edid = unsafe {
                    slice::from_raw_parts(efi_edid.0.Edid, efi_edid.0.SizeOfEdid as usize)
                };

                Some((
                    (edid[0x38] as u32) | (((edid[0x3A] as u32) & 0xF0) << 4),
                    (edid[0x3B] as u32) | (((edid[0x3D] as u32) & 0xF0) << 4),
                ))
            },
            Err(err) => {
                log::warn!("Failed to get EFI EDID: {:?}", err);

                // Fallback to the current output resolution
                match Output::one() {
                    Ok(output) => {
                        Some((
                            output.0.Mode.Info.HorizontalResolution,
                            output.0.Mode.Info.VerticalResolution,
                        ))
                    },
                    Err(err) => {
                        log::error!("Failed to get output: {:?}", err);
                        None
                    }
                }
            }
        }
    }

    fn get_key(&self) -> OsKey {
        //TODO: do not unwrap

        let mut index = 0;
        status_to_result(
            (self.st.BootServices.WaitForEvent)(1, &self.st.ConsoleIn.WaitForKey, &mut index)
        ).unwrap();

        let mut key = TextInputKey {
            ScanCode: 0,
            UnicodeChar: 0
        };
        status_to_result(
            (self.st.ConsoleIn.ReadKeyStroke)(self.st.ConsoleIn, &mut key)
        ).unwrap();

        match key.ScanCode {
            0 => match key.UnicodeChar {
                13 => OsKey::Enter,
                _ => OsKey::Other,
            },
            1 => OsKey::Up,
            2 => OsKey::Down,
            3 => OsKey::Right,
            4 => OsKey::Left,
            _ => OsKey::Other,
        }
    }

    fn get_text_position(&self) -> (usize, usize) {
        (
            self.st.ConsoleOut.Mode.CursorColumn as usize,
            self.st.ConsoleOut.Mode.CursorRow as usize,
        )
    }

    fn set_text_position(&self, x: usize, y: usize) {
        status_to_result(
            (self.st.ConsoleOut.SetCursorPosition)(self.st.ConsoleOut, x, y)
        ).unwrap();
    }

    fn set_text_highlight(&self, highlight: bool) {
        let attr = if highlight { 0x70 } else { 0x07 };
        status_to_result(
            (self.st.ConsoleOut.SetAttribute)(self.st.ConsoleOut, attr)
        ).unwrap();
    }
}

unsafe extern "C" fn kernel_entry(
    page_phys: usize,
    stack: u64,
    func: u64,
    args: *const KernelArgs,
) -> ! {
    // Read memory map and exit boot services
    let key = memory_map();
    exit_boot_services(key);

    // Disable interrupts
    llvm_asm!("cli" : : : "memory" : "intel", "volatile");

    // Enable paging
    paging_enter(page_phys as u64);

    // Set stack
    llvm_asm!("mov rsp, $0" : : "r"(stack) : "memory" : "intel", "volatile");

    // Call kernel entry
    let entry_fn: extern "sysv64" fn(args_ptr: *const KernelArgs) -> ! = mem::transmute(func);
    entry_fn(args);
}

pub fn main() -> Result<()> {
    LOGGER.init();

    let mut os = OsEfi {
        st: std::system_table(),
    };

    find_acpi_table_pointers();

    let (page_phys, mut args) = crate::main(&mut os);

    unsafe {
        args.acpi_rsdps_base = RSDPS_AREA.as_ref().map(Vec::as_ptr).unwrap_or(core::ptr::null()) as usize as u64 + PHYS_OFFSET;
        args.acpi_rsdps_size = RSDPS_AREA.as_ref().map(Vec::len).unwrap_or(0) as u64;

        kernel_entry(
            page_phys,
            args.stack_base + args.stack_size + PHYS_OFFSET,
            ptr::read((args.kernel_base + 0x18) as *const u64),
            &args,
        );
    }
}
