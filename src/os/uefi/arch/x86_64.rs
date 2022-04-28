use core::{arch::asm, mem, ptr};
use uefi::status::Result;
use x86::{
    controlregs::{self, Cr0, Cr4},
    msr,
};

use crate::{
    KernelArgs,
    Os,
    arch::PHYS_OFFSET,
    logger::LOGGER,
};

use super::super::{
    OsEfi,
    acpi::{
        RSDPS_AREA_BASE,
        RSDPS_AREA_SIZE,
        find_acpi_table_pointers,
    },
    memory_map::memory_map,
};

unsafe extern "C" fn kernel_entry(
    page_phys: usize,
    stack: u64,
    func: u64,
    args: *const KernelArgs,
) -> ! {
    // Read memory map and exit boot services
    {
        let mut memory_iter = memory_map();
        memory_iter.exit_boot_services();
        memory_iter.set_virtual_address_map(PHYS_OFFSET);
        mem::forget(memory_iter);
    }

    // Disable interrupts
    asm!("cli");

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
    cr0 |= Cr0::CR0_ENABLE_PAGING
        | Cr0::CR0_WRITE_PROTECT
        | Cr0::CR0_PROTECTED_MODE;
    controlregs::cr0_write(cr0);

    // Set stack
    asm!("mov rsp, {}", in(reg) stack);

    // Call kernel entry
    let entry_fn: extern "sysv64" fn(*const KernelArgs) -> ! = mem::transmute(func);
    entry_fn(args);
}

pub fn main() -> Result<()> {
    LOGGER.init();

    let mut os = OsEfi {
        st: std::system_table(),
    };

    // Disable cursor
    let _ = (os.st.ConsoleOut.EnableCursor)(os.st.ConsoleOut, false);

    find_acpi_table_pointers(&mut os);

    let (page_phys, mut args) = crate::main(&mut os);

    unsafe {
        args.acpi_rsdps_base = RSDPS_AREA_BASE as u64;
        args.acpi_rsdps_size = RSDPS_AREA_SIZE as u64;

        kernel_entry(
            page_phys,
            args.stack_base + args.stack_size + PHYS_OFFSET,
            ptr::read((args.kernel_base + 0x18) as *const u64),
            &args,
        );
    }
}
