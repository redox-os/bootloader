use alloc::alloc::{alloc_zeroed, Layout};
use core::{convert::TryFrom, mem, ptr, slice};
use linked_list_allocator::LockedHeap;
use spin::Mutex;

use crate::logger::LOGGER;
use crate::os::{Os, OsHwDesc, OsKey, OsVideoMode};
use crate::KernelArgs;

use self::disk::DiskBios;
use self::memory_map::memory_map;
use self::thunk::ThunkData;
use self::vbe::VideoModeIter;
use self::vga::{Vga, VgaTextColor};

#[macro_use]
mod macros;

mod disk;
mod memory_map;
mod panic;
pub(crate) mod serial;
mod thunk;
mod vbe;
mod vga;

// Real mode memory allocation, for use with thunk
// 0x500 to 0x7BFF is free
const DISK_BIOS_ADDR: usize = 0x70000; // 64 KiB at 448 KiB, ends at 512 KiB
const VBE_CARD_INFO_ADDR: usize = 0x1000; // 512 bytes, ends at 0x11FF
const VBE_MODE_INFO_ADDR: usize = 0x1200; // 256 bytes, ends at 0x12FF
const VBE_EDID_ADDR: usize = 0x1300; // 128 bytes, ends at 0x137F
const MEMORY_MAP_ADDR: usize = 0x1380; // 24 bytes, ends at 0x1397
const DISK_ADDRESS_PACKET_ADDR: usize = 0x1398; // 16 bytes, ends at 0x13A7
const THUNK_STACK_ADDR: usize = 0x7C00; // Grows downwards
const VGA_ADDR: usize = 0xB8000;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub(crate) static VGA: Mutex<Vga> = Mutex::new(unsafe { Vga::new(VGA_ADDR, 80, 25) });

pub struct OsBios {
    boot_disk: usize,
    thunk10: extern "C" fn(),
    thunk13: extern "C" fn(),
    thunk15: extern "C" fn(),
    thunk16: extern "C" fn(),
}

#[allow(dead_code)]
#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
pub struct Rsdp {
    signature: [u8; 8],
    checksum: u8,
    oemid: [u8; 6],
    revision: u8,
    rsdt_address: u32,
}

#[allow(dead_code)]
#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
pub struct Xsdp {
    rsdp: Rsdp,

    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    reserved: [u8; 3],
}

unsafe fn search_rsdp(start: usize, end: usize) -> Option<(u64, u64)> {
    // Align start up to 16 bytes
    let mut addr = ((start + 15) / 16) * 16;
    // Search until reading the end of the Rsdp would be past the end of the memory area
    while addr + mem::size_of::<Rsdp>() <= end {
        let rsdp = ptr::read(addr as *const Rsdp);
        if &rsdp.signature == b"RSD PTR " {
            //TODO: check checksum?
            if rsdp.revision == 0 {
                return Some((addr as u64, mem::size_of::<Rsdp>() as u64));
            } else if rsdp.revision == 2 {
                let xsdp = ptr::read(addr as *const Xsdp);
                //TODO: check extended checksum?
                return Some((addr as u64, xsdp.length as u64));
            }
        }

        // Rsdp is always aligned to 16 bytes
        addr += 16;
    }
    None
}

impl Os<DiskBios, VideoModeIter> for OsBios {
    fn name(&self) -> &str {
        "x86/BIOS"
    }

    fn alloc_zeroed_page_aligned(&self, size: usize) -> *mut u8 {
        assert!(size != 0);

        let page_size = self.page_size();
        let pages = (size + page_size - 1) / page_size;

        let ptr =
            unsafe { alloc_zeroed(Layout::from_size_align(pages * page_size, page_size).unwrap()) };

        assert!(!ptr.is_null());
        ptr
    }

    fn page_size(&self) -> usize {
        4096
    }

    fn filesystem(
        &self,
        password_opt: Option<&[u8]>,
    ) -> syscall::Result<redoxfs::FileSystem<DiskBios>> {
        let disk = DiskBios::new(u8::try_from(self.boot_disk).unwrap(), self.thunk13);

        //TODO: get block from partition table
        let block = 2 * crate::MIBI as u64 / redoxfs::BLOCK_SIZE;
        redoxfs::FileSystem::open(disk, password_opt, Some(block), false)
    }

    fn hwdesc(&self) -> OsHwDesc {
        // See ACPI specification - Finding the RSDP on IA-PC Systems
        unsafe {
            let ebda_segment = ptr::read(0x40E as *const u16);
            let ebda_addr = (ebda_segment as usize) << 4;
            if let Some((addr, size)) =
                search_rsdp(ebda_addr, ebda_addr + 1024).or(search_rsdp(0xE0000, 0xFFFFF))
            {
                // Copy to a page
                let page_aligned = self.alloc_zeroed_page_aligned(size as usize);
                ptr::copy(addr as *const u8, page_aligned, size as usize);
                return OsHwDesc::Acpi(page_aligned as u64, size);
            }
        }
        OsHwDesc::NotFound
    }

    fn video_outputs(&self) -> usize {
        //TODO: return 1 only if vbe supported?
        1
    }

    fn video_modes(&self, _output_i: usize) -> VideoModeIter {
        VideoModeIter::new(self.thunk10)
    }

    fn set_video_mode(&self, _output_i: usize, mode: &mut OsVideoMode) {
        // Set video mode
        let mut data = ThunkData::new();
        data.eax = 0x4F02;
        data.ebx = mode.id;
        unsafe {
            data.with(self.thunk10);
        }
        //TODO: check result
    }

    fn best_resolution(&self, _output_i: usize) -> Option<(u32, u32)> {
        let mut data = ThunkData::new();
        data.eax = 0x4F15;
        data.ebx = 0x01;
        data.ecx = 0;
        data.edx = 0;
        data.edi = VBE_EDID_ADDR as u32;
        unsafe {
            data.with(self.thunk10);
        }

        if data.eax == 0x4F {
            let edid = unsafe { slice::from_raw_parts(VBE_EDID_ADDR as *const u8, 128) };

            Some((
                (edid[0x38] as u32) | (((edid[0x3A] as u32) & 0xF0) << 4),
                (edid[0x3B] as u32) | (((edid[0x3D] as u32) & 0xF0) << 4),
            ))
        } else {
            log::warn!("Failed to get VBE EDID: 0x{:X}", { data.eax });
            None
        }
    }

    fn get_key(&self) -> OsKey {
        // Read keypress
        let mut data = ThunkData::new();
        unsafe {
            data.with(self.thunk16);
        }
        match (data.eax >> 8) as u8 {
            0x4B => OsKey::Left,
            0x4D => OsKey::Right,
            0x48 => OsKey::Up,
            0x50 => OsKey::Down,
            0x0E => OsKey::Backspace,
            0x53 => OsKey::Delete,
            0x1C => OsKey::Enter,
            _ => match data.eax as u8 {
                0 => OsKey::Other,
                b => OsKey::Char(b as char),
            },
        }
    }

    fn clear_text(&self) {
        //TODO: clear screen for VGA
    }

    fn get_text_position(&self) -> (usize, usize) {
        let vga = VGA.lock();
        (vga.x, vga.y)
    }

    fn set_text_position(&self, x: usize, y: usize) {
        //TODO: ensure this is inside bounds!
        let mut vga = VGA.lock();
        vga.x = x;
        vga.y = y;
    }

    fn set_text_highlight(&self, highlight: bool) {
        let mut vga = VGA.lock();
        if highlight {
            vga.bg = VgaTextColor::Gray;
            vga.fg = VgaTextColor::Black;
        } else {
            vga.bg = VgaTextColor::Black;
            vga.fg = VgaTextColor::Gray;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn start(
    kernel_entry: extern "C" fn(
        page_table: usize,
        stack: u64,
        func: u64,
        args: *const KernelArgs,
        long_mode: usize,
    ) -> !,
    boot_disk: usize,
    thunk10: extern "C" fn(),
    thunk13: extern "C" fn(),
    thunk15: extern "C" fn(),
    thunk16: extern "C" fn(),
) -> ! {
    #[cfg(feature = "serial_debug")]
    {
        let mut com1 = serial::COM1.lock();
        com1.init();
        com1.write(b"SERIAL\n");
    }

    {
        // Make sure we are in mode 3 (80x25 text mode)
        let mut data = ThunkData::new();
        data.eax = 0x03;
        data.with(thunk10);
    }

    {
        // Disable cursor
        let mut data = ThunkData::new();
        data.eax = 0x0100;
        data.ecx = 0x3F00;
        data.with(thunk10);
    }

    // Clear screen
    VGA.lock().clear();

    // Set logger
    LOGGER.init();

    let mut os = OsBios {
        boot_disk,
        thunk10,
        thunk13,
        thunk15,
        thunk16,
    };

    let (heap_start, heap_size) = memory_map(os.thunk15).expect("No memory for heap");

    ALLOCATOR.lock().init(heap_start as *mut u8, heap_size);

    let (page_phys, func, args) = crate::main(&mut os);

    kernel_entry(
        page_phys,
        args.stack_base
            + args.stack_size
            + if crate::KERNEL_64BIT {
                crate::arch::x64::PHYS_OFFSET as u64
            } else {
                crate::arch::x32::PHYS_OFFSET as u64
            },
        func,
        &args,
        if crate::KERNEL_64BIT { 1 } else { 0 },
    );
}
