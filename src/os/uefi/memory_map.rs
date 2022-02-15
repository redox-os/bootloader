use alloc::vec::Vec;
use core::{mem, ptr};
use uefi::memory::{MemoryDescriptor, MemoryType};

use crate::os::{OsMemoryEntry, OsMemoryKind};

use super::status_to_result;

const EFI_MEMORY_RUNTIME: u64 = 0x8000000000000000;

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
            &mut descriptor_version
        )).expect("Failed to get UEFI memory map");

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

    pub fn exit_boot_services(&self) {
        let handle = std::handle();
        let uefi = std::system_table();

        status_to_result((uefi.BootServices.ExitBootServices)(
            handle,
            self.map_key
        )).expect("Failed to exit UEFI boot services");
    }

    pub fn set_virtual_address_map(&mut self, phys_offset: u64) {
        let uefi = std::system_table();

        for i in 0..self.map.len()/self.descriptor_size {
            let descriptor_ptr = unsafe { self.map.as_mut_ptr().add(i * self.descriptor_size) };
            let descriptor = unsafe { &mut *(descriptor_ptr as *mut MemoryDescriptor) };
            if descriptor.Attribute & EFI_MEMORY_RUNTIME == EFI_MEMORY_RUNTIME {
                descriptor.VirtualStart.0 = descriptor.PhysicalStart.0 + phys_offset;
            }
        }

        status_to_result((uefi.RuntimeServices.SetVirtualAddressMap)(
            self.map.len(),
            self.descriptor_size,
            self.descriptor_version,
            self.map.as_ptr() as *const MemoryDescriptor
        )).expect("Failed to set UEFI runtime services virtual address map");
    }
}

impl Iterator for MemoryMapIter {
    type Item=OsMemoryEntry;
    fn next(&mut self) -> Option<Self::Item> {
        if self.i < self.map.len()/self.descriptor_size {
            let descriptor_ptr = unsafe { self.map.as_ptr().add(self.i * self.descriptor_size) };
            self.i += 1;

            let descriptor = unsafe { ptr::read(descriptor_ptr as *const MemoryDescriptor) };
            let descriptor_type: MemoryType = unsafe { mem::transmute(descriptor.Type) };

            Some(OsMemoryEntry {
                base: descriptor.PhysicalStart.0,
                //TODO: do not hard code page size
                size: descriptor.NumberOfPages * 4096,
                kind: match descriptor_type {
                    MemoryType::EfiLoaderCode |
                    MemoryType::EfiLoaderData |
                    MemoryType::EfiBootServicesCode |
                    MemoryType::EfiBootServicesData |
                    MemoryType::EfiConventionalMemory => {
                        OsMemoryKind::Free
                    },
                    //TODO: mark ACPI memory as reclaim
                    _ => {
                        OsMemoryKind::Reserved
                    }
                }
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
        crate::AREAS[iter.i] = entry;
    }

    // Rewind iterator
    iter.i = 0;

    iter
}
