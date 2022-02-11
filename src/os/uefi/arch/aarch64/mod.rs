use core::{mem, ptr};
use uefi::guid::Guid;
use uefi::status::{Error, Result};

use crate::{
    KernelArgs,
    logger::LOGGER,
};

use super::super::{
    OsEfi,
};

use self::memory_map::memory_map;
use self::paging::paging;

mod memory_map;
mod paging;

static PHYS_OFFSET: u64 = 0xFFFF800000000000;

static mut DTB_PHYSICAL: u64 = 0;

#[no_mangle]
pub extern "C" fn __chkstk() {
    //TODO
}

unsafe fn exit_boot_services(key: usize) {
    let handle = std::handle();
    let uefi = std::system_table();

    let _ = (uefi.BootServices.ExitBootServices)(handle, key);
}

static DTB_GUID: Guid = Guid(0xb1b621d5, 0xf19c, 0x41a5, [0x83, 0x0b, 0xd9, 0x15, 0x2c, 0x69, 0xaa, 0xe0]);

fn find_dtb() -> Result<()> {
    let cfg_tables = std::system_table().config_tables();
    for cfg_table in cfg_tables.iter() {
        if cfg_table.VendorGuid == DTB_GUID {
            unsafe {
                DTB_PHYSICAL = cfg_table.VendorTable as u64;
                println!("DTB: {:X}", DTB_PHYSICAL);
            }
            return Ok(());
        }
    }
    println!("Failed to find DTB");
    Err(Error::NotFound)
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

    // Enable paging
    asm!("msr daifset, #2");
    paging();

    // Call kernel entry
    let entry_fn: extern "C" fn(dtb: u64) -> ! = mem::transmute(func);
    entry_fn(DTB_PHYSICAL);
}

pub fn main() -> Result<()> {
    LOGGER.init();

    find_dtb()?;

    let mut os = OsEfi {
        st: std::system_table(),
    };

    let (page_phys, args) = crate::main(&mut os);

    unsafe {
        kernel_entry(
            page_phys,
            args.stack_base + args.stack_size + PHYS_OFFSET,
            ptr::read((args.kernel_base + 0x18) as *const u64),
            &args,
        );
    }
}
