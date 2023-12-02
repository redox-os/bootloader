use byteorder::BE;
use byteorder::ByteOrder;
use uefi::guid::GuidKind;
use uefi::status::{Error, Result};
use std::{slice, vec::Vec};
use fdt;

use crate::{Disk, Os, OsVideoMode};

pub(crate) static mut RSDPS_AREA_BASE: *mut u8 = 0 as *mut u8;
pub(crate) static mut RSDPS_AREA_SIZE: usize = 0;

pub static mut DEV_MEM_AREA: Vec<(usize, usize)> = Vec::new();

pub unsafe fn is_in_dev_mem_region(addr: usize) -> bool {
    if DEV_MEM_AREA.is_empty() {
        return false;
    }
    for item in &DEV_MEM_AREA {
        if (addr >= item.0) && (addr < item.0 + item.1) {
            return true;
        }
    }
    return false;
}

unsafe fn get_dev_mem_region(fdt: &fdt::Fdt) {
    let soc = fdt.find_node("/soc");
    let cell_sizes = fdt.root().cell_sizes();
    let chunk_size = (cell_sizes.address_cells * 2 + cell_sizes.size_cells) * 4;
    if let Some(soc) = soc {
        if let Some(ranges) = soc.property("ranges") {
            for chunk in ranges.value.chunks(chunk_size) {
                let child_bus_addr = {
                    if cell_sizes.address_cells == 1 {
                        BE::read_u32(&chunk[0..4]) as u64
                    } else if cell_sizes.address_cells == 2 {
                        BE::read_u32(&chunk[0..8]) as u64
                    } else {
                        DEV_MEM_AREA.clear();
                        return ;
                    }
                };
                let parent_bus_addr = {
                    if cell_sizes.address_cells == 1 {
                        BE::read_u32(&chunk[4..8]) as u64
                    } else if cell_sizes.address_cells == 2 {
                        BE::read_u32(&chunk[8..16]) as u64
                    } else {
                        DEV_MEM_AREA.clear();
                        return ;
                    }
                };
                let addr_size = {
                    if cell_sizes.address_cells == 1 {
                        BE::read_u32(&chunk[8..12]) as u64
                    } else if cell_sizes.address_cells == 2 {
                        BE::read_u32(&chunk[16..24]) as u64
                    } else {
                        DEV_MEM_AREA.clear();
                        return ;
                    }
                };
                println!("dev mem 0x{:08x} 0x{:08x} 0x{:08x}", child_bus_addr,
                         parent_bus_addr, addr_size);
                DEV_MEM_AREA.push((parent_bus_addr as usize, addr_size as usize));
            }
        }
    }
}

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
                    //println!("DTB model = {}", fdt.root().model());
                    get_dev_mem_region(&fdt);
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
