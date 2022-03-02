use core::{mem, ptr};
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
        "mrs     x0, sctlr_el1",
        "bic     x0, x0, 1",
        "msr     sctlr_el1, x0",
        "isb",
        lateout("x0") _
    );

    //TODO: Set stack

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
