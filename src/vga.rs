use core::{fmt, slice};

#[derive(Clone, Copy)]
#[repr(packed)]
pub struct VgaTextBlock {
    pub char: u8,
    pub color: u8,
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
    pub blocks: &'static mut [VgaTextBlock],
    pub width: usize,
    pub height: usize,
    pub x: usize,
    pub y: usize,
    pub bg: VgaTextColor,
    pub fg: VgaTextColor,
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
