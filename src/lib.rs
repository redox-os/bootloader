#![no_std]
#![feature(asm)]
#![feature(lang_items)]
#![feature(llvm_asm)]

use core::{
    fmt::{self, Write},
    ptr,
    slice,
};

mod panic;

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

#[derive(Clone, Copy)]
#[repr(packed)]
pub struct VgaTextBlock {
    char: u8,
    color: u8,
}

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

    writeln!(vga, "Arrow keys and space select mode, enter to continue");

    loop {
        // Read keypress
        let mut data = ThunkData::new();
        data.with(thunk16);
        writeln!(
            vga,
            "'{}' 0x{:02X}",
            (data.ax as u8) as char,
            (data.ax >> 8) as u8
        );
    }
}
