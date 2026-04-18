use core::slice;

use super::*;
use crate::os::Os;

// Sv39 scheme

pub(crate) const PHYS_OFFSET: u64 = 0xFFFF_FFC0_0000_0000;
pub(crate) const SATP_BITS: usize = 8;

pub unsafe fn paging_create(os: &impl Os, kernel_phys: u64, kernel_size: u64) -> Option<usize> {
    unsafe {
        // Create L2
        let l2 = paging_allocate(os)?;

        {
            // Create L1 for identity mapping
            for l2_i in 0..8 {
                let addr = l2_i as u64 * 0x4000_0000;
                // Identity map 8 GiB using 1GB pages
                l2[l2_i] = addr >> 2 | RWX | VALID | ACCESSED | DIRTY;
                // map phys into kernel VAS
                l2[(PAGE_ENTRIES / 2) + l2_i] = addr >> 2 | RWX | VALID | ACCESSED | DIRTY;
            }
        }

        {
            // Create L1 for kernel mapping
            let l1 = paging_allocate(os)?;

            // Link second to last L0 entry to L1
            l2[510] = l1.as_ptr() as u64 >> 2 | VALID;

            // Map kernel_size at kernel offset
            let mut kernel_mapped = 0;
            let mut l1_i = 0;
            while kernel_mapped < kernel_size && l1_i < l1.len() {
                let l0 = paging_allocate(os)?;
                l1[l1_i] = l0.as_ptr() as u64 >> 2 | VALID;
                l1_i += 1;

                let mut l0_i = 0;
                while kernel_mapped < kernel_size && l0_i < l2.len() {
                    let addr = kernel_phys + kernel_mapped;
                    l0[l0_i] = addr >> 2 | RWX | VALID | ACCESSED | DIRTY;
                    l0_i += 1;
                    kernel_mapped += PAGE_SIZE as u64;
                }
            }
            assert!(kernel_mapped >= kernel_size);
        }

        Some(l2.as_ptr() as usize)
    }
}

pub unsafe fn paging_physmem(os: &impl Os, page_phys: usize, phys: u64, size: u64) -> Option<u64> {
    unsafe {
        if phys + size <= 0x2_0000_0000 {
            return Some(phys + PHYS_OFFSET);
        }

        let mut l1_i = (phys as usize >> (PAGE_SHIFT + 2 * TABLE_SHIFT)) + PAGE_ENTRIES / 2;
        let mut l0_i = (phys as usize >> (PAGE_SHIFT + TABLE_SHIFT)) & TABLE_MASK;
        assert_eq!(phys & ((1 << (PAGE_SHIFT + TABLE_SHIFT)) - 1), 0);

        let l1 = slice::from_raw_parts_mut(page_phys as *mut u64, PAGE_ENTRIES);

        // Map framebuffer_size at framebuffer offset
        let mut mapped = 0;
        while mapped < size && l1_i < l1.len() {
            let l0 = get_table(os, l1, l1_i)?;

            while mapped < size && l0_i < l0.len() {
                let addr = phys + mapped;
                assert_eq!(l0[l0_i], 0);
                l0[l0_i] = (addr >> 2) | RWX | VALID | ACCESSED | DIRTY;
                mapped += 1 << (PAGE_SHIFT + TABLE_SHIFT); // Map with 2mb mega-pages
                l0_i += 1;
            }

            l1_i += 1;
            l0_i = 0;
        }

        assert!(mapped >= size);

        Some(phys + PHYS_OFFSET)
    }
}
