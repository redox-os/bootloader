use core::slice;
use redoxfs::Disk;

use crate::os::{Os, OsVideoMode};

const ENTRY_ADDRESS_MASK: u64 = 0x000F_FFFF_FFFF_F000;
const PAGE_ENTRIES: usize = 512;
const PAGE_SIZE: usize = 4096;
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
    // Create L0
    let l0 = paging_allocate(os)?;

    {
        // Create L1 for identity mapping
        let l1 = paging_allocate(os)?;

        // Link first user and first kernel L0 entry to L1
        l0[0] = l1.as_ptr() as u64 | 1 << 10 | 1 << 1 | 1;
        l0[256] = l1.as_ptr() as u64 | 1 << 10 | 1 << 1 | 1;

        // Identity map 8 GiB using 2 MiB pages
        for l1_i in 0..8 {
            let l2 = paging_allocate(os)?;
            l1[l1_i] = l2.as_ptr() as u64 | 1 << 10 | 1 << 1 | 1;
            for l2_i in 0..l2.len() {
                let addr =
                    l1_i as u64 * 0x4000_0000 +
                    l2_i as u64 * 0x20_0000;
                l2[l2_i] = addr | 1 << 10 | 1;
            }
        }
    }

    {
        // Create L1 for kernel mapping
        let l1 = paging_allocate(os)?;

        // Link second to last L0 entry to L1
        l0[510] = l1.as_ptr() as u64 | 1 << 10 | 1 << 1 | 1;

        // Map kernel_size at kernel offset
        let mut kernel_mapped = 0;
        let mut l1_i = 0;
        while kernel_mapped < kernel_size && l1_i < l1.len() {
            let l2 = paging_allocate(os)?;
            l1[l1_i] = l2.as_ptr() as u64 | 1 << 10 | 1 << 1 | 1;
            l1_i += 1;

            let mut l2_i = 0;
            while kernel_mapped < kernel_size && l2_i < l2.len(){
                let l3 = paging_allocate(os)?;
                l2[l2_i] = l3.as_ptr() as u64 | 1 << 10 | 1 << 1 | 1;
                l2_i += 1;

                let mut l3_i = 0;
                while kernel_mapped < kernel_size && l3_i < l3.len() {
                    let addr = kernel_phys + kernel_mapped;
                    l3[l3_i] = addr | 1 << 10 | 1 << 1 | 1;
                    l3_i += 1;
                    kernel_mapped += PAGE_SIZE as u64;
                }
            }
        }
        assert!(kernel_mapped >= kernel_size);
    }

    Some(l0.as_ptr() as usize)
}

pub unsafe fn paging_framebuffer<
    D: Disk,
    V: Iterator<Item=OsVideoMode>
>(os: &mut dyn Os<D, V>, page_phys: usize, framebuffer_phys: u64, framebuffer_size: u64) -> Option<()> {
    log::error!("paging_framebuffer not implemented for aarch64");
    None
}
