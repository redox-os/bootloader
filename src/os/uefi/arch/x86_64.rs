use core::{arch::asm, mem};
use uefi::status::Result;
use x86::{
    controlregs::{self, Cr0, Cr4},
    msr,
};

use crate::{logger::LOGGER, KernelArgs};

use super::super::{memory_map::memory_map, OsEfi};

unsafe extern "C" fn kernel_entry(
    page_phys: usize,
    stack: u64,
    func: u64,
    args: *const KernelArgs,
) -> ! {
    // Read memory map and exit boot services
    memory_map().exit_boot_services();

    // Enable FXSAVE/FXRSTOR, Page Global, Page Address Extension, and Page Size Extension
    let mut cr4 = controlregs::cr4();
    cr4 |= Cr4::CR4_ENABLE_SSE
        | Cr4::CR4_ENABLE_GLOBAL_PAGES
        | Cr4::CR4_ENABLE_PAE
        | Cr4::CR4_ENABLE_PSE;
    controlregs::cr4_write(cr4);

    // Enable Long mode and NX bit
    let mut efer = msr::rdmsr(msr::IA32_EFER);
    efer |= 1 << 11 | 1 << 8;
    msr::wrmsr(msr::IA32_EFER, efer);

    // Set new page map
    controlregs::cr3_write(page_phys as u64);

    // Enable paging, write protect kernel, protected mode
    let mut cr0 = controlregs::cr0();
    cr0 |= Cr0::CR0_ENABLE_PAGING | Cr0::CR0_WRITE_PROTECT | Cr0::CR0_PROTECTED_MODE;
    controlregs::cr0_write(cr0);

    // Set stack
    asm!("mov rsp, {}", in(reg) stack);

    // Call kernel entry
    let entry_fn: extern "sysv64" fn(*const KernelArgs) -> ! = mem::transmute(func);
    entry_fn(args);
}

pub fn main() -> Result<()> {
    LOGGER.init();

    let mut os = OsEfi::new();

    // Disable cursor
    let _ = (os.st.ConsoleOut.EnableCursor)(os.st.ConsoleOut, false);

    let (page_phys, func, args) = crate::main(&mut os);

    unsafe {
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
        );
    }
}

pub fn disable_interrupts() {
    unsafe {
        asm!("cli");
    }
}
