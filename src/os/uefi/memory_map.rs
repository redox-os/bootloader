use core::{mem, ptr};
use log::error;
use uefi::memory::{MemoryDescriptor, MemoryType};

use crate::os::{OsMemoryEntry, OsMemoryKind};

pub struct MemoryMapIter {
    map: [u8; 4096],
    map_size: usize,
    descriptor_size: usize,
    i: usize,
}

impl MemoryMapIter {
    pub fn new() -> Self {
        let uefi = std::system_table();

        let mut map: [u8; 4096] = [0; 4096];
        let mut map_size = map.len();
        let mut map_key = 0;
        let mut descriptor_size = 0;
        let mut descriptor_version = 0;
        let _ = (uefi.BootServices.GetMemoryMap)(
            &mut map_size,
            map.as_mut_ptr() as *mut MemoryDescriptor,
            &mut map_key,
            &mut descriptor_size,
            &mut descriptor_version
        );

        Self {
            map,
            map_size,
            descriptor_size,
            i: 0,
        }
    }
}

impl Iterator for MemoryMapIter {
    type Item=OsMemoryEntry;
    fn next(&mut self) -> Option<Self::Item> {
        if
            self.descriptor_size >= mem::size_of::<MemoryDescriptor>() &&
            self.i < self.map_size/self.descriptor_size
        {
            let descriptor_ptr = unsafe { self.map.as_ptr().add(self.i * self.descriptor_size) };
            self.i += 1;

            let descriptor = unsafe { ptr::read(descriptor_ptr as *const MemoryDescriptor) };
            let descriptor_type: MemoryType = unsafe { mem::transmute(descriptor.Type) };

            Some(OsMemoryEntry {
                base: descriptor.PhysicalStart.0,
                //TODO: do not hard code page size
                size: descriptor.NumberOfPages * 4096,
                kind: match descriptor_type {
                    MemoryType::EfiBootServicesCode |
                    MemoryType::EfiBootServicesData |
                    MemoryType::EfiConventionalMemory => {
                        OsMemoryKind::Free
                    },
                    MemoryType::EfiLoaderCode |
                    MemoryType::EfiLoaderData |
                    MemoryType::EfiACPIReclaimMemory => {
                        OsMemoryKind::Reclaim
                    },
                    _ => {
                        OsMemoryKind::Reserved
                    }
                }
            })
        } else {
            error!("Unknown memory descriptor size: {}", self.descriptor_size);
            None
        }
    }
}
