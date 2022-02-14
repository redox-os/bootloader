use alloc::boxed::Box;
use core::{mem, ptr};
use log::error;
use uefi::memory::{MemoryDescriptor, MemoryType};

use crate::os::{OsMemoryEntry, OsMemoryKind};

use super::status_to_result;

pub struct MemoryMapIter {
    map: Box<[u8]>,
    map_key: usize,
    descriptor_size: usize,
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
        map.truncate(map_size);

        Self {
            map: map.into_boxed_slice(),
            map_key,
            descriptor_size,
            i: 0,
        }
    }
}

impl Iterator for MemoryMapIter {
    type Item=OsMemoryEntry;
    fn next(&mut self) -> Option<Self::Item> {
        if self.descriptor_size >= mem::size_of::<MemoryDescriptor>() {
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
        } else {
            error!("Unknown memory descriptor size: {}", self.descriptor_size);
            None
        }
    }
}

pub unsafe fn memory_map() -> usize {
    let iter = MemoryMapIter::new();
    let map_key = iter.map_key;

    for (i, entry) in iter.enumerate() {
        crate::AREAS[i] = entry;
    }

    map_key
}
