use crate::os::Os;

pub(crate) mod x32;
pub(crate) mod x64;

pub unsafe fn paging_create(os: &impl Os, kernel_phys: u64, kernel_size: u64) -> Option<usize> {
    unsafe {
        if crate::KERNEL_64BIT {
            x64::paging_create(os, kernel_phys, kernel_size)
        } else {
            x32::paging_create(os, kernel_phys, kernel_size)
        }
    }
}

pub unsafe fn paging_framebuffer(
    os: &impl Os,
    page_phys: usize,
    framebuffer_phys: u64,
    framebuffer_size: u64,
) -> Option<u64> {
    unsafe {
        if crate::KERNEL_64BIT {
            x64::paging_framebuffer(os, page_phys, framebuffer_phys, framebuffer_size)
        } else {
            x32::paging_framebuffer(os, page_phys, framebuffer_phys, framebuffer_size)
        }
    }
}
