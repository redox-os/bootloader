use uefi::guid::GuidKind;
use uefi::status::{Error, Result};
use std::{slice, vec::Vec};
use fdt;

use crate::{Disk, Os, OsVideoMode};

pub(crate) static mut RSDPS_AREA_BASE: *mut u8 = 0 as *mut u8;
pub(crate) static mut RSDPS_AREA_SIZE: usize = 0;

pub(crate) fn find_dtb<
    D: Disk,
    V: Iterator<Item=OsVideoMode>
>(os: &mut dyn Os<D, V>) {
    let mut rsdps_area = Vec::new();
    let cfg_tables = std::system_table().config_tables();
    for cfg_table in cfg_tables.iter() {
        if cfg_table.VendorGuid.kind() == GuidKind::DeviceTree {
            let addr = cfg_table.VendorTable as u64;
            println!("DTB: {:X}", addr);
            unsafe {
                if let Ok(fdt) = fdt::Fdt::from_ptr(cfg_table.VendorTable as *const u8) {
                    println!("DTB model = {}", fdt.root().model());
                    let length = fdt.total_size();
                    let address = cfg_table.VendorTable;
                    let align = 8;
                    rsdps_area.extend(unsafe { core::slice::from_raw_parts(address as *const u8, length) });
                    rsdps_area.resize(((rsdps_area.len() + (align - 1)) / align) * align, 0u8);
                    RSDPS_AREA_SIZE = rsdps_area.len();
                    RSDPS_AREA_BASE = os.alloc_zeroed_page_aligned(RSDPS_AREA_SIZE);
                    slice::from_raw_parts_mut(
                        RSDPS_AREA_BASE,
                        RSDPS_AREA_SIZE
                    ).copy_from_slice(&rsdps_area);
                    return ;
                } else {
                    println!("Failed to parser DTB");
                }
            }
        }
    }
    println!("Failed to find DTB");
}
