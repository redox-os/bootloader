use core::slice;
use redoxfs::Disk;

use crate::os::{Os, OsVideoMode};

pub(crate) const PHYS_OFFSET: u64 = 0xfffffe0000000000;

unsafe fn paging_allocate<
    D: Disk,
    V: Iterator<Item=OsVideoMode>
>(os: &mut dyn Os<D, V>) -> Option<&'static mut [u64]> {
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
    V: Iterator<Item=OsVideoMode>
>(os: &mut dyn Os<D, V>, kernel_phys: u64, kernel_base: u64) -> Option<usize> {
    log::error!("paging_create not implemented for aarch64");
    None
}

pub unsafe fn paging_framebuffer<
    D: Disk,
    V: Iterator<Item=OsVideoMode>
>(os: &mut dyn Os<D, V>, page_phys: usize, framebuffer_phys: u64, framebuffer_size: u64) -> Option<()> {
    log::error!("paging_framebuffer not implemented for aarch64");
    None
}
