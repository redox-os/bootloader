use core::slice;
use x86::{
    controlregs::{self, Cr0, Cr4},
    msr,
};
use uefi::status::Result;

unsafe fn paging_allocate() -> Result<&'static mut [u64]> {
    let ptr = super::allocate_zero_pages(1)?;

    Ok(slice::from_raw_parts_mut(
        ptr as *mut u64,
        512 // page size divided by u64 size
    ))
}

pub unsafe fn paging_create(kernel_phys: u64) -> Result<u64> {
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
                        kernel_phys;
                    pt[pt_i] = addr | 1 << 1 | 1;
                }
            }
        }
    }

    Ok(pml4.as_ptr() as u64)
}

pub unsafe fn paging_enter(page_phys: u64) {
    // Enable OSXSAVE, FXSAVE/FXRSTOR, Page Global, Page Address Extension, and Page Size Extension
    let mut cr4 = controlregs::cr4();
    cr4 |= Cr4::CR4_ENABLE_OS_XSAVE
        | Cr4::CR4_ENABLE_SSE
        | Cr4::CR4_ENABLE_GLOBAL_PAGES
        | Cr4::CR4_ENABLE_PAE
        | Cr4::CR4_ENABLE_PSE;
    controlregs::cr4_write(cr4);

    // Enable Long mode and NX bit
    let mut efer = msr::rdmsr(msr::IA32_EFER);
    efer |= 1 << 11 | 1 << 8;
    msr::wrmsr(msr::IA32_EFER, efer);

    // Set new page map
    controlregs::cr3_write(page_phys);

    // Enable paging, write protect kernel, protected mode
    let mut cr0 = controlregs::cr0();
    cr0 |= Cr0::CR0_ENABLE_PAGING | Cr0::CR0_WRITE_PROTECT | Cr0::CR0_PROTECTED_MODE;
    controlregs::cr0_write(cr0);
}
