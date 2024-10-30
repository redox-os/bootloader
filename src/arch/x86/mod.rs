use redoxfs::Disk;

use crate::os::{Os, OsVideoMode};

pub(crate) mod x32;
pub(crate) mod x64;

pub unsafe fn paging_create<D: Disk, V: Iterator<Item = OsVideoMode>>(
    os: &dyn Os<D, V>,
    kernel_phys: u64,
    kernel_size: u64,
) -> Option<usize> {
    if crate::KERNEL_64BIT {
        x64::paging_create(os, kernel_phys, kernel_size)
    } else {
        x32::paging_create(os, kernel_phys, kernel_size)
    }
}

pub unsafe fn paging_framebuffer<D: Disk, V: Iterator<Item = OsVideoMode>>(
    os: &dyn Os<D, V>,
    page_phys: usize,
    framebuffer_phys: u64,
    framebuffer_size: u64,
) -> Option<u64> {
    if crate::KERNEL_64BIT {
        x64::paging_framebuffer(os, page_phys, framebuffer_phys, framebuffer_size)
    } else {
        x32::paging_framebuffer(os, page_phys, framebuffer_phys, framebuffer_size)
    }
}
