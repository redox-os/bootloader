#![no_std]
#![feature(asm)]
#![feature(lang_items)]
#![feature(llvm_asm)]

use core::{
    ptr,
    slice,
};

mod panic;

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

#[no_mangle]
pub unsafe extern "C" fn kstart(
    thunk10: extern "C" fn(),
    thunk13: extern "C" fn(),
) -> ! {
    {
        let mut data = ThunkData::new();
        data.ax = 0x03;
        data.with(thunk10);
    }

    let vga = slice::from_raw_parts_mut(
        0xb8000 as *mut VgaTextBlock,
        80 * 25
    );

    for i in 0..vga.len() {
        vga[i].char = 0;
        vga[i].color =
            ((VgaTextColor::DarkGray as u8) << 4) |
            (VgaTextColor::White as u8);
    }

    let draw_text = |vga: &mut [VgaTextBlock], x: usize, y: usize, text: &str| {
        let mut i = y * 80 + x;
        for c in text.chars() {
            if let Some(block) = vga.get_mut(i) {
                block.char = c as u8;
            }
            i += 1;
        }
    };

    draw_text(
        vga,
        10, 1,
        "Arrow keys and space select mode, enter to continue"
    );

    loop {}
}
