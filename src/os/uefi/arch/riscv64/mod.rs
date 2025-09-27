use crate::KernelArgs;
use crate::arch::PHYS_OFFSET;
use crate::logger::LOGGER;
use crate::os::OsEfi;
use crate::os::uefi::memory_map::memory_map;
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
    unsafe {
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
}

pub fn main() -> Result<()> {
    LOGGER.init();

    let mut os = OsEfi::new();

    // Disable cursor
    let _ = (os.st.ConsoleOut.EnableCursor)(os.st.ConsoleOut, false);

    let (page_phys, func, args) = crate::main(&mut os);

    unsafe {
        memory_map().exit_boot_services();

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
