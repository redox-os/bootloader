use core::{arch::asm, mem, ptr};
use uefi::status::Result;

use crate::{
    KernelArgs,
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

#[no_mangle]
pub extern "C" fn __chkstk() {
    //TODO
}

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
    asm!("msr daifset, #2");

    // Disable MMU
    asm!(
        "mrs {0}, sctlr_el1", // Read system control register
        "bic {0}, {0}, 1", // Clear MMU enable bit
        "msr sctlr_el1, {0}", // Write system control register
        "isb", // Instruction sync barrier
        out(reg) _,
    );

    // Set page tables
    asm!(
        "dsb sy", // Data sync barrier
        "msr ttbr1_el1, {0}", // Set higher half page table
        "isb", // Instruction sync barrier
        "tlbi vmalle1is", // Invalidate TLB
        in(reg) page_phys,
    );

    // Set MAIR
    asm!(
        "msr mair_el1, {0}",
        in(reg) 0xff4400, // MAIR: Arrange for Device, Normal Non-Cache, Normal Write-Back access types
    );

    // Set TCR
    asm!(
        "mrs {1}, id_aa64mmfr0_el1", // Read memory model feature register
        "bfi {0}, {1}, #32, #3",
        "msr tcr_el1, {0}", // Write translaction control register
        "isb", // Instruction sync barrier
        in(reg) 0x1085100510u64, // TCR: (TxSZ, ASID_16, TG1_4K, Cache Attrs, SMP Attrs)
        out(reg) _,
    );

    // Enable MMU
    asm!(
        "mrs {2}, sctlr_el1", // Read system control register
        "bic {2}, {2}, {0}", // Clear bits
        "orr {2}, {2}, {1}", // Set bits
        "msr sctlr_el1, {2}", // Write system control register
        "isb", // Instruction sync barrier
        in(reg) 0x32802c2,  // Clear SCTLR bits: (EE, EOE, IESB, WXN, UMA, ITD, THEE, A)
        in(reg) 0x3485d13d, // Set SCTLR bits: (LSMAOE, nTLSMD, UCI, SPAN, nTWW, nTWI, UCT, DZE, I, SED, SA0, SA, C, M, CP15BEN)
        out(reg) _,
    );

    // Set stack
    asm!("mov sp, {}", in(reg) stack);

    // Call kernel entry
    let entry_fn: extern "C" fn(*const KernelArgs) -> ! = mem::transmute(func);
    entry_fn(args);
}

pub fn main() -> Result<()> {
    LOGGER.init();

    //TODO: support this in addition to ACPI?
    // let dtb = find_dtb()?;

    let mut os = OsEfi {
        st: std::system_table(),
    };

    // Disable cursor
    let _ = (os.st.ConsoleOut.EnableCursor)(os.st.ConsoleOut, false);

    find_acpi_table_pointers(&mut os);

    let (page_phys, func, mut args) = crate::main(&mut os);

    unsafe {
        args.acpi_rsdps_base = RSDPS_AREA_BASE as u64;
        args.acpi_rsdps_size = RSDPS_AREA_SIZE as u64;

        kernel_entry(
            page_phys,
            args.stack_base + args.stack_size + PHYS_OFFSET,
            func,
            &args,
        );
    }
}
