#![no_std]
#![feature(asm)]
#![feature(lang_items)]
#![feature(llvm_asm)]
#![cfg_attr(
    target_os = "uefi",
    no_main,
    feature(control_flow_enum),
    feature(try_trait_v2),
)]

#[cfg_attr(target_os = "none", macro_use)]
extern crate alloc;

#[cfg(target_os = "uefi")]
#[macro_use]
extern crate uefi_std as std;

use alloc::vec::Vec;
use core::cmp;
use redoxfs::Disk;

use self::os::{Os, OsKey, OsMemoryEntry, OsVideoMode};

#[macro_use]
mod os;

mod arch;
mod logger;

fn main<
    D: Disk,
    M: Iterator<Item=OsMemoryEntry>,
    V: Iterator<Item=OsVideoMode>
>(os: &mut dyn Os<D, M, V>) -> Option<OsVideoMode> {
    let mut modes = Vec::new();
    for mode in os.video_modes() {
        let mut aspect_w = mode.width;
        let mut aspect_h = mode.height;
        for i in 2..cmp::min(aspect_w / 2, aspect_h / 2) {
            while aspect_w % i == 0 && aspect_h % i == 0 {
                aspect_w /= i;
                aspect_h /= i;
            }
        }

        modes.push((
            mode,
            format!("{:>4}x{:<4} {:>3}:{:<3}", mode.width, mode.height, aspect_w, aspect_h)
        ));
    }

    // Sort modes by pixel area, reversed
    modes.sort_by(|a, b| (b.0.width * b.0.height).cmp(&(a.0.width * a.0.height)));

    println!();
    println!("Arrow keys and enter select mode");
    println!();
    print!(" ");

    let (off_x, off_y) = os.get_text_position();
    let rows = 12;
    //TODO 0x4F03 VBE function to get current mode
    let mut selected = modes.get(0).map_or(0, |x| x.0.id);
    while ! modes.is_empty() {
        let mut row = 0;
        let mut col = 0;
        for (mode, text) in modes.iter() {
            if row >= rows {
                col += 1;
                row = 0;
            }

            os.set_text_position(off_x + col * 20, off_y + row);
            os.set_text_highlight(mode.id == selected);

            print!("{}", text);

            row += 1;
        }

        // Read keypress
        match os.get_key() {
            OsKey::Left => {
                if let Some(mut mode_i) = modes.iter().position(|x| x.0.id == selected) {
                    if mode_i < rows {
                        while mode_i < modes.len() {
                            mode_i += rows;
                        }
                    }
                    mode_i -= rows;
                    if let Some(new) = modes.get(mode_i) {
                        selected = new.0.id;
                    }
                }
            },
            OsKey::Right => {
                if let Some(mut mode_i) = modes.iter().position(|x| x.0.id == selected) {
                    mode_i += rows;
                    if mode_i >= modes.len() {
                        mode_i = mode_i % rows;
                    }
                    if let Some(new) = modes.get(mode_i) {
                        selected = new.0.id;
                    }
                }
            },
            OsKey::Up => {
                if let Some(mut mode_i) = modes.iter().position(|x| x.0.id == selected) {
                    if mode_i % rows == 0 {
                        mode_i += rows;
                        if mode_i > modes.len() {
                            mode_i = modes.len();
                        }
                    }
                    mode_i -= 1;
                    if let Some(new) = modes.get(mode_i) {
                        selected = new.0.id;
                    }
                }
            },
            OsKey::Down => {
                if let Some(mut mode_i) = modes.iter().position(|x| x.0.id == selected) {
                    mode_i += 1;
                    if mode_i % rows == 0 {
                        mode_i -= rows;
                    }
                    if mode_i >= modes.len() {
                        mode_i = mode_i - mode_i % rows;
                    }
                    if let Some(new) = modes.get(mode_i) {
                        selected = new.0.id;
                    }
                }
            },
            OsKey::Enter => {
                break;
            },
            _ => (),
        }
    }

    os.set_text_position(0, off_y + rows);
    os.set_text_highlight(false);
    println!();

    if let Some(mode_i) = modes.iter().position(|x| x.0.id == selected) {
        if let Some((mode, _text)) = modes.get(mode_i) {
            return Some(*mode);
        }
    }

    None
}
