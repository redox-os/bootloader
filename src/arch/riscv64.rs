use core::slice;

use crate::os::{Os, OsMemoryEntry, OsMemoryKind};

// Sv48 scheme

const PAGE_SHIFT: usize = 12;
const TABLE_SHIFT: usize = 9;
const TABLE_MASK: usize = (1 << TABLE_SHIFT) - 1;
const PAGE_ENTRIES: usize = 512;
const PAGE_SIZE: usize = 4096;
const PHYS_MASK: usize = (1usize << 44) - 1;
pub(crate) const PHYS_OFFSET: u64 = 0xFFFF_8000_0000_0000;

unsafe fn paging_allocate(os: &impl Os) -> Option<&'static mut [u64]> {
    unsafe {
        let ptr = os.alloc_zeroed_page_aligned(PAGE_SIZE);
        if !ptr.is_null() {
            area_add(OsMemoryEntry {
                base: ptr as u64,
                size: PAGE_SIZE as u64,
                kind: OsMemoryKind::Reclaim,
            });
            Some(slice::from_raw_parts_mut(ptr as *mut u64, PAGE_ENTRIES))
        } else {
            None
        }
    }
}

const VALID: u64 = 1;
const RWX: u64 = 7 << 1;

unsafe fn get_table(os: &impl Os, parent: &mut [u64], index: usize) -> Option<&'static mut [u64]> {
    unsafe {
        if parent[index] == 0 {
            let table = paging_allocate(os)?;
            parent[index] = table.as_ptr() as u64 >> 2 | VALID;
            Some(table)
        } else {
            Some(slice::from_raw_parts_mut(
                (((parent[index] >> 10) & PHYS_MASK as u64) << 12) as *mut u64,
                PAGE_ENTRIES,
            ))
        }
    }
}

pub unsafe fn paging_create(os: &impl Os, kernel_phys: u64, kernel_size: u64) -> Option<usize> {
    unsafe {
        // Create L0
        let l0 = paging_allocate(os)?;

        {
            // Create L1 for identity mapping
            let l1 = paging_allocate(os)?;

            // Map L1 into beginning of userspace and kernelspace
            l0[0] = (l1.as_ptr() as u64 >> 2) | VALID;
            l0[PAGE_ENTRIES / 2] = (l1.as_ptr() as u64 >> 2) | VALID;

            // Identity map 8 GiB using 1GB pages
            for l1_i in 0..8 {
                let addr = l1_i as u64 * 0x4000_0000;
                l1[l1_i] = addr >> 2 | RWX | VALID;
            }
        }

        {
            // Create L1 for kernel mapping
            let l1 = paging_allocate(os)?;

            // Link second to last L0 entry to L1
            l0[510] = l1.as_ptr() as u64 >> 2 | VALID;

            // Map kernel_size at kernel offset
            let mut kernel_mapped = 0;
            let mut l1_i = 0;
            while kernel_mapped < kernel_size && l1_i < l1.len() {
                let l2 = paging_allocate(os)?;
                l1[l1_i] = l2.as_ptr() as u64 >> 2 | VALID;
                l1_i += 1;

                let mut l2_i = 0;
                while kernel_mapped < kernel_size && l2_i < l2.len() {
                    let l3 = paging_allocate(os)?;
                    l2[l2_i] = l3.as_ptr() as u64 >> 2 | VALID;
                    l2_i += 1;

                    let mut l3_i = 0;
                    while kernel_mapped < kernel_size && l3_i < l3.len() {
                        let addr = kernel_phys + kernel_mapped;
                        l3[l3_i] = addr >> 2 | RWX | VALID;
                        l3_i += 1;
                        kernel_mapped += PAGE_SIZE as u64;
                    }
                }
            }
            assert!(kernel_mapped >= kernel_size);
        }

        Some(l0.as_ptr() as usize)
    }
}

pub unsafe fn paging_physmem(os: &impl Os, page_phys: usize, phys: u64, size: u64) -> Option<u64> {
    unsafe {
        if phys + size <= 0x2_0000_0000 {
            return Some(phys + PHYS_OFFSET);
        }

        let mut l0_i = (phys as usize >> (PAGE_SHIFT + 3 * TABLE_SHIFT)) + PAGE_ENTRIES / 2;
        let mut l1_i = (phys as usize >> (PAGE_SHIFT + 2 * TABLE_SHIFT)) & TABLE_MASK;
        let mut l2_i = (phys as usize >> (PAGE_SHIFT + TABLE_SHIFT)) & TABLE_MASK;
        assert_eq!(phys & (1 << (PAGE_SHIFT + TABLE_SHIFT) - 1), 0);

        let l0 = slice::from_raw_parts_mut(page_phys as *mut u64, PAGE_ENTRIES);

        // Map framebuffer_size at framebuffer offset
        let mut mapped = 0;
        while mapped < size && l0_i < l0.len() {
            let l1 = get_table(os, l0, l0_i)?;

            while mapped < size && l1_i < l1.len() {
                let l2 = get_table(os, l1, l1_i)?;

                while mapped < size && l2_i < l2.len() {
                    let addr = phys + mapped;
                    assert_eq!(l2[l2_i], 0);
                    l2[l2_i] = ((addr >> 2) | RWX | VALID) as u64;
                    mapped += 1 << (PAGE_SHIFT + TABLE_SHIFT);
                    l2_i += 1;
                }

                l1_i += 1;
                l2_i = 0;
            }

            l0_i += 1;
            l1_i = 0;
        }

        assert!(mapped >= size);

        Some(phys + PHYS_OFFSET)
    }
}

use crate::area_add;
pub use paging_physmem as paging_framebuffer;
