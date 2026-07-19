use alloc::vec;
use alloc::vec::Vec;
use core::{mem, ptr};
use uefi::memory::{MemoryDescriptor, MemoryType};

use crate::area_add;
#[cfg(target_arch = "aarch64")]
use crate::os::{OsMemoryAttribute, OsMemoryAttributeRange};
use crate::os::{OsMemoryEntry, OsMemoryKind};

use super::status_to_result;

const UEFI_PAGE_SIZE: u64 = 4096;

pub struct MemoryMapIter {
    map: Vec<u8>,
    map_key: usize,
    descriptor_size: usize,
    descriptor_version: u32,
    i: usize,
}

impl MemoryMapIter {
    pub fn new() -> Self {
        let uefi = std::system_table();

        let mut map = vec![0; 65536];
        let mut map_size = map.len();
        let mut map_key = 0;
        let mut descriptor_size = 0;
        let mut descriptor_version = 0;
        status_to_result((uefi.BootServices.GetMemoryMap)(
            &mut map_size,
            map.as_mut_ptr() as *mut MemoryDescriptor,
            &mut map_key,
            &mut descriptor_size,
            &mut descriptor_version,
        ))
        .expect("Failed to get UEFI memory map");

        // Ensure descriptor size is usable
        assert!(descriptor_size >= mem::size_of::<MemoryDescriptor>());

        // Ensure descriptor version is supported
        assert_eq!(descriptor_version, 1);

        // Reduce map size to returned value
        map.truncate(map_size);

        Self {
            map,
            map_key,
            descriptor_size,
            descriptor_version,
            i: 0,
        }
    }

    pub fn exit_boot_services(mut self) {
        let handle = std::handle();
        let uefi = std::system_table();

        // We are writing to the memory map that will be passed to
        // SetVirtualAddressMap before ExitBootServices as on some firmware
        // EfiLoaderData memory regions like this one are marked as read-only
        // after ExitBootServices
        for i in 0..self.map.len() / self.descriptor_size {
            let descriptor_ptr = unsafe { self.map.as_mut_ptr().add(i * self.descriptor_size) };
            let descriptor = unsafe { &mut *(descriptor_ptr as *mut MemoryDescriptor) };

            // Map all memory regions even when not marked as EFI_MEMORY_RUNTIME
            // as some firmware uses memory regions not marked as
            // EFI_MEMORY_RUNTIME in runtime services. Linux has a list of
            // exactly which memory regions need to be mapped, but for simplicity
            // we are mapping all regions here.

            // Identity map all memory regions as some firmware fails to update
            // all pointers in SetVirtualAddressMap.

            descriptor.VirtualStart.0 = descriptor.PhysicalStart.0;
        }

        status_to_result((uefi.BootServices.ExitBootServices)(handle, self.map_key))
            .expect("Failed to exit UEFI boot services");

        // Runtime services must be called with interrupts disabled
        super::arch::disable_interrupts();

        status_to_result((uefi.RuntimeServices.SetVirtualAddressMap)(
            self.map.len(),
            self.descriptor_size,
            self.descriptor_version,
            self.map.as_ptr() as *const MemoryDescriptor,
        ))
        .expect("Failed to set UEFI runtime services virtual address map");

        // After ExitBootServices, GlobalAlloc::dealloc() is not allowed anymore
        // as it uses boot services.
        mem::forget(self);
    }
}

/// Take a snapshot of the cacheability information needed by the AArch64
/// bootstrap page table. Page-table allocations performed afterward may
/// change the UEFI map, so this snapshot's map key must not be used by
/// ExitBootServices.
#[cfg(target_arch = "aarch64")]
pub(crate) fn memory_attribute_ranges() -> Vec<OsMemoryAttributeRange> {
    const EFI_MEMORY_UC: u64 = 1 << 0;
    const EFI_MEMORY_WC: u64 = 1 << 1;
    const EFI_MEMORY_WT: u64 = 1 << 2;
    const EFI_MEMORY_WB: u64 = 1 << 3;
    const EFI_CACHEABILITY_MASK: u64 =
        EFI_MEMORY_UC | EFI_MEMORY_WC | EFI_MEMORY_WT | EFI_MEMORY_WB;

    let memory_map = MemoryMapIter::new();
    let mut ranges = Vec::<OsMemoryAttributeRange>::with_capacity(
        memory_map.map.len() / memory_map.descriptor_size,
    );

    for i in 0..memory_map.map.len() / memory_map.descriptor_size {
        let descriptor_ptr = unsafe { memory_map.map.as_ptr().add(i * memory_map.descriptor_size) };
        let descriptor = unsafe { ptr::read(descriptor_ptr as *const MemoryDescriptor) };
        let start = descriptor.PhysicalStart.0;
        let end = start.saturating_add(descriptor.NumberOfPages.saturating_mul(UEFI_PAGE_SIZE));
        if start == end {
            continue;
        }

        let fallback_write_back = matches!(
            descriptor.Type,
            value if value == MemoryType::EfiLoaderCode as u32
                || value == MemoryType::EfiLoaderData as u32
                || value == MemoryType::EfiBootServicesCode as u32
                || value == MemoryType::EfiBootServicesData as u32
                || value == MemoryType::EfiRuntimeServicesCode as u32
                || value == MemoryType::EfiRuntimeServicesData as u32
                || value == MemoryType::EfiConventionalMemory as u32
        );
        // GetMemoryMap reports supported cacheability classes rather than
        // necessarily the class currently selected by the firmware. Prefer
        // the strongest standard AArch64 mapping advertised for the range.
        // TODO: Honor EFI_MEMORY_ISA_VALID and EFI_MEMORY_ISA_MASK when the
        // page-table code can allocate arbitrary MAIR entries.
        let attribute = if descriptor.Attribute & EFI_MEMORY_WB != 0 {
            OsMemoryAttribute::NormalWriteBack
        } else if descriptor.Attribute & EFI_MEMORY_WT != 0 {
            OsMemoryAttribute::NormalWriteThrough
        } else if descriptor.Attribute & EFI_MEMORY_WC != 0 {
            OsMemoryAttribute::NormalNonCacheable
        } else if descriptor.Attribute & EFI_MEMORY_UC != 0 {
            OsMemoryAttribute::Device
        } else if descriptor.Attribute & EFI_CACHEABILITY_MASK == 0 && fallback_write_back {
            // Some non-conforming firmware omits cacheability attributes.
            // Preserve the established treatment of ordinary RAM in that
            // case. Reserved, MMIO, ACPI, and persistent memory remain Device
            // unless the firmware explicitly advertises another attribute.
            OsMemoryAttribute::NormalWriteBack
        } else {
            OsMemoryAttribute::Device
        };

        ranges.push(OsMemoryAttributeRange {
            base: start,
            size: end - start,
            attribute,
        });
    }

    ranges.sort_unstable_by_key(|range| range.base);

    let mut merged = Vec::<OsMemoryAttributeRange>::new();
    for range in ranges {
        let range_end = range.base.saturating_add(range.size);
        if let Some(last) = merged.last_mut()
            && last.attribute == range.attribute
            && range.base <= last.base.saturating_add(last.size)
        {
            let last_end = last.base.saturating_add(last.size);
            last.size = last_end.max(range_end) - last.base;
        } else {
            merged.push(range);
        }
    }

    merged
}

impl Iterator for MemoryMapIter {
    type Item = OsMemoryEntry;
    fn next(&mut self) -> Option<Self::Item> {
        if self.i < self.map.len() / self.descriptor_size {
            let descriptor_ptr = unsafe { self.map.as_ptr().add(self.i * self.descriptor_size) };
            self.i += 1;

            let descriptor = unsafe { ptr::read(descriptor_ptr as *const MemoryDescriptor) };
            let descriptor_type: MemoryType = unsafe { mem::transmute(descriptor.Type) };

            Some(OsMemoryEntry {
                base: descriptor.PhysicalStart.0,
                size: descriptor.NumberOfPages * UEFI_PAGE_SIZE,
                kind: match descriptor_type {
                    MemoryType::EfiLoaderCode
                    | MemoryType::EfiLoaderData
                    | MemoryType::EfiBootServicesCode
                    | MemoryType::EfiBootServicesData
                    | MemoryType::EfiConventionalMemory => OsMemoryKind::Free,
                    //TODO: mark ACPI memory as reclaim
                    _ => OsMemoryKind::Reserved,
                },
            })
        } else {
            None
        }
    }
}

pub unsafe fn memory_map() -> MemoryMapIter {
    let mut iter = MemoryMapIter::new();

    // Using next to avoid consuming iterator
    while let Some(entry) = iter.next() {
        area_add(entry);
    }

    // Rewind iterator
    iter.i = 0;

    iter
}
