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

mod panic;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

#[derive(Clone, Copy)]
#[repr(packed)]
pub struct ThunkData {
    di: u16,
    si: u16,
    bp: u16,
    sp: u16,
    bx: u16,
    dx: u16,
    cx: u16,
    ax: u16,
}

impl ThunkData {
    pub const STACK: usize = 0x7C00;

    pub fn new() -> Self {
        Self {
            di: 0,
            si: 0,
            bp: 0,
            sp: Self::STACK as u16,
            bx: 0,
            dx: 0,
            cx: 0,
            ax: 0,
        }
    }

    pub unsafe fn save(&self) {
        ptr::write((Self::STACK - 16) as *mut ThunkData, *self);
    }

    pub unsafe fn load(&mut self) {
        *self = ptr::read((Self::STACK - 16) as *const ThunkData);
    }

    pub unsafe fn with(&mut self, f: extern "C" fn()) {
        self.save();
        f();
        self.load();
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(packed)]
pub struct VbeCardInfo {
	signature: [u8; 4],
	version: u16,
	oemstring: u32,
	capabilities: u32,
	videomodeptr: u32,
	totalmemory: u16,
	oemsoftwarerev: u16,
	oemvendornameptr: u32,
	oemproductnameptr: u32,
	oemproductrevptr: u32,
	reserved: [u8; 222],
	oemdata: [u8; 256],
}

#[derive(Clone, Copy, Debug)]
#[repr(packed)]
pub struct VbeModeInfo {
	attributes: u16,
	winA: u8,
	winB: u8,
	granularity: u16,
	winsize: u16,
	segmentA: u16,
	segmentB: u16,
	winfuncptr: u32,
	bytesperscanline: u16,
	xresolution: u16,
	yresolution: u16,
	xcharsize: u8,
	ycharsize: u8,
	numberofplanes: u8,
	bitsperpixel: u8,
	numberofbanks: u8,
	memorymodel: u8,
	banksize: u8,
	numberofimagepages: u8,
	unused: u8,
	redmasksize: u8,
	redfieldposition: u8,
	greenmasksize: u8,
	greenfieldposition: u8,
	bluemasksize: u8,
	bluefieldposition: u8,
	rsvdmasksize: u8,
	rsvdfieldposition: u8,
	directcolormodeinfo: u8,
	physbaseptr: u32,
	offscreenmemoryoffset: u32,
	offscreenmemsize: u16,
	reserved: [u8; 206],
}

#[derive(Clone, Copy)]
#[repr(packed)]
pub struct VgaTextBlock {
    char: u8,
    color: u8,
}

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum VgaTextColor {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Purple = 5,
    Brown = 6,
    Gray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    LightPurple = 13,
    Yellow = 14,
    White = 15,
}

pub struct Vga {
    blocks: &'static mut [VgaTextBlock],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    bg: VgaTextColor,
    fg: VgaTextColor,
}

impl Vga {
    pub unsafe fn new(ptr: *mut VgaTextBlock, width: usize, height: usize) -> Self {
        Self {
            blocks: slice::from_raw_parts_mut(
                ptr,
                width * height
            ),
            width,
            height,
            x: 0,
            y: 0,
            bg: VgaTextColor::DarkGray,
            fg: VgaTextColor::White,
        }
    }
}

impl fmt::Write for Vga {
    fn write_str(&mut self, s: &str) -> Result<(), fmt::Error> {
        for c in s.chars() {
            if self.x >= self.width {
                self.x = 0;
                self.y += 1;
            }
            while self.y >= self.height {
                for y in 1..self.height {
                    for x in 0..self.width {
                        let i = y * self.width + x;
                        let j = i - self.width;
                        self.blocks[j] = self.blocks[i];
                        if y + 1 == self.height {
                            self.blocks[i].char = 0;
                        }
                    }
                }
                self.y -= 1;
            }
            match c {
                '\r' => {
                    self.x = 0;
                },
                '\n' => {
                    self.x = 0;
                    self.y += 1;
                },
                _ => {
                    let i = self.y * self.width + self.x;
                    if let Some(block) = self.blocks.get_mut(i) {
                        block.char = c as u8;
                        block.color =
                            ((self.bg as u8) << 4) |
                            (self.fg as u8);
                    }
                }
            }
            self.x += 1;
        }

        Ok(())
    }
}

#[no_mangle]
pub unsafe extern "C" fn kstart(
    thunk10: extern "C" fn(),
    thunk13: extern "C" fn(),
    thunk15: extern "C" fn(),
    thunk16: extern "C" fn(),
) -> ! {
    {
        // Make sure we are in mode 3 (80x25 text mode)
        let mut data = ThunkData::new();
        data.ax = 0x03;
        data.with(thunk10);
    }

    {
        // Disable cursor
        let mut data = ThunkData::new();
        data.ax = 0x0100;
        data.cx = 0x3F00;
        data.with(thunk10);
    }

    let mut vga = Vga::new(0xb8000 as *mut VgaTextBlock, 80, 25);

    for i in 0..vga.blocks.len() {
        vga.blocks[i].char = 0;
        vga.blocks[i].color =
            ((VgaTextColor::DarkGray as u8) << 4) |
            (VgaTextColor::White as u8);
    }

    // Initialize allocator at the end of stage 3 with a meager 1 MiB
    extern "C" {
        static mut __end: u8;
    }
    let heap_start = &__end as *const _ as usize;
    let heap_size = 1024 * 1024;
    ALLOCATOR.lock().init(heap_start, heap_size);

    let mut modes = Vec::new();
    {
        // Get card info
        let mut data = ThunkData::new();
        data.ax = 0x4F00;
        data.di = 0x1000;
        data.with(thunk10);
        if data.ax == 0x004F {
            let card_info = ptr::read(0x1000 as *const VbeCardInfo);

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
                data.ax = 0x4F01;
                data.cx = mode;
                data.di = 0x2000;
                data.with(thunk10);
                if data.ax == 0x004F {
                    let mode_info = ptr::read(0x2000 as *const VbeModeInfo);

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

                    modes.push((mode, w, h, format!("{:>4}x{:<4} {:>3}:{:<3}", w, h, aspect_w, aspect_h)));
                } else {
                    writeln!(vga, "Failed to read VBE mode 0x{:04X} info: 0x{:04X}", mode, data.ax);
                }
            }
        } else {
            writeln!(vga, "Failed to read VBE card info: 0x{:04X}", data.ax);
        }
    }

    // Sort modes by pixel area, reversed
    modes.sort_by(|a, b| (b.1 * b.2).cmp(&(a.1 * a.2)));

    writeln!(vga, "Arrow keys and space select mode, enter to continue");

    //TODO 0x4F03 VBE function to get current mode
    let rows = 12;
    let mut selected = modes.get(0).map_or(0, |x| x.0);
    loop {
        let mut row = 0;
        let mut col = 0;
        for (mode, w, h, text) in modes.iter() {
            if row >= rows {
                col += 1;
                row = 0;
            }

            vga.x = 1 + col * 20;
            vga.y = 1 + row;

            if *mode == selected {
                vga.bg = VgaTextColor::White;
                vga.fg = VgaTextColor::Black;
            } else {
                vga.bg = VgaTextColor::DarkGray;
                vga.fg = VgaTextColor::White;
            }

            write!(vga, "{}", text);

            row += 1;
        }

        // Read keypress
        let mut data = ThunkData::new();
        data.with(thunk16);
        writeln!(
            vga,
            "{:?} 0x{:02X}",
            (data.ax as u8) as char,
            (data.ax >> 8) as u8
        );

        match (data.ax >> 8) as u8 {
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
                data.ax = 0x4F02;
                data.bx = selected;
                data.with(thunk10);
            },
            _ => (),
        }
    }
}
