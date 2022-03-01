use std::{slice, vec::Vec};
use uefi::guid::GuidKind;

use crate::{Disk, Os, OsVideoMode};

pub(crate) static mut RSDPS_AREA_BASE: *mut u8 = 0 as *mut u8;
pub(crate) static mut RSDPS_AREA_SIZE: usize = 0;

struct Invalid;

fn validate_rsdp(address: usize, v2: bool) -> core::result::Result<usize, Invalid> {
    #[repr(packed)]
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
    let rsdp_bytes = unsafe { core::slice::from_raw_parts(address as *const u8, core::mem::size_of::<Rsdp>()) };
    let rsdp = unsafe { (rsdp_bytes.as_ptr() as *const Rsdp).as_ref::<'static>().unwrap() };

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

    let length = if rsdp.revision == 2 { rsdp.length as usize } else { core::mem::size_of::<Rsdp>() };

    Ok(length)
}

pub(crate) fn find_acpi_table_pointers<
    D: Disk,
    V: Iterator<Item=OsVideoMode>
>(os: &mut dyn Os<D, V>) {
    let mut rsdps_area = Vec::new();

    let cfg_tables = std::system_table().config_tables();

    for (address, v2) in cfg_tables.iter().find_map(|cfg_table| {
        if cfg_table.VendorGuid.kind() == GuidKind::Acpi {
            Some((cfg_table.VendorTable, false))
        } else if cfg_table.VendorGuid.kind() == GuidKind::Acpi2 {
            Some((cfg_table.VendorTable, true))
        } else {
            None
        }
    }) {
        match validate_rsdp(address, v2) {
            Ok(length) => {
                let align = 8;

                rsdps_area.extend(&u32::to_ne_bytes(length as u32));
                rsdps_area.extend(unsafe { core::slice::from_raw_parts(address as *const u8, length) });
                rsdps_area.resize(((rsdps_area.len() + (align - 1)) / align) * align, 0u8);
            }
            Err(_) => log::warn!("Found RSDP that was not valid at {:p}", address as *const u8),
        }
    }

    if ! rsdps_area.is_empty() {
        unsafe {
            // Copy to page aligned area
            RSDPS_AREA_SIZE = rsdps_area.len();
            RSDPS_AREA_BASE = os.alloc_zeroed_page_aligned(RSDPS_AREA_SIZE);
            slice::from_raw_parts_mut(
                RSDPS_AREA_BASE,
                RSDPS_AREA_SIZE
            ).copy_from_slice(&rsdps_area);
        }
    }
}
