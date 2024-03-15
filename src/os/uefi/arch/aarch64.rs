use core::{arch::asm, mem, ptr};
use uefi::status::Result;

use crate::{
    KernelArgs,
    arch::PHYS_OFFSET,
    logger::LOGGER,
};

use super::super::{
    OsEfi,
    dtb::{
        RSDP_AREA_BASE,
        RSDP_AREA_SIZE,
        find_dtb,
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
        "msr ttbr0_el1, {0}", // Set lower half page table
        "isb", // Instruction sync barrier
        "tlbi vmalle1is", // Invalidate TLB
        in(reg) page_phys,
    );

    // Set MAIR
    // You can think about MAIRs as of an array with 8 elements each of 8 bits long.
    // You can store inside MAIRs up to 8 attributes sets and reffer them by the index 0..7 stored in INDX (AttrIndx) field of the table descriptor.
    // https://lowenware.com/blog/aarch64-mmu-programming/
    // https://developer.arm.com/documentation/102376/0200/Describing-memory-in-AArch64
    // https://developer.arm.com/documentation/ddi0595/2021-06/AArch64-Registers/MAIR-EL1--Memory-Attribute-Indirection-Register--EL1-
    // Attribute 0 (0xFF) - normal memory, caches are enabled
    // Attribute 1 (0x44) - normal memory, caches are disabled. Atomics wouldn't work here if memory doesn't support exclusive access (most real hardware don't)
    // Attribute 2 (0x00) - nGnRnE device memory, caches are disabled, gathering, re-ordering, and early write acknowledgement aren't allowed.
    asm!(
        "msr mair_el1, {0}",
        in(reg) 0x00000000000044FF as u64, // MAIR: Arrange for Device, Normal Non-Cache, Normal Write-Back access types
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

    let mut os = OsEfi::new();

    // Disable cursor
    let _ = (os.st.ConsoleOut.EnableCursor)(os.st.ConsoleOut, false);

    find_dtb(&mut os);

    let (page_phys, func, mut args) = crate::main(&mut os);

    unsafe {
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
