use alloc::alloc;
use core::slice;

unsafe fn paging_allocate() -> Option<&'static mut [u64]> {
    let ptr = alloc::alloc_zeroed(
        alloc::Layout::from_size_align(4096, 4096).unwrap()
    );
    if ! ptr.is_null() {
        Some(slice::from_raw_parts_mut(
            ptr as *mut u64,
            512 // page size divided by u64 size
        ))
    } else {
        None
    }
}

pub unsafe fn paging_create(kernel_phys: usize) -> Option<usize> {
    // Create PML4
    let pml4 = paging_allocate()?;

    // Recursive mapping for compatibility
    pml4[511] = pml4.as_ptr() as u64 | 1 << 1 | 1;

    {
        // Create PDP for identity mapping
        let pdp = paging_allocate()?;

        // Link first user and first kernel PML4 entry to PDP
        pml4[0] = pdp.as_ptr() as u64 | 1 << 1 | 1;
        pml4[256] = pdp.as_ptr() as u64 | 1 << 1 | 1;

        // Identity map 8 GiB pages
        for pdp_i in 0..8 {
            let pd = paging_allocate()?;
            pdp[pdp_i] = pd.as_ptr() as u64 | 1 << 1 | 1;
            for pd_i in 0..pd.len() {
                let pt = paging_allocate()?;
                pd[pd_i] = pt.as_ptr() as u64 | 1 << 1 | 1;
                for pt_i in 0..pt.len() {
                    let addr =
                        pdp_i as u64 * 0x4000_0000 +
                        pd_i as u64 * 0x20_0000 +
                        pt_i as u64 * 0x1000;
                    pt[pt_i] = addr | 1 << 1 | 1;
                }
            }
        }
    }

    {
        // Create PDP for kernel mapping
        let pdp = paging_allocate()?;

        // Link second to last PML4 entry to PDP
        pml4[510] = pdp.as_ptr() as u64 | 1 << 1 | 1;

        // Map 1 GiB at kernel offset
        for pdp_i in 0..1 {
            let pd = paging_allocate()?;
            pdp[pdp_i] = pd.as_ptr() as u64 | 1 << 1 | 1;
            for pd_i in 0..pd.len() {
                let pt = paging_allocate()?;
                pd[pd_i] = pt.as_ptr() as u64 | 1 << 1 | 1;
                for pt_i in 0..pt.len() {
                    let addr =
                        pdp_i as u64 * 0x4000_0000 +
                        pd_i as u64 * 0x20_0000 +
                        pt_i as u64 * 0x1000 +
                        kernel_phys as u64;
                    pt[pt_i] = addr | 1 << 1 | 1;
                }
            }
        }
    }

    Some(pml4.as_ptr() as usize)
}
