use core::{mem, ptr};
use std::vec::Vec;
use uefi::status::Result;

use crate::{
    KernelArgs,
    logger::LOGGER,
};

use super::super::{
    OsEfi,
    exit_boot_services,
    acpi::{
        RSDPS_AREA,
        find_acpi_table_pointers,
    },
};

use self::memory_map::memory_map;
use self::paging::paging_enter;

mod memory_map;
mod paging;

static PHYS_OFFSET: u64 = 0xFFFF800000000000;

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
    let entry_fn: extern "sysv64" fn(*const KernelArgs) -> ! = mem::transmute(func);
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
