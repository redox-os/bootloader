#![no_std]
#![feature(alloc_error_handler)]
#![feature(int_roundings)]
#![feature(lang_items)]
#![allow(internal_features)]
#![feature(let_chains)]
#![cfg_attr(target_os = "uefi", no_main, feature(try_trait_v2))]
#![cfg_attr(target_arch = "riscv64", feature(naked_functions))]

extern crate alloc;

#[cfg(target_os = "uefi")]
#[macro_use]
extern crate uefi_std as std;

use alloc::{format, string::String, vec::Vec};
use core::{
    cmp,
    fmt::{self, Write},
    mem, ptr, slice, str,
};
use redoxfs::Disk;

use self::arch::{paging_create, paging_framebuffer};
use self::os::{Os, OsHwDesc, OsKey, OsMemoryEntry, OsMemoryKind, OsVideoMode};

#[macro_use]
mod os;

mod arch;
mod logger;
mod serial_16550;

const KIBI: usize = 1024;
const MIBI: usize = KIBI * KIBI;

//TODO: allocate this in a more reasonable manner
static mut AREAS: [OsMemoryEntry; 1024] = [OsMemoryEntry {
    base: 0,
    size: 0,
    kind: OsMemoryKind::Null,
}; 1024];
static mut AREAS_LEN: usize = 0;

pub fn area_add(area: OsMemoryEntry) {
    unsafe {
        for existing_area in &mut AREAS[0..AREAS_LEN] {
            if existing_area.kind == area.kind {
                if existing_area.base.unchecked_add(existing_area.size) == area.base {
                    existing_area.size += area.size;
                    return;
                }
                if area.base.unchecked_add(area.size) == existing_area.base {
                    existing_area.base = area.base;
                    return;
                }
            }
        }
        *AREAS.get_mut(AREAS_LEN).expect("AREAS overflowed!") = area;
        AREAS_LEN += 1;
    }
}

pub static mut KERNEL_64BIT: bool = false;

pub static mut LIVE_OPT: Option<(u64, &'static [u8])> = None;

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
#[repr(C, packed(8))]
pub struct KernelArgs {
    kernel_base: u64,
    kernel_size: u64,
    stack_base: u64,
    stack_size: u64,
    env_base: u64,
    env_size: u64,

    /// The base pointer to the saved RSDP.
    ///
    /// This field can be NULL, and if so, the system has not booted with UEFI or in some other way
    /// retrieved the RSDPs. The kernel or a userspace driver will thus try searching the BIOS
    /// memory instead. On UEFI systems, searching is not guaranteed to actually work though.
    acpi_rsdp_base: u64,
    /// The size of the RSDP region.
    acpi_rsdp_size: u64,

    areas_base: u64,
    areas_size: u64,

    bootstrap_base: u64,
    bootstrap_size: u64,
}

fn select_mode<D: Disk, V: Iterator<Item = OsVideoMode>>(
    os: &dyn Os<D, V>,
    output_i: usize,
) -> Option<OsVideoMode> {
    let mut modes = Vec::new();
    for mode in os.video_modes(output_i) {
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
            format!(
                "{:>4}x{:<4} {:>3}:{:<3}",
                mode.width, mode.height, aspect_w, aspect_h
            ),
        ));
    }

    if modes.is_empty() {
        return None;
    }

    // Sort modes by pixel area, reversed
    modes.sort_by(|a, b| (b.0.width * b.0.height).cmp(&(a.0.width * a.0.height)));

    // Set selected based on best resolution
    print!("Output {}", output_i);
    let mut selected = modes.get(0).map_or(0, |x| x.0.id);
    if let Some((best_width, best_height)) = os.best_resolution(output_i) {
        print!(", best resolution: {}x{}", best_width, best_height);
        for (mode, _text) in modes.iter() {
            if mode.width == best_width && mode.height == best_height {
                selected = mode.id;
                break;
            }
        }
    }
    println!();

    println!("Arrow keys and enter select mode");
    println!();
    print!(" ");

    let (off_x, off_y) = os.get_text_position();
    let rows = 12;
    let mut mode_opt = None;
    while !modes.is_empty() {
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
            }
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
            }
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
            }
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
            }
            OsKey::Enter => {
                if let Some(mode_i) = modes.iter().position(|x| x.0.id == selected) {
                    if let Some((mode, _text)) = modes.get(mode_i) {
                        mode_opt = Some(*mode);
                    }
                }
                break;
            }
            _ => (),
        }
    }

    os.set_text_position(0, off_y + rows);
    os.set_text_highlight(false);
    println!();

    mode_opt
}

fn redoxfs<D: Disk, V: Iterator<Item = OsVideoMode>>(
    os: &dyn Os<D, V>,
) -> (redoxfs::FileSystem<D>, Option<&'static [u8]>) {
    let attempts = 10;
    for attempt in 0..=attempts {
        let mut password_opt = None;
        if attempt > 0 {
            print!("\rRedoxFS password ({}/{}): ", attempt, attempts);

            let mut password = String::new();

            loop {
                match os.get_key() {
                    OsKey::Backspace | OsKey::Delete => {
                        if !password.is_empty() {
                            print!("\x08 \x08");
                            password.pop();
                        }
                    }
                    OsKey::Char(c) => {
                        print!("*");
                        password.push(c)
                    }
                    OsKey::Enter => break,
                    _ => (),
                }
            }

            // Erase password information
            while os.get_text_position().0 > 0 {
                print!("\x08 \x08");
            }

            if !password.is_empty() {
                password_opt = Some(password);
            }
        }
        match os.filesystem(password_opt.as_ref().map(|x| x.as_bytes())) {
            Ok(fs) => {
                return (
                    fs,
                    password_opt.map(|password| {
                        // Copy password to page aligned memory
                        let password_size = password.len();
                        let password_base = os.alloc_zeroed_page_aligned(password_size);
                        unsafe {
                            ptr::copy(password.as_ptr(), password_base, password_size);
                            slice::from_raw_parts(password_base, password_size)
                        }
                    }),
                );
            }
            Err(err) => match err.errno {
                // Incorrect password, try again
                syscall::ENOKEY => (),
                _ => {
                    panic!("Failed to open RedoxFS: {}", err);
                }
            },
        }
    }
    panic!("RedoxFS out of unlock attempts");
}

#[derive(PartialEq)]
enum Filetype {
    Elf,
    Initfs,
}
fn load_to_memory<D: Disk>(
    os: &dyn Os<D, impl Iterator<Item = OsVideoMode>>,
    fs: &mut redoxfs::FileSystem<D>,
    dirname: &str,
    filename: &str,
    filetype: Filetype,
) -> &'static mut [u8] {
    fs.tx(|tx| {
        let dir_node = tx
            .find_node(redoxfs::TreePtr::root(), dirname)
            .unwrap_or_else(|err| panic!("Failed to find {} directory: {}", dirname, err));

        let node = tx
            .find_node(dir_node.ptr(), filename)
            .unwrap_or_else(|err| panic!("Failed to find {} file: {}", filename, err));

        let size = node.data().size();

        print!("{}: 0/{} MiB", filename, size / MIBI as u64);

        let ptr = os.alloc_zeroed_page_aligned(size as usize);
        if ptr.is_null() {
            panic!("Failed to allocate memory for {}", filename);
        }

        let slice = unsafe { slice::from_raw_parts_mut(ptr, size as usize) };

        let mut i = 0;
        for chunk in slice.chunks_mut(MIBI) {
            print!(
                "\r{}: {}/{} MiB",
                filename,
                i / MIBI as u64,
                size / MIBI as u64
            );
            i += tx
                .read_node_inner(&node, i, chunk)
                .unwrap_or_else(|err| panic!("Failed to read `{}` file: {}", filename, err))
                as u64;
        }
        println!(
            "\r{}: {}/{} MiB",
            filename,
            i / MIBI as u64,
            size / MIBI as u64
        );

        if filetype == Filetype::Elf {
            let magic = &slice[..4];
            if magic != b"\x7FELF" {
                panic!("{} has invalid magic number {:#X?}", filename, magic);
            }
        } else if filetype == Filetype::Initfs {
            let magic = &slice[..8];
            if magic != b"RedoxFtw" {
                panic!("{} has invalid magic number {:#X?}", filename, magic);
            }
        }

        Ok(slice)
    })
    .unwrap_or_else(|err| {
        panic!(
            "RedoxFS transaction failed while loading `{}`: {}",
            filename, err
        )
    })
}

fn elf_entry(data: &[u8]) -> (u64, bool) {
    match (data[4], data[5]) {
        // 32-bit, little endian
        (1, 1) => (
            u32::from_le_bytes(
                <[u8; 4]>::try_from(&data[0x18..0x18 + 4]).expect("conversion cannot fail"),
            ) as u64,
            false,
        ),
        // 32-bit, big endian
        (1, 2) => (
            u32::from_be_bytes(
                <[u8; 4]>::try_from(&data[0x18..0x18 + 4]).expect("conversion cannot fail"),
            ) as u64,
            false,
        ),
        // 64-bit, little endian
        (2, 1) => (
            u64::from_le_bytes(
                <[u8; 8]>::try_from(&data[0x18..0x18 + 8]).expect("conversion cannot fail"),
            ),
            true,
        ),
        // 64-bit, big endian
        (2, 2) => (
            u64::from_be_bytes(
                <[u8; 8]>::try_from(&data[0x18..0x18 + 8]).expect("conversion cannot fail"),
            ),
            true,
        ),
        (ei_class, ei_data) => {
            panic!("Unsupported ELF EI_CLASS {} EI_DATA {}", ei_class, ei_data);
        }
    }
}

fn main<D: Disk, V: Iterator<Item = OsVideoMode>>(os: &dyn Os<D, V>) -> (usize, u64, KernelArgs) {
    println!(
        "Redox OS Bootloader {} on {}",
        env!("CARGO_PKG_VERSION"),
        os.name()
    );

    let hwdesc = os.hwdesc();
    println!("Hardware descriptor: {:x?}", hwdesc);
    let (acpi_rsdp_base, acpi_rsdp_size) = match hwdesc {
        OsHwDesc::Acpi(base, size) => (base, size),
        OsHwDesc::DeviceTree(base, size) => (base, size),
        OsHwDesc::NotFound => (0, 0),
    };

    let (mut fs, password_opt) = redoxfs(os);

    print!("RedoxFS ");
    for i in 0..fs.header.uuid().len() {
        if i == 4 || i == 6 || i == 8 || i == 10 {
            print!("-");
        }

        print!("{:>02x}", fs.header.uuid()[i]);
    }
    println!(": {} MiB", fs.header.size() / MIBI as u64);
    println!();

    let mut mode_opts = Vec::new();
    for output_i in 0..os.video_outputs() {
        if output_i > 0 {
            os.clear_text();
        }
        mode_opts.push(select_mode(os, output_i));
    }

    let stack_size = 128 * KIBI;
    let stack_base = os.alloc_zeroed_page_aligned(stack_size);
    if stack_base.is_null() {
        panic!("Failed to allocate memory for stack");
    }

    let live_opt = if cfg!(feature = "live") {
        let size = fs.header.size();

        print!("live: 0/{} MiB", size / MIBI as u64);

        let ptr = os.alloc_zeroed_page_aligned(size as usize);
        if ptr.is_null() {
            panic!("Failed to allocate memory for live");
        }

        let live = unsafe { slice::from_raw_parts_mut(ptr, size as usize) };

        let mut i = 0;
        for chunk in live.chunks_mut(MIBI) {
            print!("\rlive: {}/{} MiB", i / MIBI as u64, size / MIBI as u64);
            i += unsafe {
                fs.disk
                    .read_at(fs.block + i / redoxfs::BLOCK_SIZE, chunk)
                    .expect("Failed to read live disk") as u64
            };
        }
        println!("\rlive: {}/{} MiB", i / MIBI as u64, size / MIBI as u64);

        println!("Switching to live disk");
        unsafe {
            LIVE_OPT = Some((fs.block, slice::from_raw_parts_mut(ptr, size as usize)));
        }

        area_add(OsMemoryEntry {
            base: live.as_ptr() as u64,
            size: live.len() as u64,
            kind: OsMemoryKind::Reserved,
        });

        Some(live)
    } else {
        None
    };

    let (kernel, kernel_entry) = {
        let kernel = load_to_memory(os, &mut fs, "boot", "kernel", Filetype::Elf);
        let (kernel_entry, kernel_64bit) = elf_entry(kernel);
        unsafe {
            KERNEL_64BIT = kernel_64bit;
        }
        (kernel, kernel_entry)
    };

    let (bootstrap_size, bootstrap_base) = {
        let initfs_slice = load_to_memory(os, &mut fs, "boot", "initfs", Filetype::Initfs);

        let memory = unsafe {
            let total_size = initfs_slice.len().next_multiple_of(4096);
            let ptr = os.alloc_zeroed_page_aligned(total_size);
            assert!(!ptr.is_null(), "failed to allocate bootstrap+initfs memory");
            core::slice::from_raw_parts_mut(ptr, total_size)
        };
        memory[..initfs_slice.len()].copy_from_slice(initfs_slice);

        (memory.len() as u64, memory.as_mut_ptr() as u64)
    };

    let page_phys = unsafe { paging_create(os, kernel.as_ptr() as u64, kernel.len() as u64) }
        .expect("Failed to set up paging");

    let mut env_size = 64 * KIBI;
    let env_base = os.alloc_zeroed_page_aligned(env_size);
    if env_base.is_null() {
        panic!("Failed to allocate memory for stack");
    }

    {
        let mut w = SliceWriter {
            slice: unsafe { slice::from_raw_parts_mut(env_base, env_size) },
            i: 0,
        };

        writeln!(w, "BOOT_MODE={}", os.name()).unwrap();

        match hwdesc {
            OsHwDesc::Acpi(addr, size) => {
                writeln!(w, "RSDP_ADDR={:016x}", addr).unwrap();
                writeln!(w, "RSDP_SIZE={:016x}", size).unwrap();
            }
            OsHwDesc::DeviceTree(addr, size) => {
                writeln!(w, "DTB_ADDR={:016x}", addr).unwrap();
                writeln!(w, "DTB_SIZE={:016x}", size).unwrap();
            }
            OsHwDesc::NotFound => {}
        }

        if let Some(live) = live_opt {
            writeln!(w, "DISK_LIVE_ADDR={:016x}", live.as_ptr() as usize).unwrap();
            writeln!(w, "DISK_LIVE_SIZE={:016x}", live.len()).unwrap();
            writeln!(w, "REDOXFS_BLOCK={:016x}", 0).unwrap();
        } else {
            writeln!(w, "REDOXFS_BLOCK={:016x}", fs.block).unwrap();
        }
        write!(w, "REDOXFS_UUID=").unwrap();
        for i in 0..fs.header.uuid().len() {
            if i == 4 || i == 6 || i == 8 || i == 10 {
                write!(w, "-").unwrap();
            }

            write!(w, "{:>02x}", fs.header.uuid()[i]).unwrap();
        }
        writeln!(w).unwrap();
        if let Some(password) = password_opt {
            writeln!(
                w,
                "REDOXFS_PASSWORD_ADDR={:016x}",
                password.as_ptr() as usize
            )
            .unwrap();
            writeln!(w, "REDOXFS_PASSWORD_SIZE={:016x}", password.len()).unwrap();
        }

        #[cfg(target_arch = "riscv64")]
        {
            let boot_hartid = os::efi_get_boot_hartid()
                .expect("Could not retrieve boot hart id from EFI implementation!");
            writeln!(w, "BOOT_HART_ID={:016x}", boot_hartid).unwrap();
        }

        for output_i in 0..os.video_outputs() {
            if let Some(mut mode) = mode_opts[output_i] {
                // Set mode to get updated values
                os.set_video_mode(output_i, &mut mode);

                if output_i == 0 {
                    let virt = unsafe {
                        paging_framebuffer(
                            os,
                            page_phys,
                            mode.base,
                            (mode.stride * mode.height * 4) as u64,
                        )
                    }
                    .expect("Failed to map framebuffer");

                    writeln!(w, "FRAMEBUFFER_ADDR={:016x}", mode.base).unwrap();
                    writeln!(w, "FRAMEBUFFER_VIRT={:016x}", virt).unwrap();
                    writeln!(w, "FRAMEBUFFER_WIDTH={:016x}", mode.width).unwrap();
                    writeln!(w, "FRAMEBUFFER_HEIGHT={:016x}", mode.height).unwrap();
                    writeln!(w, "FRAMEBUFFER_STRIDE={:016x}", mode.stride).unwrap();
                } else {
                    writeln!(
                        w,
                        "FRAMEBUFFER{}={:#x},{},{},{}",
                        output_i, mode.base, mode.width, mode.height, mode.stride,
                    )
                    .unwrap();
                }
            }
        }

        env_size = w.i;
    }

    (
        page_phys,
        kernel_entry,
        KernelArgs {
            kernel_base: kernel.as_ptr() as u64,
            kernel_size: kernel.len() as u64,
            stack_base: stack_base as u64,
            stack_size: stack_size as u64,
            env_base: env_base as u64,
            env_size: env_size as u64,
            acpi_rsdp_base,
            acpi_rsdp_size,
            areas_base: unsafe { AREAS.as_ptr() as u64 },
            areas_size: unsafe { (AREAS.len() * mem::size_of::<OsMemoryEntry>()) as u64 },
            bootstrap_base,
            bootstrap_size,
        },
    )
}
