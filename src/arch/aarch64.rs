use crate::area_add;
use crate::os::{Os, OsMemoryAttribute, OsMemoryAttributeRange, OsMemoryEntry, OsMemoryKind};
use core::slice;

pub(crate) const PF_PRESENT: u64 = 1 << 0;
// Bit 1 selects a table entry for L0-L2 and a page entry for L3.
pub(crate) const PF_TABLE: u64 = 1 << 1;
pub(crate) const PF_PAGE: u64 = 1 << 1;
pub(crate) const PF_OUTER_SHAREABLE: u64 = 0b01 << 8;
pub(crate) const PF_INNER_SHAREABLE: u64 = 0b11 << 8;
pub(crate) const PF_ACCESS: u64 = 1 << 10;

const ATTR_INDEX_NORMAL_WB: u64 = 0 << 2;
const ATTR_INDEX_NORMAL_NC: u64 = 1 << 2;
const ATTR_INDEX_DEVICE: u64 = 2 << 2;
const ATTR_INDEX_NORMAL_WT: u64 = 3 << 2;

pub(crate) const PF_NORMAL_WB: u64 = PF_INNER_SHAREABLE | ATTR_INDEX_NORMAL_WB;
pub(crate) const PF_NORMAL_NC: u64 = PF_INNER_SHAREABLE | ATTR_INDEX_NORMAL_NC;
pub(crate) const PF_DEV: u64 = PF_OUTER_SHAREABLE | ATTR_INDEX_DEVICE;
pub(crate) const PF_NORMAL_WT: u64 = PF_INNER_SHAREABLE | ATTR_INDEX_NORMAL_WT;

pub(crate) const ENTRY_ADDRESS_MASK: u64 = 0x000F_FFFF_FFFF_F000;
pub(crate) const PAGE_ENTRIES: usize = 512;
const PAGE_SIZE: usize = 4096;
const L2_BLOCK_SIZE: u64 = 2 * 1024 * 1024;
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

fn page_flags(attribute: OsMemoryAttribute) -> u64 {
    match attribute {
        OsMemoryAttribute::Device => PF_DEV,
        OsMemoryAttribute::NormalNonCacheable => PF_NORMAL_NC,
        OsMemoryAttribute::NormalWriteThrough => PF_NORMAL_WT,
        OsMemoryAttribute::NormalWriteBack => PF_NORMAL_WB,
    }
}

fn range_attribute(
    ranges: &[OsMemoryAttributeRange],
    start: u64,
    end: u64,
) -> Option<OsMemoryAttribute> {
    if let Some(range) = ranges
        .iter()
        .find(|range| range.base <= start && end <= range.base.saturating_add(range.size))
    {
        return Some(range.attribute);
    }

    if ranges
        .iter()
        .any(|range| start < range.base.saturating_add(range.size) && range.base < end)
    {
        None
    } else {
        Some(OsMemoryAttribute::Device)
    }
}

pub unsafe fn paging_create(os: &impl Os, kernel_phys: u64, kernel_size: u64) -> Option<usize> {
    unsafe {
        let memory_attribute_ranges = os.memory_attribute_ranges();

        // Create L0
        let l0 = paging_allocate(os)?;

        {
            // Create L1 for identity mapping
            let l1 = paging_allocate(os)?;

            // Link first user and first kernel L0 entry to L1
            l0[0] = l1.as_ptr() as u64 | PF_ACCESS | PF_TABLE | PF_PRESENT;
            l0[256] = l1.as_ptr() as u64 | PF_ACCESS | PF_TABLE | PF_PRESENT;

            // Identity-map 8 GiB while preserving the cacheability classes
            // supplied by the boot environment. Use 2 MiB blocks normally
            // and 4 KiB pages only where an attribute boundary crosses a
            // block. Addresses absent from the map remain Device.
            for l1_i in 0..8 {
                let l2 = paging_allocate(os)?;
                l1[l1_i] = l2.as_ptr() as u64 | PF_ACCESS | PF_TABLE | PF_PRESENT;

                for (l2_i, entry) in l2.iter_mut().enumerate() {
                    let addr = l1_i as u64 * 0x4000_0000 + l2_i as u64 * L2_BLOCK_SIZE;
                    let end = addr + L2_BLOCK_SIZE;
                    if let Some(attribute) = range_attribute(&memory_attribute_ranges, addr, end) {
                        *entry = addr | PF_ACCESS | page_flags(attribute) | PF_PRESENT;
                    } else {
                        let l3 = paging_allocate(os)?;
                        *entry = l3.as_ptr() as u64 | PF_ACCESS | PF_TABLE | PF_PRESENT;
                        for (l3_i, page) in l3.iter_mut().enumerate() {
                            let page_addr = addr + l3_i as u64 * PAGE_SIZE as u64;
                            let page_end = page_addr + PAGE_SIZE as u64;
                            let attribute =
                                range_attribute(&memory_attribute_ranges, page_addr, page_end)
                                    .expect("memory attribute boundary is not page-aligned");
                            *page = page_addr
                                | PF_ACCESS
                                | page_flags(attribute)
                                | PF_PAGE
                                | PF_PRESENT;
                        }
                    }
                }
            }
        }

        {
            // Create L1 for kernel mapping
            let l1 = paging_allocate(os)?;

            // Link second to last L0 entry to L1
            l0[510] = l1.as_ptr() as u64 | PF_ACCESS | PF_TABLE | PF_PRESENT;

            // Map kernel_size at kernel offset
            let mut kernel_mapped = 0;
            let mut l1_i = 0;
            while kernel_mapped < kernel_size && l1_i < l1.len() {
                let l2 = paging_allocate(os)?;
                l1[l1_i] = l2.as_ptr() as u64 | PF_ACCESS | PF_TABLE | PF_PRESENT;
                l1_i += 1;

                let mut l2_i = 0;
                while kernel_mapped < kernel_size && l2_i < l2.len() {
                    let l3 = paging_allocate(os)?;
                    l2[l2_i] = l3.as_ptr() as u64 | PF_ACCESS | PF_TABLE | PF_PRESENT;
                    l2_i += 1;

                    let mut l3_i = 0;
                    while kernel_mapped < kernel_size && l3_i < l3.len() {
                        let addr = kernel_phys + kernel_mapped;
                        let attribute = range_attribute(
                            &memory_attribute_ranges,
                            addr,
                            addr + PAGE_SIZE as u64,
                        )
                        .expect("kernel memory attribute boundary is not page-aligned");
                        assert_ne!(
                            attribute,
                            OsMemoryAttribute::Device,
                            "kernel image was allocated in Device memory"
                        );
                        l3[l3_i] = addr | PF_ACCESS | page_flags(attribute) | PF_PAGE | PF_PRESENT;
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

pub unsafe fn paging_framebuffer(
    os: &impl Os,
    page_phys: usize,
    framebuffer_phys: u64,
    framebuffer_size: u64,
) -> Option<u64> {
    unsafe {
        //TODO: smarter test for framebuffer already mapped
        if framebuffer_phys + framebuffer_size <= 0x2_0000_0000 {
            return Some(framebuffer_phys + PHYS_OFFSET);
        }

        let memory_attribute_ranges = os.memory_attribute_ranges();

        let l0_i = ((framebuffer_phys / 0x80_0000_0000) + 256) as usize;
        let mut l1_i = ((framebuffer_phys % 0x80_0000_0000) / 0x4000_0000) as usize;
        let mut l2_i = ((framebuffer_phys % 0x4000_0000) / 0x20_0000) as usize;
        let mut l3_i = ((framebuffer_phys % 0x20_0000) / (PAGE_SIZE as u64)) as usize;
        assert_eq!(framebuffer_phys % (PAGE_SIZE as u64), 0);

        let l0 = slice::from_raw_parts_mut(page_phys as *mut u64, PAGE_ENTRIES);

        // Create l1 for framebuffer mapping
        let l1 = if l0[l0_i] == 0 {
            let l1 = paging_allocate(os)?;
            l0[l0_i] = l1.as_ptr() as u64 | PF_ACCESS | PF_TABLE | PF_PRESENT;
            l1
        } else {
            slice::from_raw_parts_mut((l0[l0_i] & ENTRY_ADDRESS_MASK) as *mut u64, PAGE_ENTRIES)
        };

        // Map framebuffer_size at framebuffer offset
        let mut framebuffer_mapped = 0;
        while framebuffer_mapped < framebuffer_size && l1_i < l1.len() {
            let l2 = paging_allocate(os)?;
            assert_eq!(l1[l1_i], 0);
            l1[l1_i] = l2.as_ptr() as u64 | PF_ACCESS | PF_TABLE | PF_PRESENT;

            while framebuffer_mapped < framebuffer_size && l2_i < l2.len() {
                let l3 = paging_allocate(os)?;
                assert_eq!(l2[l2_i], 0);
                l2[l2_i] = l3.as_ptr() as u64 | PF_ACCESS | PF_TABLE | PF_PRESENT;

                while framebuffer_mapped < framebuffer_size && l3_i < l3.len() {
                    let addr = framebuffer_phys + framebuffer_mapped;
                    assert_eq!(l3[l3_i], 0);
                    let attribute =
                        range_attribute(&memory_attribute_ranges, addr, addr + PAGE_SIZE as u64)
                            .expect("framebuffer memory attribute boundary is not page-aligned");
                    l3[l3_i] = addr | PF_ACCESS | page_flags(attribute) | PF_PAGE | PF_PRESENT;
                    framebuffer_mapped += PAGE_SIZE as u64;
                    l3_i += 1;
                }

                l2_i += 1;
                l3_i = 0;
            }

            l1_i += 1;
            l2_i = 0;
        }
        assert!(framebuffer_mapped >= framebuffer_size);

        Some(framebuffer_phys + PHYS_OFFSET)
    }
}
