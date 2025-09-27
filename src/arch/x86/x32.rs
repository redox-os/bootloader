use crate::area_add;
use crate::os::{Os, OsMemoryEntry, OsMemoryKind};
use core::slice;

const PAGE_ENTRIES: usize = 1024;
const PAGE_SIZE: usize = 4096;
pub(crate) const PHYS_OFFSET: u32 = 0x8000_0000;

unsafe fn paging_allocate(os: &impl Os) -> Option<&'static mut [u32]> {
    unsafe {
        let ptr = os.alloc_zeroed_page_aligned(PAGE_SIZE);
        if !ptr.is_null() {
            area_add(OsMemoryEntry {
                base: ptr as u64,
                size: PAGE_SIZE as u64,
                kind: OsMemoryKind::Reclaim,
            });
            Some(slice::from_raw_parts_mut(ptr as *mut u32, PAGE_ENTRIES))
        } else {
            None
        }
    }
}

pub unsafe fn paging_create(os: &impl Os, kernel_phys: u64, kernel_size: u64) -> Option<usize> {
    unsafe {
        let pd = paging_allocate(os)?;
        //Identity map 1 GiB using 4 MiB pages, also map at PHYS_OFFSET
        for pd_i in 0..256 {
            let addr = pd_i as u32 * 0x40_0000;
            pd[pd_i] = addr | 1 << 7 | 1 << 1 | 1;
            pd[pd_i + 512] = addr | 1 << 7 | 1 << 1 | 1;
        }

        // Map kernel_size at kernel offset
        let mut kernel_mapped = 0;
        let mut pd_i = 0xC000_0000 / 0x40_0000;
        while kernel_mapped < kernel_size && pd_i < pd.len() {
            let pt = paging_allocate(os)?;
            pd[pd_i] = pt.as_ptr() as u32 | 1 << 1 | 1;
            pd_i += 1;

            let mut pt_i = 0;
            while kernel_mapped < kernel_size && pt_i < pt.len() {
                let addr = kernel_phys + kernel_mapped;
                pt[pt_i] = addr as u32 | 1 << 1 | 1;
                pt_i += 1;
                kernel_mapped += PAGE_SIZE as u64;
            }
        }
        assert!(kernel_mapped >= kernel_size);

        Some(pd.as_ptr() as usize)
    }
}

pub unsafe fn paging_framebuffer(
    os: &impl Os,
    page_phys: usize,
    framebuffer_phys: u64,
    framebuffer_size: u64,
) -> Option<u64> {
    unsafe {
        let framebuffer_virt = 0xD000_0000; // 256 MiB after kernel mapping, but before heap mapping

        let pd = slice::from_raw_parts_mut(page_phys as *mut u32, PAGE_ENTRIES);

        // Map framebuffer_size at framebuffer offset
        let mut framebuffer_mapped = 0;
        let mut pd_i = framebuffer_virt / 0x40_0000;
        while framebuffer_mapped < framebuffer_size && pd_i < pd.len() {
            let pt = paging_allocate(os)?;
            pd[pd_i] = pt.as_ptr() as u32 | 1 << 1 | 1;
            pd_i += 1;

            let mut pt_i = 0;
            while framebuffer_mapped < framebuffer_size && pt_i < pt.len() {
                let addr = framebuffer_phys + framebuffer_mapped;
                pt[pt_i] = addr as u32 | 1 << 1 | 1;
                pt_i += 1;
                framebuffer_mapped += PAGE_SIZE as u64;
            }
        }
        assert!(framebuffer_mapped >= framebuffer_size);

        Some(framebuffer_virt as u64)
    }
}
