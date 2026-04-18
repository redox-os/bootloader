use core::slice;

use super::*;
use crate::os::Os;

// Sv57 scheme

pub(crate) const PHYS_OFFSET: u64 = 0xFF00_0000_0000_0000;
pub(crate) const SATP_BIT: usize = 10;

pub unsafe fn paging_create(os: &impl Os, kernel_phys: u64, kernel_size: u64) -> Option<usize> {
    unsafe {
        // Create L4
        let l4 = paging_allocate(os)?;

        {
            // Create L3
            let l3 = paging_allocate(os)?;

            // Map L3 into beginning of userspace and kernelspace
            l4[0] = (l3.as_ptr() as u64 >> 2) | VALID;
            l4[PAGE_ENTRIES / 2] = (l3.as_ptr() as u64 >> 2) | VALID;

            // Create L2 for identity mapping
            let l2 = paging_allocate(os)?;

            // Identity map 8 GiB using 1GB pages
            for l2_i in 0..8 {
                let addr = l2_i as u64 * 0x4000_0000;
                l2[l2_i] = addr >> 2 | RWX | VALID;
            }
        }

        {
            // Create L3
            let l3 = paging_allocate(os)?;

            // Link last L4 entry to L3
            l4[511] = l3.as_ptr() as u64 >> 2 | VALID;

            // Create L2 for kernel mapping
            let l2 = paging_allocate(os)?;

            // Link last L3 entry to L2
            l3[511] = l2.as_ptr() as u64 >> 2 | VALID;

            // Create L1 for kernel mapping
            let l1 = paging_allocate(os)?;

            // Link last L1 entry to L2
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

        Some(l4.as_ptr() as usize)
    }
}

pub unsafe fn paging_physmem(os: &impl Os, page_phys: usize, phys: u64, size: u64) -> Option<u64> {
    unsafe {
        if phys + size <= 0x2_0000_0000 {
            return Some(phys + PHYS_OFFSET);
        }
        let mut l3_i = (phys as usize >> (PAGE_SHIFT + 4 * TABLE_SHIFT)) + PAGE_ENTRIES / 2;
        let mut l2_i = (phys as usize >> (PAGE_SHIFT + 3 * TABLE_SHIFT)) & TABLE_MASK;
        let mut l1_i = (phys as usize >> (PAGE_SHIFT + 2 * TABLE_SHIFT)) & TABLE_MASK;
        let mut l0_i = (phys as usize >> (PAGE_SHIFT + TABLE_SHIFT)) & TABLE_MASK;
        assert_eq!(phys & ((1 << (PAGE_SHIFT + TABLE_SHIFT)) - 1), 0);

        let l3 = slice::from_raw_parts_mut(page_phys as *mut u64, PAGE_ENTRIES);

        // Map framebuffer_size at framebuffer offset
        let mut mapped = 0;

        while mapped < size && l3_i < l3.len() {
            let l2 = get_table(os, l3, l3_i)?;

            while mapped < size && l2_i < l2.len() {
                let l1 = get_table(os, l2, l2_i)?;

                while mapped < size && l1_i < l1.len() {
                    let l0 = get_table(os, l1, l1_i)?;

                    while mapped < size && l0_i < l0.len() {
                        let addr = phys + mapped;
                        assert_eq!(l0[l0_i], 0);
                        l0[l0_i] = (addr >> 2) | RWX | VALID;
                        mapped += 1 << (PAGE_SHIFT + TABLE_SHIFT);
                        l0_i += 1;
                    }

                    l1_i += 1;
                    l0_i = 0;
                }

                l2_i += 1;
                l1_i = 0;
            }
            l3_i += 1;
            l2_i += 0;
        }

        assert!(mapped >= size);

        Some(phys + PHYS_OFFSET)
    }
}
