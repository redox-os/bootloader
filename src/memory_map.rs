use core::{cmp, mem, ptr};

use crate::thunk::ThunkData;

#[repr(packed)]
struct MemoryMapEntry {
    pub base: u64,
    pub length: u64,
    pub kind: u32,
}

pub unsafe fn memory_map(thunk15: extern "C" fn()) -> Option<(usize, usize)> {
    let mut heap_limits = None;
    let mut data = ThunkData::new();
    loop {
        let index = data.ebx;

        data.eax = 0xE820;
        data.ecx = mem::size_of::<MemoryMapEntry>() as u32;
        data.edx = 0x534D4150;
        data.edi = crate::MEMORY_MAP_ADDR as u32;

        data.with(thunk15);

        assert_eq!(data.eax, 0x534D4150);
        assert_eq!(data.ecx, mem::size_of::<MemoryMapEntry>() as u32);
        let entry = ptr::read(crate::MEMORY_MAP_ADDR as *const MemoryMapEntry);

        //TODO: There is a problem with QEMU crashing if we write at about 8 MiB, so skip to 16
        let heap_start = 16 * 1024 * 1024;
        if (
            entry.kind == 1 &&
            entry.base <= heap_start as u64 &&
            (entry.base + entry.length) >= heap_start as u64
        ) {
            let heap_end = cmp::min(
                entry.base + entry.length,
                usize::MAX as u64
            ) as usize;
            if heap_end >= heap_start {
                heap_limits = Some((
                    heap_start,
                    heap_end - heap_start
                ));
            }
        }

        if data.ebx == 0 {
            return heap_limits;
        }
    }
}
