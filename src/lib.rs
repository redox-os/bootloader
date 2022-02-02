#![no_std]
#![feature(lang_items)]
#![feature(llvm_asm)]

use core::slice;

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

#[no_mangle]
pub unsafe extern "C" fn kstart() -> ! {
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
