use alloc::{
    string::String,
};
use core::{
    alloc::{GlobalAlloc, Layout},
    convert::TryFrom,
    slice,
};
use linked_list_allocator::LockedHeap;
use spin::Mutex;

use crate::arch::paging_create;
use crate::logger::LOGGER;
use crate::os::{Os, OsKey};

use self::disk::DiskBios;
use self::memory_map::{memory_map, MemoryMapIter};
use self::thunk::ThunkData;
use self::vbe::VideoModeIter;
use self::vga::{VgaTextColor, Vga};

#[macro_use]
mod macros;

mod disk;
mod memory_map;
mod panic;
mod thunk;
mod vbe;
mod vga;

// Real mode memory allocation, for use with thunk
// 0x500 to 0x7BFF is free
const DISK_BIOS_ADDR: usize = 0x1000; // 4096 bytes, ends at 0x1FFF
const VBE_CARD_INFO_ADDR: usize = 0x2000; // 512 bytes, ends at 0x21FF
const VBE_MODE_INFO_ADDR: usize = 0x2200; // 256 bytes, ends at 0x22FF
const MEMORY_MAP_ADDR: usize = 0x2300; // 24 bytes, ends at 0x2317
const DISK_ADDRESS_PACKET_ADDR: usize = 0x2318; // 16 bytes, ends at 0x2327
const THUNK_STACK_ADDR: usize = 0x7C00; // Grows downwards
const VGA_ADDR: usize = 0xB8000;

const PHYS_OFFSET: u64 = 0xFFFF800000000000;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub(crate) static VGA: Mutex<Vga> = Mutex::new(
    unsafe { Vga::new(VGA_ADDR, 80, 25) }
);

#[allow(dead_code)]
#[repr(packed)]
pub struct KernelArgs {
    kernel_base: u64,
    kernel_size: u64,
    stack_base: u64,
    stack_size: u64,
    env_base: u64,
    env_size: u64,

    /// The base 64-bit pointer to an array of saved RSDPs. It's up to the kernel (and possibly
    /// userspace), to decide which RSDP to use. The buffer will be a linked list containing a
    /// 32-bit relative (to this field) next, and the actual struct afterwards.
    ///
    /// This field can be NULL, and if so, the system has not booted with UEFI or in some other way
    /// retrieved the RSDPs. The kernel or a userspace driver will thus try searching the BIOS
    /// memory instead. On UEFI systems, searching is not guaranteed to actually work though.
    acpi_rsdps_base: u64,
    /// The size of the RSDPs region.
    acpi_rsdps_size: u64,
}

pub struct OsBios {
    boot_disk: usize,
    thunk10: extern "C" fn(),
    thunk13: extern "C" fn(),
    thunk15: extern "C" fn(),
    thunk16: extern "C" fn(),
}

impl Os<
    DiskBios,
    MemoryMapIter,
    VideoModeIter
> for OsBios {
    fn disk(&self) -> DiskBios {
        DiskBios::new(u8::try_from(self.boot_disk).unwrap(), self.thunk13)
    }

    fn memory(&self) -> MemoryMapIter {
        MemoryMapIter::new(self.thunk15)
    }

    fn video_modes(&self) -> VideoModeIter {
        VideoModeIter::new(self.thunk10)
    }

    fn set_video_mode(&self, id: u32) {
        // Set video mode
        let mut data = ThunkData::new();
        data.eax = 0x4F02;
        data.ebx = id;
        unsafe { data.with(self.thunk10); }
        //TODO: check result
    }

    fn get_key(&self) -> OsKey {
        // Read keypress
        let mut data = ThunkData::new();
        unsafe { data.with(self.thunk16); }
        match (data.eax >> 8) as u8 {
            0x4B => OsKey::Left,
            0x4D => OsKey::Right,
            0x48 => OsKey::Up,
            0x50 => OsKey::Down,
            0x1C => OsKey::Enter,
            _ => OsKey::Other,
        }
    }

    fn get_text_position(&self) -> (usize, usize) {
        let vga = VGA.lock();
        (vga.x, vga.y)
    }

    fn set_text_position(&self, x: usize, y: usize) {
        //TODO: ensure this is inside bounds!
        let mut vga = VGA.lock();
        vga.x = x;
        vga.y = y;
    }

    fn set_text_highlight(&self, highlight: bool) {
        let mut vga = VGA.lock();
        if highlight {
            vga.bg = VgaTextColor::White;
            vga.fg = VgaTextColor::Black;
        } else {
            vga.bg = VgaTextColor::DarkGray;
            vga.fg = VgaTextColor::White;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn start(
    kernel_entry: extern "C" fn(
        page_table: usize,
        stack: u64,
        func: u64,
        args: *const KernelArgs,
    ) -> !,
    boot_disk: usize,
    thunk10: extern "C" fn(),
    thunk13: extern "C" fn(),
    thunk15: extern "C" fn(),
    thunk16: extern "C" fn(),
) -> ! {
    {
        // Make sure we are in mode 3 (80x25 text mode)
        let mut data = ThunkData::new();
        data.eax = 0x03;
        data.with(thunk10);
    }

    {
        // Disable cursor
        let mut data = ThunkData::new();
        data.eax = 0x0100;
        data.ecx = 0x3F00;
        data.with(thunk10);
    }

    // Clear screen
    VGA.lock().clear();

    // Set logger
    LOGGER.init();

    let (heap_start, heap_size) = memory_map(thunk15)
        .expect("No memory for heap");

    ALLOCATOR.lock().init(heap_start, heap_size);

    let mut os = OsBios {
        boot_disk,
        thunk10,
        thunk13,
        thunk15,
        thunk16,
    };

    // Locate kernel on RedoxFS
    let disk = os.disk();

    //TODO: get block from partition table
    let block = 1024 * 1024 / redoxfs::BLOCK_SIZE;
    let mut fs = redoxfs::FileSystem::open(disk, Some(block))
        .expect("Failed to open RedoxFS");

    print!("RedoxFS ");
    for i in 0..fs.header.1.uuid.len() {
        if i == 4 || i == 6 || i == 8 || i == 10 {
            print!("-");
        }

        print!("{:>02x}", fs.header.1.uuid[i]);
    }
    println!(": {} MiB", fs.header.1.size / 1024 / 1024);

    let mode_opt = crate::main(&mut os);

    let kernel = {
        let node = fs.find_node("kernel", fs.header.1.root)
            .expect("Failed to find kernel file");

        let size = fs.node_len(node.0)
            .expect("Failed to read kernel size");

        print!("Kernel: 0/{} MiB", size / 1024 / 1024);

        let ptr = ALLOCATOR.alloc_zeroed(
            Layout::from_size_align(size as usize, 4096).unwrap()
        );
        if ptr.is_null() {
            panic!("Failed to allocate memory for kernel");
        }

        let kernel = slice::from_raw_parts_mut(
            ptr,
            size as usize
        );

        let mut i = 0;
        for chunk in kernel.chunks_mut(1024 * 1024) {
            print!("\rKernel: {}/{} MiB", i / 1024 / 1024, size / 1024 / 1024);
            i += fs.read_node(node.0, i, chunk, 0, 0)
                .expect("Failed to read kernel file") as u64;
        }
        println!("\rKernel: {}/{} MiB", i / 1024 / 1024, size / 1024 / 1024);

        let magic = &kernel[..4];
        if magic != b"\x7FELF" {
            panic!("Kernel has invalid magic number {:#X?}", magic);
        }

        kernel
    };

    let page_phys = paging_create(kernel.as_ptr() as usize)
        .expect("Failed to set up paging");

    //TODO: properly reserve page table allocations so kernel does not re-use them

    let stack_size = 0x20000;
    let stack_base = ALLOCATOR.alloc_zeroed(
        Layout::from_size_align(stack_size, 4096).unwrap()
    );
    if stack_base.is_null() {
        panic!("Failed to allocate memory for stack");
    }

    let mut env = String::with_capacity(4096);

    if let Some(mode) = mode_opt {
        env.push_str(&format!("FRAMEBUFFER_ADDR={:016x}\n", mode.base));
        env.push_str(&format!("FRAMEBUFFER_WIDTH={:016x}\n", mode.width));
        env.push_str(&format!("FRAMEBUFFER_HEIGHT={:016x}\n", mode.height));
        os.set_video_mode(mode.id);
    }
    env.push_str(&format!("REDOXFS_BLOCK={:016x}\n", fs.block));
    env.push_str("REDOXFS_UUID=");
    for i in 0..fs.header.1.uuid.len() {
        if i == 4 || i == 6 || i == 8 || i == 10 {
            env.push('-');
        }

        env.push_str(&format!("{:>02x}", fs.header.1.uuid[i]));
    }

    let args = KernelArgs {
        kernel_base: kernel.as_ptr() as u64,
        kernel_size: kernel.len() as u64,
        stack_base: stack_base as u64,
        stack_size: stack_size as u64,
        env_base: env.as_ptr() as u64,
        env_size: env.len() as u64,
        acpi_rsdps_base: 0,
        acpi_rsdps_size: 0,
    };

    kernel_entry(
        page_phys,
        args.stack_base + args.stack_size + PHYS_OFFSET,
        *(kernel.as_ptr().add(0x18) as *const u64),
        &args,
    );
}
