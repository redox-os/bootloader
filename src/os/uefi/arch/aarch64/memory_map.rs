use uefi::memory::MemoryDescriptor;

pub unsafe fn memory_map() -> usize {
    let uefi = std::system_table();

    let mut map: [u8; 65536] = [0; 65536];
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

    map_key
}
