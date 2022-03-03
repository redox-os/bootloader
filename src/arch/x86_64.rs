use core::slice;
use redoxfs::Disk;

use crate::os::{Os, OsVideoMode};

pub(crate) const ENTRY_ADDRESS_MASK: u64 = 0x000F_FFFF_FFFF_F000;
pub(crate) const PAGE_ENTRIES: usize = 512;
pub(crate) const PAGE_SIZE: usize = 4096;
pub(crate) const PHYS_OFFSET: u64 = 0xFFFF_8000_0000_0000;

unsafe fn paging_allocate<
    D: Disk,
    V: Iterator<Item=OsVideoMode>
>(os: &mut dyn Os<D, V>) -> Option<&'static mut [u64]> {
    let ptr = os.alloc_zeroed_page_aligned(PAGE_SIZE);
    if ! ptr.is_null() {
        Some(slice::from_raw_parts_mut(
            ptr as *mut u64,
            PAGE_ENTRIES
        ))
    } else {
        None
    }
}

pub unsafe fn paging_create<
    D: Disk,
    V: Iterator<Item=OsVideoMode>
>(os: &mut dyn Os<D, V>, kernel_phys: u64, kernel_size: u64) -> Option<usize> {
    // Create PML4
    let pml4 = paging_allocate(os)?;

    {
        // Create PDP for identity mapping
        let pdp = paging_allocate(os)?;

        // Link first user and first kernel PML4 entry to PDP
        pml4[0] = pdp.as_ptr() as u64 | 1 << 1 | 1;
        pml4[256] = pdp.as_ptr() as u64 | 1 << 1 | 1;

        // Identity map 8 GiB using 2 MiB pages
        for pdp_i in 0..8 {
            let pd = paging_allocate(os)?;
            pdp[pdp_i] = pd.as_ptr() as u64 | 1 << 1 | 1;
            for pd_i in 0..pd.len() {
                let addr =
                    pdp_i as u64 * 0x4000_0000 +
                    pd_i as u64 * 0x20_0000;
                pd[pd_i] = addr | 1 << 7 | 1 << 1 | 1;
            }
        }
    }

    {
        // Create PDP for kernel mapping
        let pdp = paging_allocate(os)?;

        // Link second to last PML4 entry to PDP
        pml4[510] = pdp.as_ptr() as u64 | 1 << 1 | 1;

        // Map kernel_size at kernel offset
        let mut kernel_mapped = 0;
        let mut pdp_i = 0;
        while kernel_mapped < kernel_size && pdp_i < pdp.len() {
            let pd = paging_allocate(os)?;
            pdp[pdp_i] = pd.as_ptr() as u64 | 1 << 1 | 1;
            pdp_i += 1;

            let mut pd_i = 0;
            while kernel_mapped < kernel_size && pd_i < pd.len(){
                let pt = paging_allocate(os)?;
                pd[pd_i] = pt.as_ptr() as u64 | 1 << 1 | 1;
                pd_i += 1;

                let mut pt_i = 0;
                while kernel_mapped < kernel_size && pt_i < pt.len() {
                    let addr = kernel_phys + kernel_mapped;
                    pt[pt_i] = addr | 1 << 1 | 1;
                    pt_i += 1;
                    kernel_mapped += PAGE_SIZE as u64;
                }
            }
        }
        assert!(kernel_mapped >= kernel_size);
    }

    Some(pml4.as_ptr() as usize)
}

pub unsafe fn paging_framebuffer<
    D: Disk,
    V: Iterator<Item=OsVideoMode>
>(os: &mut dyn Os<D, V>, page_phys: usize, framebuffer_phys: u64, framebuffer_size: u64) -> Option<()> {
    //TODO: smarter test for framebuffer already mapped
    if framebuffer_phys + framebuffer_size <= 0x2_0000_0000 {
        return Some(());
    }

    let pml4_i = ((framebuffer_phys / 0x80_0000_0000) + 256) as usize;
    let mut pdp_i = ((framebuffer_phys % 0x80_0000_0000) / 0x4000_0000) as usize;
    let mut pd_i = ((framebuffer_phys % 0x4000_0000) / 0x20_0000) as usize;
    assert_eq!(framebuffer_phys % 0x20_0000, 0);

    let pml4 = slice::from_raw_parts_mut(
        page_phys as *mut u64,
        PAGE_ENTRIES
    );

    // Create PDP for framebuffer mapping
    let pdp = if pml4[pml4_i] == 0 {
        let pdp = paging_allocate(os)?;
        pml4[pml4_i] = pdp.as_ptr() as u64 | 1 << 1 | 1;
        pdp
    } else {
        slice::from_raw_parts_mut(
            (pml4[pml4_i] & ENTRY_ADDRESS_MASK) as *mut u64,
            PAGE_ENTRIES
        )
    };

    // Map framebuffer_size at framebuffer offset
    let mut framebuffer_mapped = 0;
    while framebuffer_mapped < framebuffer_size && pdp_i < pdp.len() {
        let pd = paging_allocate(os)?;
        assert_eq!(pdp[pdp_i], 0);
        pdp[pdp_i] = pd.as_ptr() as u64 | 1 << 1 | 1;

        while framebuffer_mapped < framebuffer_size && pd_i < pd.len() {
            let addr = framebuffer_phys + framebuffer_mapped;
            assert_eq!(pd[pd_i], 0);
            pd[pd_i] = addr | 1 << 7 | 1 << 1 | 1;
            framebuffer_mapped += 0x20_0000;
            pd_i += 1;
        }

        pdp_i += 1;
        pd_i = 0;
    }
    assert!(framebuffer_mapped >= framebuffer_size);

    Some(())
}
