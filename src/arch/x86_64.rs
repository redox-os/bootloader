use core::slice;
use redoxfs::Disk;

use crate::os::{Os, OsVideoMode};

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
>(os: &mut dyn Os<D, V>, kernel_phys: usize, kernel_size: usize) -> Option<usize> {
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
                    let addr = (kernel_phys + kernel_mapped) as u64;
                    pt[pt_i] = addr | 1 << 1 | 1;
                    pt_i += 1;
                    kernel_mapped += 4096;
                }
            }
        }
        assert!(kernel_mapped >= kernel_size);
    }

    Some(pml4.as_ptr() as usize)
}
