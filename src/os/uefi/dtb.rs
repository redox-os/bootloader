use crate::Os;
use alloc::vec::Vec;
use byteorder::BE;
use byteorder::ByteOrder;
use core::slice;
use uefi::guid::DEVICE_TREE_GUID;
#[cfg(target_arch = "aarch64")]
use uefi::{
    guid::SMBIOS3_TABLE_GUID,
    status::{Result, Status},
};

fn parse_dtb(os: &impl Os, address: *const u8) -> Option<(u64, u64)> {
    unsafe {
        if let Ok(fdt) = fdt::Fdt::from_ptr(address) {
            let mut rsdps_area = Vec::new();
            //println!("DTB model = {}", fdt.root().model());
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

pub(crate) fn find_dtb(os: &impl Os) -> Option<(u64, u64)> {
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
