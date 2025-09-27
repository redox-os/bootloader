use core::{cmp, mem, ptr};

use crate::area_add;
use crate::os::{OsMemoryEntry, OsMemoryKind};

use super::{MEMORY_MAP_ADDR, thunk::ThunkData};

#[repr(C, packed)]
struct MemoryMapEntry {
    pub base: u64,
    pub size: u64,
    pub kind: u32,
}

pub struct MemoryMapIter {
    thunk15: extern "C" fn(),
    data: ThunkData,
    first: bool,
}

impl MemoryMapIter {
    pub fn new(thunk15: extern "C" fn()) -> Self {
        Self {
            thunk15,
            data: ThunkData::new(),
            first: true,
        }
    }
}

impl Iterator for MemoryMapIter {
    type Item = OsMemoryEntry;
    fn next(&mut self) -> Option<Self::Item> {
        if self.first {
            self.first = false;
        } else if self.data.ebx == 0 {
            return None;
        }

        self.data.eax = 0xE820;
        self.data.ecx = mem::size_of::<MemoryMapEntry>() as u32;
        self.data.edx = 0x534D4150;
        self.data.edi = MEMORY_MAP_ADDR as u32;

        unsafe {
            self.data.with(self.thunk15);
        }

        //TODO: return error?
        assert_eq!({ self.data.eax }, 0x534D4150);
        assert_eq!({ self.data.ecx }, mem::size_of::<MemoryMapEntry>() as u32);

        let entry = unsafe { ptr::read(MEMORY_MAP_ADDR as *const MemoryMapEntry) };
        Some(Self::Item {
            base: entry.base,
            size: entry.size,
            kind: match entry.kind {
                0 => OsMemoryKind::Null,
                1 => OsMemoryKind::Free,
                3 => OsMemoryKind::Reclaim,
                _ => OsMemoryKind::Reserved,
            },
        })
    }
}

pub unsafe fn memory_map(thunk15: extern "C" fn()) -> Option<(usize, usize)> {
    let mut heap_limits = None;
    for entry in MemoryMapIter::new(thunk15) {
        let heap_start = 1024 * 1024;
        if { entry.kind } == OsMemoryKind::Free
            && entry.base <= heap_start as u64
            && (entry.base + entry.size) >= heap_start as u64
        {
            let heap_end = cmp::min(entry.base + entry.size, usize::MAX as u64) as usize;
            if heap_end >= heap_start {
                heap_limits = Some((heap_start, heap_end - heap_start));
            }
        }

        area_add(entry);
    }
    heap_limits
}
