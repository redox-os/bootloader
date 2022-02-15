use core::slice;
use redoxfs::Disk;

use crate::os::{Os, OsMemoryEntry, OsVideoMode};

unsafe fn paging_allocate<
    D: Disk,
    M: Iterator<Item=OsMemoryEntry>,
    V: Iterator<Item=OsVideoMode>
>(os: &mut dyn Os<D, M, V>) -> Option<&'static mut [u64]> {
    let ptr = os.alloc_zeroed_page_aligned(4096);
    if ! ptr.is_null() {
        Some(slice::from_raw_parts_mut(
            ptr as *mut u64,
            512 // page size divided by u64 size
        ))
    } else {
        None
    }
}

pub unsafe fn paging_create<
    D: Disk,
    M: Iterator<Item=OsMemoryEntry>,
    V: Iterator<Item=OsVideoMode>
>(os: &mut dyn Os<D, M, V>, kernel_phys: usize, kernel_base: usize) -> Option<usize> {
    log::error!("paging_create not implemented for aarch64");
    None
}
