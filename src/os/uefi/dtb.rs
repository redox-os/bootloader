use crate::{Disk, Os, OsVideoMode};
use alloc::vec::Vec;
use byteorder::ByteOrder;
use byteorder::BE;
use core::slice;
use fdt::Fdt;
use uefi::guid::DEVICE_TREE_GUID;
#[cfg(target_arch = "aarch64")]
use uefi::{
    guid::SMBIOS3_TABLE_GUID,
    status::{Result, Status},
};

pub static mut DEV_MEM_AREA: Vec<(usize, usize)> = Vec::new();

pub unsafe fn is_in_dev_mem_region(addr: usize) -> bool {
    if DEV_MEM_AREA.is_empty() {
        return false;
    }
    for item in DEV_MEM_AREA.iter() {
        if (addr >= item.0) && (addr < item.0 + item.1) {
            return true;
        }
    }
    return false;
}

unsafe fn get_dev_mem_region(fdt: &Fdt) {
    let Some(soc) = fdt.find_node("/soc") else {
        return;
    };
    let Some(ranges) = soc.property("ranges") else {
        return;
    };
    let cell_sizes = soc.cell_sizes();
    let chunk_size = (cell_sizes.address_cells * 2 + cell_sizes.size_cells) * 4;
    for chunk in ranges.value.chunks(chunk_size) {
        let child_bus_addr = {
            if cell_sizes.address_cells == 1 {
                BE::read_u32(&chunk[0..4]) as u64
            } else if cell_sizes.address_cells == 2 {
                BE::read_u32(&chunk[0..8]) as u64
            } else {
                DEV_MEM_AREA.clear();
                return;
            }
        };
        let parent_bus_addr = {
            if cell_sizes.address_cells == 1 {
                BE::read_u32(&chunk[4..8]) as u64
            } else if cell_sizes.address_cells == 2 {
                BE::read_u32(&chunk[8..16]) as u64
            } else {
                DEV_MEM_AREA.clear();
                return;
            }
        };
        let addr_size = {
            if cell_sizes.size_cells == 1 {
                BE::read_u32(&chunk[8..12]) as u64
            } else if cell_sizes.size_cells == 2 {
                BE::read_u32(&chunk[16..20]) as u64
            } else {
                DEV_MEM_AREA.clear();
                return;
            }
        };
        println!(
            "dev mem 0x{:08x} 0x{:08x} 0x{:08x}",
            child_bus_addr, parent_bus_addr, addr_size
        );
        DEV_MEM_AREA.push((parent_bus_addr as usize, addr_size as usize));
    }
}

fn parse_dtb<D: Disk, V: Iterator<Item = OsVideoMode>>(
    os: &dyn Os<D, V>,
    address: *const u8,
) -> Option<(u64, u64)> {
    unsafe {
        if let Ok(fdt) = fdt::Fdt::from_ptr(address) {
            let mut rsdps_area = Vec::new();
            //println!("DTB model = {}", fdt.root().model());
            get_dev_mem_region(&fdt);
            let length = fdt.total_size();
            let align = 8;
            rsdps_area.extend(core::slice::from_raw_parts(address, length));
            rsdps_area.resize(((rsdps_area.len() + (align - 1)) / align) * align, 0u8);
            let size = rsdps_area.len();
            let base = os.alloc_zeroed_page_aligned(size);
            slice::from_raw_parts_mut(base, size).copy_from_slice(&rsdps_area);
            Some((base as u64, size as u64))
        } else {
            println!("Failed to parse DTB");
            None
        }
    }
}

#[cfg(target_arch = "aarch64")]
fn find_smbios3_system(address: *const u8) -> Result<dmidecode::System<'static>> {
    unsafe {
        let smb = core::slice::from_raw_parts(address, 24);
        if let Ok(smbios) = dmidecode::EntryPoint::search(smb) {
            let smb_structure_data = core::slice::from_raw_parts(
                smbios.smbios_address() as *const u8,
                smbios.smbios_len() as usize,
            );
            for structure in smbios.structures(smb_structure_data) {
                if let Ok(sval) = structure {
                    //println!("SMBIOS: {:#?}", sval);
                    if let dmidecode::Structure::System(buf) = sval {
                        return Ok(buf);
                    }
                }
            }
        }
    }
    Err(Status::NOT_FOUND)
}

pub(crate) fn find_dtb<D: Disk, V: Iterator<Item = OsVideoMode>>(
    os: &dyn Os<D, V>,
) -> Option<(u64, u64)> {
    let cfg_tables = std::system_table().config_tables();
    for cfg_table in cfg_tables.iter() {
        if cfg_table.VendorGuid == DEVICE_TREE_GUID {
            let addr = cfg_table.VendorTable;
            return parse_dtb(os, addr as *const u8);
        }
    }

    /* This hack is no longer needed, but can be re-enabled for testing
    #[cfg(target_arch = "aarch64")]
    for cfg_table in cfg_tables.iter() {
        if cfg_table.VendorGuid == SMBIOS3_TABLE_GUID {
            let addr = cfg_table.VendorTable;
            if let Ok(sys) = find_smbios3_system(addr as *const u8) {
                let get_dtb_addr = match (sys.manufacturer, sys.version) {
                    ("QEMU", version) if version.starts_with("virt") => Some(0x4000_0000 as usize),
                    _ => None,
                };
                if let Some(dtb_addr) = get_dtb_addr {
                    return parse_dtb(os, dtb_addr as *const u8);
                }
            }
        }
    }
    */

    None
}
