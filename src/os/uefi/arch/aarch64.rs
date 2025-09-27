use core::{arch::asm, fmt::Write, mem, slice};
use uefi::status::Result;

use crate::{
    KernelArgs,
    arch::{ENTRY_ADDRESS_MASK, PAGE_ENTRIES, PF_PRESENT, PF_TABLE, PHYS_OFFSET},
    logger::LOGGER,
};

use super::super::{OsEfi, memory_map::memory_map};

unsafe fn dump_page_tables(table_phys: u64, table_virt: u64, table_level: u64) {
    unsafe {
        let entries = slice::from_raw_parts(table_phys as *const u64, PAGE_ENTRIES);
        for (i, entry) in entries.iter().enumerate() {
            let phys = entry & ENTRY_ADDRESS_MASK;
            let flags = entry & !ENTRY_ADDRESS_MASK;
            if flags & PF_PRESENT == 0 {
                continue;
            }
            let mut shift = 39u64;
            for _ in 0..table_level {
                shift -= 9;
                print!("\t");
            }
            let virt = table_virt + (i as u64) << shift;
            println!(
                "index {} virt {:#x}: phys {:#x} flags {:#x}",
                i, virt, phys, flags
            );
            if table_level < 3 && flags & PF_TABLE == PF_TABLE {
                dump_page_tables(phys, virt, table_level + 1);
            }
        }
    }
}

unsafe extern "C" fn kernel_entry(
    page_phys: usize,
    stack: u64,
    func: u64,
    args: *const KernelArgs,
) -> ! {
    unsafe {
        // Read memory map and exit boot services
        memory_map().exit_boot_services();

        let currentel: u64;
        asm!(
            "mrs {0}, currentel", // Read current exception level
            out(reg) currentel,
        );
        if currentel == (2 << 2) {
            // Need to drop from EL2 to EL1

            // Allow access to timers
            asm!(
                "mrs {0}, cnthctl_el2",
                "orr {0}, {0}, #0x3",
                "msr cnthctl_el2, {0}",
                "msr cntvoff_el2, xzr",
                out(reg) _
            );

            // Initialize ID registers
            asm!(
                "mrs {0}, midr_el1",
                "msr vpidr_el2, {0}",
                "mrs {0}, mpidr_el1",
                "msr vmpidr_el2, {0}",
                out(reg) _
            );

            // Disable traps
            asm!(
                "msr cptr_el2, {0}",
                "msr hstr_el2, xzr",
                in(reg) 0x33FF as u64
            );

            // Enable floating point
            asm!(
                "msr cpacr_el1, {0}",
                in(reg) (3 << 20) as u64
            );

            // Set EL1 system control register
            asm!(
                "msr sctlr_el1, {0}",
                in(reg) 0x30d00800 as u64
            );

            // Set EL1 stack and VBAR
            asm!(
                "mov {0}, sp",
                "msr sp_el1, {0}",
                "mrs {0}, vbar_el2",
                "msr vbar_el1, {0}",
                out(reg) _
            );

            // Configure execution state of EL1 as aarch64 and disable hypervisor call.
            asm!(
                "msr hcr_el2, {0}",
                in(reg) ((1u64 << 31) | (1u64 << 29)),
            );

            // Set saved program status register
            asm!(
                "msr spsr_el2, {0}",
                in(reg) 0x3C5 as u64
            );

            // Switch to EL1
            asm!(
                "adr {0}, 1f",
                "msr elr_el2, {0}",
                "eret",
                "1:",
                out(reg) _
            );
        } else if currentel == (1 << 2) {
            // Already in EL1
        } else {
            //TODO: what to do if not EL2 or already EL1?
            loop {
                asm!("wfi");
            }
        }

        // Disable MMU
        asm!(
            "mrs {0}, sctlr_el1", // Read system control register
            "bic {0}, {0}, 1", // Clear MMU enable bit
            "msr sctlr_el1, {0}", // Write system control register
            "isb", // Instruction sync barrier
            out(reg) _,
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
            "msr tcr_el1, {0}", // Write translation control register
            "isb", // Instruction sync barrier
            in(reg) 0x1085100510u64, // TCR: (TxSZ, ASID_16, TG1_4K, Cache Attrs, SMP Attrs)
            out(reg) _,
        );

        // Set page tables
        asm!(
            "dsb sy", // Data sync barrier
            "msr ttbr1_el1, {0}", // Set higher half page table
            "msr ttbr0_el1, {0}", // Set lower half page table
            "isb", // Instruction sync barrier
            "dsb ishst", // Data sync barrier, only for stores, and only for inner shareable domain
            "tlbi vmalle1is", // Invalidate TLB
            "dsb ish", // Dta sync bariar, only for inner shareable domain
            "isb", // Instruction sync barrier
            in(reg) page_phys,
        );

        // Enable MMU
        asm!(
            "mrs {2}, sctlr_el1", // Read system control register
            "bic {2}, {2}, {0}", // Clear bits
            "orr {2}, {2}, {1}", // Set bits
            "msr sctlr_el1, {2}", // Write system control register
            "isb", // Instruction sync barrier
            in(reg) 0x32802c2u64,  // Clear SCTLR bits: (EE, EOE, IESB, WXN, UMA, ITD, THEE, A)
            in(reg) 0x3485d13du64, // Set SCTLR bits: (LSMAOE, nTLSMD, UCI, SPAN, nTWW, nTWI, UCT, DZE, I, SED, SA0, SA, C, M, CP15BEN)
            out(reg) _,
        );

        // Set stack
        asm!("mov sp, {}", in(reg) stack);

        // Call kernel entry
        let entry_fn: extern "C" fn(*const KernelArgs) -> ! = mem::transmute(func);
        entry_fn(args);
    }
}

pub fn main() -> Result<()> {
    LOGGER.init();

    let mut os = OsEfi::new();

    // Disable cursor
    let _ = (os.st.ConsoleOut.EnableCursor)(os.st.ConsoleOut, false);

    let currentel: u64;
    unsafe {
        asm!(
            "mrs {0}, currentel", // Read current exception level
            out(reg) currentel,
        );
    }
    log::info!("Currently in EL{}", (currentel >> 2) & 3);

    let (page_phys, func, args) = crate::main(&mut os);

    unsafe {
        let stack = args.stack_base + args.stack_size + PHYS_OFFSET;

        // dump_page_tables(page_phys as _, 0, 0);

        println!(
            "kernel_entry({:#x}, {:#x}, {:#x}, {:p})",
            page_phys, stack, func, &args
        );
        println!("{:#x?}", args);

        kernel_entry(page_phys, stack, func, &args);
    }
}

pub fn disable_interrupts() {
    unsafe {
        asm!("msr daifset, #2");
    }
}
