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

use alloc::{
    vec::Vec,
};
use core::{
    cmp,
    fmt::{self, Write},
    slice,
    str,
};
use redoxfs::Disk;

use self::arch::paging_create;
use self::os::{Os, OsKey, OsMemoryEntry, OsVideoMode};

#[macro_use]
mod os;

mod arch;
mod logger;

const KIBI: usize = 1024;
const MIBI: usize = KIBI * KIBI;

struct SliceWriter<'a> {
    slice: &'a mut [u8],
    i: usize,
}

impl<'a> Write for SliceWriter<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if let Some(slice_b) = self.slice.get_mut(self.i) {
                *slice_b = b;
                self.i += 1;
            } else {
                return Err(fmt::Error);
            }
        }
        Ok(())
    }
}

#[allow(dead_code)]
#[derive(Debug)]
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

fn main<
    D: Disk,
    M: Iterator<Item=OsMemoryEntry>,
    V: Iterator<Item=OsVideoMode>
>(os: &mut dyn Os<D, M, V>) -> (usize, KernelArgs) {
    let mut fs = os.filesystem();

    print!("RedoxFS ");
    for i in 0..fs.header.1.uuid.len() {
        if i == 4 || i == 6 || i == 8 || i == 10 {
            print!("-");
        }

        print!("{:>02x}", fs.header.1.uuid[i]);
    }
    println!(": {} MiB", fs.header.1.size / MIBI as u64);

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
    let mut mode_opt = None;
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
                if let Some(mode_i) = modes.iter().position(|x| x.0.id == selected) {
                    if let Some((mode, _text)) = modes.get(mode_i) {
                        mode_opt = Some(*mode);
                    }
                }
                break;
            },
            _ => (),
        }
    }

    os.set_text_position(0, off_y + rows);
    os.set_text_highlight(false);
    println!();

    let stack_size = 128 * KIBI;
    let stack_base = os.alloc_zeroed_page_aligned(stack_size);
    if stack_base.is_null() {
        panic!("Failed to allocate memory for stack");
    }

    let kernel = {
        let node = fs.find_node("kernel", fs.header.1.root)
            .expect("Failed to find kernel file");

        let size = fs.node_len(node.0)
            .expect("Failed to read kernel size");

        print!("Kernel: 0/{} MiB", size / MIBI as u64);

        let ptr = os.alloc_zeroed_page_aligned(size as usize);
        if ptr.is_null() {
            panic!("Failed to allocate memory for kernel");
        }

        let kernel = unsafe {
            slice::from_raw_parts_mut(ptr, size as usize)
        };

        let mut i = 0;
        for chunk in kernel.chunks_mut(MIBI) {
            print!("\rKernel: {}/{} MiB", i / MIBI as u64, size / MIBI as u64);
            i += fs.read_node(node.0, i, chunk, 0, 0)
                .expect("Failed to read kernel file") as u64;
        }
        println!("\rKernel: {}/{} MiB", i / MIBI as u64, size / MIBI as u64);

        let magic = &kernel[..4];
        if magic != b"\x7FELF" {
            panic!("Kernel has invalid magic number {:#X?}", magic);
        }

        kernel
    };

    let page_phys = unsafe { paging_create(os, kernel.as_ptr() as usize) }
        .expect("Failed to set up paging");
    //TODO: properly reserve page table allocations so kernel does not re-use them

    let live_opt = if cfg!(feature = "live") {
        let size = fs.header.1.size;

        print!("Live: 0/{} MiB", size / MIBI as u64);

        let ptr = os.alloc_zeroed_page_aligned(size as usize);
        if ptr.is_null() {
            panic!("Failed to allocate memory for live");
        }

        let live = unsafe {
            slice::from_raw_parts_mut(ptr, size as usize)
        };

        let mut i = 0;
        for chunk in live.chunks_mut(MIBI) {
            print!("\rLive: {}/{} MiB", i / MIBI as u64, size / MIBI as u64);
            i += fs.disk.read_at(fs.block + i / redoxfs::BLOCK_SIZE, chunk)
                .expect("Failed to read live disk") as u64;
        }
        println!("\rLive: {}/{} MiB", i / MIBI as u64, size / MIBI as u64);

        Some(live)
    } else {
        None
    };
    //TODO: properly reserve live disk so kernel does not re-use it

    let mut env_size = 4 * KIBI;
    let env_base = os.alloc_zeroed_page_aligned(env_size);
    if env_base.is_null() {
        panic!("Failed to allocate memory for stack");
    }

    {
        let mut w = SliceWriter {
            slice: unsafe {
                slice::from_raw_parts_mut(env_base, env_size)
            },
            i: 0,
        };

        if let Some(live) = live_opt {
            writeln!(w, "DISK_LIVE_ADDR={:016x}", live.as_ptr() as usize).unwrap();
            writeln!(w, "DISK_LIVE_SIZE={:016x}", live.len()).unwrap();
            writeln!(w, "REDOXFS_BLOCK={:016x}", 0).unwrap();
        } else {
            writeln!(w, "REDOXFS_BLOCK={:016x}", fs.block).unwrap();
        }
        write!(w, "REDOXFS_UUID=").unwrap();
        for i in 0..fs.header.1.uuid.len() {
            if i == 4 || i == 6 || i == 8 || i == 10 {
                write!(w, "-").unwrap();
            }

            write!(w, "{:>02x}", fs.header.1.uuid[i]).unwrap();
        }
        writeln!(w).unwrap();

        if let Some(mut mode) = mode_opt {
            // Set mode to get updated values
            os.set_video_mode(&mut mode);

            writeln!(w, "FRAMEBUFFER_ADDR={:016x}", mode.base).unwrap();
            writeln!(w, "FRAMEBUFFER_WIDTH={:016x}", mode.width).unwrap();
            writeln!(w, "FRAMEBUFFER_HEIGHT={:016x}", mode.height).unwrap();
        }

        env_size = w.i;
    }

    (
        page_phys,
        KernelArgs {
            kernel_base: kernel.as_ptr() as u64,
            kernel_size: kernel.len() as u64,
            stack_base: stack_base as u64,
            stack_size: stack_size as u64,
            env_base: env_base as u64,
            env_size: env_size as u64,
            acpi_rsdps_base: 0,
            acpi_rsdps_size: 0,
        }
    )
}
