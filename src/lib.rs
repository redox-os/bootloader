#![no_std]
#![feature(asm)]
#![feature(lang_items)]
#![feature(llvm_asm)]

#[macro_use]
extern crate alloc;

use alloc::{
    string::String,
    vec::Vec,
};
use core::{
    alloc::{GlobalAlloc, Layout},
    cmp,
    convert::TryFrom,
    fmt::{self, Write},
    ptr,
    slice,
};
use linked_list_allocator::LockedHeap;
use spin::Mutex;

use self::disk::DiskBios;
use self::logger::LOGGER;
use self::memory_map::memory_map;
use self::thunk::ThunkData;
use self::vbe::{VbeCardInfo, VbeModeInfo};
use self::vga::{VgaTextBlock, VgaTextColor, Vga};

#[macro_use]
mod macros;

mod disk;
mod logger;
mod memory_map;
mod paging;
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

static VGA: Mutex<Vga> = Mutex::new(
    unsafe { Vga::new(VGA_ADDR, 80, 25) }
);

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

#[no_mangle]
pub unsafe extern "C" fn kstart(
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

    let (heap_start, heap_size) = memory_map(thunk15).expect("no memory for heap");

    ALLOCATOR.lock().init(heap_start, heap_size);

    // Locate kernel on RedoxFS
    let disk = DiskBios::new(u8::try_from(boot_disk).unwrap(), thunk13);

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

    let mut modes = Vec::new();
    {
        // Get card info
        let mut data = ThunkData::new();
        data.eax = 0x4F00;
        data.edi = VBE_CARD_INFO_ADDR as u32;
        data.with(thunk10);
        if data.eax == 0x004F {
            let card_info = ptr::read(VBE_CARD_INFO_ADDR as *const VbeCardInfo);

            let mut mode_ptr = card_info.videomodeptr as *const u16;
            loop {
                // Ask for linear frame buffer with mode
                let mode = *mode_ptr | (1 << 14);
                if mode == 0xFFFF {
                    break;
                }
                mode_ptr = mode_ptr.add(1);

                // Get mode info
                let mut data = ThunkData::new();
                data.eax = 0x4F01;
                data.ecx = mode as u32;
                data.edi = VBE_MODE_INFO_ADDR as u32;
                data.with(thunk10);
                if data.eax == 0x004F {
                    let mode_info = ptr::read(VBE_MODE_INFO_ADDR as *const VbeModeInfo);

                    // We only support 32-bits per pixel modes
                    if mode_info.bitsperpixel != 32 {
                        continue;
                    }

                    let w = mode_info.xresolution as u32;
                    let h = mode_info.yresolution as u32;

                    let mut aspect_w = w;
                    let mut aspect_h = h;
                    for i in 2..cmp::min(aspect_w / 2, aspect_h / 2) {
                        while aspect_w % i == 0 && aspect_h % i == 0 {
                            aspect_w /= i;
                            aspect_h /= i;
                        }
                    }

                    //TODO: support resolutions that are not perfect multiples of 4
                    if w % 4 != 0 {
                        continue;
                    }

                    modes.push((
                        mode,
                        w, h,
                        mode_info.physbaseptr,
                        format!("{:>4}x{:<4} {:>3}:{:<3}", w, h, aspect_w, aspect_h)
                    ));
                } else {
                    panic!("Failed to read VBE mode 0x{:04X} info: 0x{:04X}", mode, data.eax);
                }
            }
        } else {
            panic!("Failed to read VBE card info: 0x{:04X}", data.eax);
        }
    }

    // Sort modes by pixel area, reversed
    modes.sort_by(|a, b| (b.1 * b.2).cmp(&(a.1 * a.2)));

    println!();
    println!("Arrow keys and enter select mode");
    println!();
    print!(" ");

    //TODO 0x4F03 VBE function to get current mode
    let off_x = VGA.lock().x;
    let off_y = VGA.lock().y;
    let rows = 12;
    let mut selected = modes.get(0).map_or(0, |x| x.0);
    loop {
        let mut row = 0;
        let mut col = 0;
        for (mode, w, h, ptr, text) in modes.iter() {
            if row >= rows {
                col += 1;
                row = 0;
            }

            VGA.lock().x = off_x + col * 20;
            VGA.lock().y = off_y + row;

            if *mode == selected {
                VGA.lock().bg = VgaTextColor::White;
                VGA.lock().fg = VgaTextColor::Black;
            } else {
                VGA.lock().bg = VgaTextColor::DarkGray;
                VGA.lock().fg = VgaTextColor::White;
            }

            print!("{}", text);

            row += 1;
        }

        // Read keypress
        let mut data = ThunkData::new();
        data.with(thunk16);
        match (data.eax >> 8) as u8 {
            0x4B /* Left */ => {
                if let Some(mut mode_i) = modes.iter().position(|x| x.0 == selected) {
                    if mode_i < rows {
                        while mode_i < modes.len() {
                            mode_i += rows;
                        }
                    }
                    mode_i -= rows;
                    if let Some(new) = modes.get(mode_i) {
                        selected = new.0;
                    }
                }
            },
            0x4D /* Right */ => {
                if let Some(mut mode_i) = modes.iter().position(|x| x.0 == selected) {
                    mode_i += rows;
                    if mode_i >= modes.len() {
                        mode_i = mode_i % rows;
                    }
                    if let Some(new) = modes.get(mode_i) {
                        selected = new.0;
                    }
                }
            },
            0x48 /* Up */ => {
                if let Some(mut mode_i) = modes.iter().position(|x| x.0 == selected) {
                    if mode_i % rows == 0 {
                        mode_i += rows;
                        if mode_i > modes.len() {
                            mode_i = modes.len();
                        }
                    }
                    mode_i -= 1;
                    if let Some(new) = modes.get(mode_i) {
                        selected = new.0;
                    }
                }
            },
            0x50 /* Down */ => {
                if let Some(mut mode_i) = modes.iter().position(|x| x.0 == selected) {
                    mode_i += 1;
                    if mode_i % rows == 0 {
                        mode_i -= rows;
                    }
                    if mode_i >= modes.len() {
                        mode_i = mode_i - mode_i % rows;
                    }
                    if let Some(new) = modes.get(mode_i) {
                        selected = new.0;
                    }
                }
            },
            0x1C /* Enter */ => {
                break;
            },
            _ => (),
        }
    }

    VGA.lock().x = 0;
    VGA.lock().y = off_y + rows;
    VGA.lock().bg = VgaTextColor::DarkGray;
    VGA.lock().fg = VgaTextColor::White;
    println!();

    let kernel = {
        let node = fs.find_node("kernel", fs.header.1.root)
            .expect("failed to find kernel file");

        let size = fs.node_len(node.0)
            .expect("failed to read kernel size");

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

        kernel
    };

    let page_phys = paging::paging_create(kernel.as_ptr() as usize)
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

    if let Some(mode_i) = modes.iter().position(|x| x.0 == selected) {
        if let Some((mode, w, h, ptr, text)) = modes.get(mode_i) {
            let mut data = ThunkData::new();
            data.eax = 0x4F02;
            data.ebx = *mode as u32;
            data.with(thunk10);

            env.push_str(&format!("FRAMEBUFFER_ADDR={:016x}\n", ptr));
            env.push_str(&format!("FRAMEBUFFER_WIDTH={:016x}\n", w));
            env.push_str(&format!("FRAMEBUFFER_HEIGHT={:016x}\n", h));
        }
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
