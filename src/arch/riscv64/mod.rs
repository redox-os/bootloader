use core::slice;

use crate::area_add;
use crate::os::{Os, OsMemoryEntry, OsMemoryKind};

pub(crate) mod sv39;
pub(crate) mod sv48;
pub(crate) mod sv57;

// Common constants
const PAGE_SHIFT: usize = 12;
const TABLE_SHIFT: usize = 9;
const TABLE_MASK: usize = (1 << TABLE_SHIFT) - 1;
const PAGE_ENTRIES: usize = 512;
const PAGE_SIZE: usize = 4096;
const PHYS_MASK: usize = (1usize << 44) - 1;

const VALID: u64 = 1;
const RWX: u64 = 7 << 1;
const ACCESSED: u64 = 1 << 6;
const DIRTY: u64 = 1 << 7;

extern crate alloc;

pub(crate) use sv39::PHYS_OFFSET;
pub(crate) use sv39::SATP_BITS;
pub(crate) use sv39::paging_create;
pub(crate) use sv39::paging_physmem as paging_framebuffer;

unsafe fn paging_allocate(os: &impl Os) -> Option<&'static mut [u64]> {
    unsafe {
        let ptr = os.alloc_zeroed_page_aligned(PAGE_SIZE);
        if !ptr.is_null() {
            area_add(OsMemoryEntry {
                base: ptr as u64,
                size: PAGE_SIZE as u64,
                kind: OsMemoryKind::Reclaim,
            });
            Some(slice::from_raw_parts_mut(ptr as *mut u64, PAGE_ENTRIES))
        } else {
            None
        }
    }
}

unsafe fn get_table(os: &impl Os, parent: &mut [u64], index: usize) -> Option<&'static mut [u64]> {
    unsafe {
        if parent[index] == 0 {
            let table = paging_allocate(os)?;
            parent[index] = table.as_ptr() as u64 >> 2 | VALID;
            Some(table)
        } else {
            Some(slice::from_raw_parts_mut(
                (((parent[index] >> 10) & PHYS_MASK as u64) << 12) as *mut u64,
                PAGE_ENTRIES,
            ))
        }
    }
}
