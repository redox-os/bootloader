#![no_std]
#![feature(asm)]
#![feature(lang_items)]
#![feature(llvm_asm)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;
use core::{
    cmp,
    fmt::{self, Write},
    ptr,
    slice,
};
use linked_list_allocator::LockedHeap;
use log::{error, info};

use self::disk::DiskBios;
use self::logger::LOGGER;
use self::thunk::ThunkData;
use self::vbe::{VbeCardInfo, VbeModeInfo};
use self::vga::{VgaTextBlock, VgaTextColor, Vga};

mod disk;
mod logger;
mod panic;
mod thunk;
mod vbe;
mod vga;

// Real mode memory allocation, for use with thunk
// 0x500 to 0x7BFF is free
const VBE_CARD_INFO_ADDR: usize = 0x500; // 512 bytes, ends at 0x6FF
const VBE_MODE_INFO_ADDR: usize = 0x700; // 256 bytes, ends at 0x7FF
const DISK_ADDRESS_PACKET_ADDR: usize = 0x0FF0; // 16 bytes, ends at 0x0FFF
const DISK_BIOS_ADDR: usize = 0x1000; // 4096 bytes, ends at 0x1FFF
const THUNK_STACK_ADDR: usize = 0x7C00; // Grows downwards
const VGA_ADDR: usize = 0xB8000;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

static mut VGA: Vga = unsafe { Vga::new(VGA_ADDR as *mut VgaTextBlock, 80, 25) };

#[no_mangle]
pub unsafe extern "C" fn kstart(
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

    {
        // Clear VGA console
        let blocks = VGA.blocks();
        for i in 0..blocks.len() {
            blocks[i] = VgaTextBlock {
                char: 0,
                color: ((VGA.bg as u8) << 4) | (VGA.fg as u8),
            };
        }
    }

    // Initialize allocator at the end of stage 3 with a meager 1 MiB
    extern "C" {
        static mut __end: u8;
    }
    let heap_start = &__end as *const _ as usize;
    let heap_size = 1024 * 1024;
    ALLOCATOR.lock().init(heap_start, heap_size);

    // Set logger
    LOGGER.init();

    // Locate RedoxFS
    {
        //TODO: ensure boot_disk is 8-bit
        info!("DISK {:02X}", boot_disk);
        let disk = DiskBios::new(boot_disk as u8, thunk13);
        //TODO: get block from partition table
        let block = 1024 * 1024 / redoxfs::BLOCK_SIZE;
        match redoxfs::FileSystem::open(disk, Some(block)) {
            Ok(mut fs) => {
                info!("RedoxFS {} MiB", fs.header.1.size / 1024 / 1024);

                match fs.find_node("kernel", fs.header.1.root) {
                    Ok(node) => match fs.node_len(node.0) {
                        Ok(len) => {
                            info!("Kernel {} MiB", len / 1024 / 1024);
                        },
                        Err(err) => {
                            error!("Failed to read kernel file length: {:?}", err);
                        }
                    },
                    Err(err) => {
                        error!("Failed to find kernel file: {:?}", err);
                    }
                }
            },
            Err(err) => {
                error!("Failed to open RedoxFS: {:?}", err);
            }
        }
    }

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
                    error!("Failed to read VBE mode 0x{:04X} info: 0x{:04X}", mode, data.eax);
                }
            }
        } else {
            error!("Failed to read VBE card info: 0x{:04X}", data.eax);
        }
    }

    // Sort modes by pixel area, reversed
    modes.sort_by(|a, b| (b.1 * b.2).cmp(&(a.1 * a.2)));

    writeln!(VGA, "Arrow keys and enter select mode").unwrap();

    //TODO 0x4F03 VBE function to get current mode
    let off_x = VGA.x;
    let off_y = VGA.y;
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

            VGA.x = off_x + col * 20;
            VGA.y = off_y + row;

            if *mode == selected {
                VGA.bg = VgaTextColor::White;
                VGA.fg = VgaTextColor::Black;
            } else {
                VGA.bg = VgaTextColor::DarkGray;
                VGA.fg = VgaTextColor::White;
            }

            write!(VGA, "{}", text).unwrap();

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
                let mut data = ThunkData::new();
                data.eax = 0x4F02;
                data.ebx = selected as u32;
                data.with(thunk10);
            },
            _ => (),
        }
    }
}
