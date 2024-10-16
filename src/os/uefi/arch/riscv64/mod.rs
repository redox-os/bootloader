use crate::arch::PHYS_OFFSET;
use crate::logger::LOGGER;
use crate::os::dtb::find_dtb;
use crate::os::uefi::dtb::{RSDP_AREA_BASE, RSDP_AREA_SIZE};
use crate::os::uefi::memory_map::memory_map;
use crate::os::OsEfi;
use crate::KernelArgs;
use core::arch::asm;
use core::mem;
use uefi::status::Result;

mod boot_protocol;
mod coff_helper;

pub use boot_protocol::*;

unsafe extern "C" fn kernel_entry(
    page_phys: usize,
    stack: u64,
    func: u64,
    args: *const KernelArgs,
) -> ! {
    // Set page tables
    asm!(
    "sfence.vma",
    "csrw satp, {0}",
    in(reg) (page_phys >> 12 | 9 << 60) // Sv48 mode
    );

    let entry_fn: extern "C" fn(*const KernelArgs) -> ! = mem::transmute(func);

    // Set stack and go to kernel
    asm!("mv sp, {0}",
    "mv a0, {1}",
    "jalr {2}",
    in(reg) stack,
    in(reg) args,
    in(reg) entry_fn
    );
    loop {}
}

pub fn main() -> Result<()> {
    LOGGER.init();

    let mut os = OsEfi::new();

    // Disable cursor
    let _ = (os.st.ConsoleOut.EnableCursor)(os.st.ConsoleOut, false);

    find_dtb(&mut os);

    let (page_phys, func, mut args) = crate::main(&mut os);

    unsafe {
        memory_map().exit_boot_services();

        args.acpi_rsdp_base = RSDP_AREA_BASE as u64;
        args.acpi_rsdp_size = RSDP_AREA_SIZE as u64;

        kernel_entry(
            page_phys,
            args.stack_base + args.stack_size + PHYS_OFFSET,
            func,
            &args,
        );
    }
}

pub fn disable_interrupts() {
    unsafe {
        asm!("csrci sstatus, 2");
    }
}
