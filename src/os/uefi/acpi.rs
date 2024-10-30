use core::slice;
use uefi::guid::{ACPI_20_TABLE_GUID, ACPI_TABLE_GUID};

use crate::{Disk, Os, OsVideoMode};

struct Invalid;

fn validate_rsdp(address: usize, _v2: bool) -> core::result::Result<usize, Invalid> {
    #[repr(C, packed)]
    #[derive(Clone, Copy, Debug)]
    struct Rsdp {
        signature: [u8; 8], // b"RSD PTR "
        chksum: u8,
        oem_id: [u8; 6],
        revision: u8,
        rsdt_addr: u32,
        // the following fields are only available for ACPI 2.0, and are reserved otherwise
        length: u32,
        xsdt_addr: u64,
        extended_chksum: u8,
        _rsvd: [u8; 3],
    }
    // paging is not enabled at this stage; we can just read the physical address here.
    let rsdp_bytes =
        unsafe { core::slice::from_raw_parts(address as *const u8, core::mem::size_of::<Rsdp>()) };
    let rsdp = unsafe {
        (rsdp_bytes.as_ptr() as *const Rsdp)
            .as_ref::<'static>()
            .unwrap()
    };

    log::debug!("RSDP: {:?}", rsdp);

    if rsdp.signature != *b"RSD PTR " {
        return Err(Invalid);
    }
    let mut base_sum = 0u8;
    for base_byte in &rsdp_bytes[..20] {
        base_sum = base_sum.wrapping_add(*base_byte);
    }
    if base_sum != 0 {
        return Err(Invalid);
    }

    if rsdp.revision == 2 {
        let mut extended_sum = 0u8;
        for byte in rsdp_bytes {
            extended_sum = extended_sum.wrapping_add(*byte);
        }

        if extended_sum != 0 {
            return Err(Invalid);
        }
    }

    let length = if rsdp.revision == 2 {
        rsdp.length as usize
    } else {
        core::mem::size_of::<Rsdp>()
    };

    Ok(length)
}

pub(crate) fn find_acpi_table_pointers<D: Disk, V: Iterator<Item = OsVideoMode>>(
    os: &dyn Os<D, V>,
) -> Option<(u64, u64)> {
    let cfg_tables = std::system_table().config_tables();
    let mut acpi = None;
    let mut acpi2 = None;
    for cfg_table in cfg_tables.iter() {
        if cfg_table.VendorGuid == ACPI_TABLE_GUID {
            match validate_rsdp(cfg_table.VendorTable, false) {
                Ok(length) => {
                    acpi = Some(unsafe {
                        core::slice::from_raw_parts(cfg_table.VendorTable as *const u8, length)
                    });
                }
                Err(_) => log::warn!(
                    "Found RSDP that was not valid at {:p}",
                    cfg_table.VendorTable as *const u8
                ),
            }
        } else if cfg_table.VendorGuid == ACPI_20_TABLE_GUID {
            match validate_rsdp(cfg_table.VendorTable, true) {
                Ok(length) => {
                    acpi2 = Some(unsafe {
                        core::slice::from_raw_parts(cfg_table.VendorTable as *const u8, length)
                    });
                }
                Err(_) => log::warn!(
                    "Found RSDP that was not valid at {:p}",
                    cfg_table.VendorTable as *const u8
                ),
            }
        }
    }

    let rsdp_area = acpi2.or(acpi).unwrap_or(&[]);

    if !rsdp_area.is_empty() {
        unsafe {
            // Copy to page aligned area
            let size = rsdp_area.len();
            let base = os.alloc_zeroed_page_aligned(size);
            slice::from_raw_parts_mut(base, size).copy_from_slice(&rsdp_area);
            Some((base as u64, size as u64))
        }
    } else {
        None
    }
}
