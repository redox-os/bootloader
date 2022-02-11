use core::{
    mem,
    ptr
};
use std::{
    vec::Vec,
};
use uefi::{
    guid::GuidKind,
    status::Result,
};

use crate::{
    KernelArgs,
    logger::LOGGER,
};

use super::super::{
    OsEfi,
};

use self::memory_map::memory_map;
use self::paging::paging_enter;

mod memory_map;
mod paging;

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

    find_acpi_table_pointers();

    let mut os = OsEfi {
        st: std::system_table(),
    };

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
