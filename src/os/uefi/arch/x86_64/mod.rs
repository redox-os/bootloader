use core::{cmp, mem, ptr, slice};
use orbclient::{Color, Renderer};
use std::fs::find;
use std::proto::Protocol;
use std::string::String;
use std::vec::Vec;
use uefi::status::{Error, Result};
use uefi::guid::GuidKind;
use uefi::memory::MemoryType;

use super::super::{
    disk::DiskEfi,
    display::{Display, ScaledDisplay, Output},
    key::{key, Key},
    text::TextDisplay,
};

use self::memory_map::memory_map;
use self::paging::{paging_create, paging_enter};

mod memory_map;
mod paging;
mod partitions;

static PHYS_OFFSET: u64 = 0xFFFF800000000000;

static mut KERNEL_PHYS: u64 = 0;
static mut KERNEL_SIZE: u64 = 0;
static mut KERNEL_ENTRY: u64 = 0;

static mut STACK_PHYS: u64 = 0;
static STACK_SIZE: u64 = 0x20000;

static mut ENV_PHYS: u64 = 0;
static mut ENV_SIZE: u64 = 0;

static mut RSDPS_AREA: Option<Vec<u8>> = None;

#[repr(packed)]
pub struct KernelArgs {
    kernel_base: u64,
    kernel_size: u64,
    stack_base: u64,
    stack_size: u64,
    env_base: u64,
    env_size: u64,

    acpi_rsdps_base: u64,
    acpi_rsdps_size: u64,
}

unsafe fn allocate_zero_pages(pages: usize) -> Result<usize> {
    let uefi = std::system_table();

    let mut ptr = 0;
    (uefi.BootServices.AllocatePages)(
        0, // AllocateAnyPages
        MemoryType::EfiRuntimeServicesData, // Keeps this memory out of free space list
        pages,
        &mut ptr
    )?;

    ptr::write_bytes(ptr as *mut u8, 0, 4096);

    Ok(ptr)
}

unsafe fn exit_boot_services(key: usize) {
    let handle = std::handle();
    let uefi = std::system_table();

    let _ = (uefi.BootServices.ExitBootServices)(handle, key);
}

unsafe fn enter() -> ! {
    let args = KernelArgs {
        kernel_base: KERNEL_PHYS,
        kernel_size: KERNEL_SIZE,
        stack_base: STACK_PHYS,
        stack_size: STACK_SIZE,
        env_base: ENV_PHYS,
        env_size: ENV_SIZE,
        acpi_rsdps_base: RSDPS_AREA.as_ref().map(Vec::as_ptr).unwrap_or(core::ptr::null()) as usize as u64 + PHYS_OFFSET,
        acpi_rsdps_size: RSDPS_AREA.as_ref().map(Vec::len).unwrap_or(0) as u64,
    };

    let entry_fn: extern "sysv64" fn(args_ptr: *const KernelArgs) -> ! = mem::transmute(KERNEL_ENTRY);
    entry_fn(&args);
}

fn get_correct_block_io() -> Result<DiskEfi> {
    // Get all BlockIo handles.
    let mut handles = vec! [uefi::Handle(0); 128];
    let mut size = handles.len() * mem::size_of::<uefi::Handle>();

    (std::system_table().BootServices.LocateHandle)(uefi::boot::LocateSearchType::ByProtocol, &uefi::guid::BLOCK_IO_GUID, 0, &mut size, handles.as_mut_ptr())?;

    let max_size = size / mem::size_of::<uefi::Handle>();
    let actual_size = std::cmp::min(handles.len(), max_size);

    // Return the handle that seems bootable.
    for handle in handles.into_iter().take(actual_size) {
        let block_io = DiskEfi::handle_protocol(handle)?;
        if !block_io.0.Media.LogicalPartition {
            continue;
        }

        let part = partitions::PartitionProto::handle_protocol(handle)?.0;
        if part.sys == 1 {
            continue;
        }
        assert_eq!({part.rev}, partitions::PARTITION_INFO_PROTOCOL_REVISION);
        if part.ty == partitions::PartitionProtoDataTy::Gpt as u32 {
            let gpt = unsafe { part.info.gpt };
            assert_ne!(gpt.part_ty_guid, partitions::ESP_GUID, "detected esp partition again");
            if gpt.part_ty_guid == partitions::REDOX_FS_GUID || gpt.part_ty_guid == partitions::LINUX_FS_GUID {
                return Ok(block_io);
            }
        } else if part.ty == partitions::PartitionProtoDataTy::Mbr as u32 {
            let mbr = unsafe { part.info.mbr };
            if mbr.ty == 0x83 {
                return Ok(block_io);
            }
        } else {
            continue;
        }
    }
    panic!("Couldn't find handle for partition");
}

struct Invalid;

fn validate_rsdp(address: usize, v2: bool) -> core::result::Result<usize, Invalid> {
    #[repr(packed)]
    #[derive(Clone, Copy, Debug)]
    struct Rsdp {
        signature: [u8; 8], // b"RSD PTR "
        chksum: u8,
        oem_id: [u8; 6],
        revision: u8,
        rsdt_addr: u32,
        // the following fields are only available for ACPI 2.0, and are reserved otherwise
        length: u32,
        xsdt_addr: u64,
        extended_chksum: u8,
        _rsvd: [u8; 3],
    }
    // paging is not enabled at this stage; we can just read the physical address here.
    let rsdp_bytes = unsafe { core::slice::from_raw_parts(address as *const u8, core::mem::size_of::<Rsdp>()) };
    let rsdp = unsafe { (rsdp_bytes.as_ptr() as *const Rsdp).as_ref::<'static>().unwrap() };

    println!("RSDP: {:?}", rsdp);

    if rsdp.signature != *b"RSD PTR " {
        return Err(Invalid);
    }
    let mut base_sum = 0u8;
    for base_byte in &rsdp_bytes[..20] {
        base_sum = base_sum.wrapping_add(*base_byte);
    }
    if base_sum != 0 {
        return Err(Invalid);
    }

    if rsdp.revision == 2 {
        let mut extended_sum = 0u8;
        for byte in rsdp_bytes {
            extended_sum = extended_sum.wrapping_add(*byte);
        }

        if extended_sum != 0 {
            return Err(Invalid);
        }
    }

    let length = if rsdp.revision == 2 { rsdp.length as usize } else { core::mem::size_of::<Rsdp>() };

    Ok(length)
}

fn find_acpi_table_pointers() -> Result<()> {
    let rsdps_area = unsafe {
        RSDPS_AREA = Some(Vec::new());
        RSDPS_AREA.as_mut().unwrap()
    };

    let cfg_tables = std::system_table().config_tables();

    for (address, v2) in cfg_tables.iter().find_map(|cfg_table| if cfg_table.VendorGuid.kind() == GuidKind::Acpi { Some((cfg_table.VendorTable, false)) } else if cfg_table.VendorGuid.kind() == GuidKind::Acpi2 { Some((cfg_table.VendorTable, true)) } else { None }) {
        match validate_rsdp(address, v2) {
            Ok(length) => {
                let align = 8;

                rsdps_area.extend(&u32::to_ne_bytes(length as u32));
                rsdps_area.extend(unsafe { core::slice::from_raw_parts(address as *const u8, length) });
                rsdps_area.resize(((rsdps_area.len() + (align - 1)) / align) * align, 0u8);
            }
            Err(_) => println!("Found RSDP that wasn't valid at {:p}", address as *const u8),
        }
    }
    Ok(())
}

fn redoxfs() -> Result<redoxfs::FileSystem<DiskEfi>> {
    // TODO: Scan multiple partitions for a kernel.
    // TODO: pass block_opt for performance reasons
    redoxfs::FileSystem::open(get_correct_block_io()?, None).map_err(|_| Error::DeviceError)
}

const MB: usize = 1024 * 1024;

fn inner() -> Result<()> {
    //TODO: detect page size?
    let page_size = 4096;

    {
        let mut env = String::new();
        if let Ok(output) = Output::one() {
            let mode = &output.0.Mode;
            env.push_str(&format!("FRAMEBUFFER_ADDR={:016x}\n", mode.FrameBufferBase));
            env.push_str(&format!("FRAMEBUFFER_WIDTH={:016x}\n", mode.Info.HorizontalResolution));
            env.push_str(&format!("FRAMEBUFFER_HEIGHT={:016x}\n", mode.Info.VerticalResolution));
        }

        println!("Loading Kernel...");
        let kernel = if let Ok((_i, mut kernel_file)) = find("\\redox_bootloader\\kernel") {
            let info = kernel_file.info()?;
            let len = info.FileSize;

            let kernel = unsafe {
                let ptr = allocate_zero_pages((len as usize + page_size - 1) / page_size)?;
                slice::from_raw_parts_mut(
                    ptr as *mut u8,
                    len as usize
                )
            };

            let mut i = 0;
            for mut chunk in kernel.chunks_mut(4 * MB) {
                print!("\r{}% - {} MB", i as u64 * 100 / len, i / MB);

                let count = kernel_file.read(&mut chunk)?;
                if count == 0 {
                    break;
                }
                //TODO: return error instead of assert
                assert_eq!(count, chunk.len());

                i += count;
            }
            println!("\r{}% - {} MB", i as u64 * 100 / len, i / MB);

            kernel
        } else {
            let mut fs = redoxfs()?;

            let root = fs.header.1.root;
            let node = fs.find_node("kernel", root).map_err(|_| Error::DeviceError)?;

            let len = fs.node_len(node.0).map_err(|_| Error::DeviceError)?;

            let kernel = unsafe {
                let ptr = allocate_zero_pages((len as usize + page_size - 1) / page_size)?;
                println!("{:X}", ptr);

                slice::from_raw_parts_mut(
                    ptr as *mut u8,
                    len as usize
                )
            };

            let mut i = 0;
            for mut chunk in kernel.chunks_mut(4 * MB) {
                print!("\r{}% - {} MB", i as u64 * 100 / len, i / MB);

                let count = fs.read_node(node.0, i as u64, &mut chunk, 0, 0).map_err(|_| Error::DeviceError)?;
                if count == 0 {
                    break;
                }
                //TODO: return error instead of assert
                assert_eq!(count, chunk.len());

                i += count;
            }
            println!("\r{}% - {} MB", i as u64 * 100 / len, i / MB);

            env.push_str(&format!("REDOXFS_BLOCK={:016x}\n", fs.block));
            env.push_str("REDOXFS_UUID=");
            for i in 0..fs.header.1.uuid.len() {
                if i == 4 || i == 6 || i == 8 || i == 10 {
                    env.push('-');
                }

                env.push_str(&format!("{:>02x}", fs.header.1.uuid[i]));
            }

            kernel
        };

        unsafe {
            KERNEL_PHYS = kernel.as_ptr() as u64;
            KERNEL_SIZE = kernel.len() as u64;
            KERNEL_ENTRY = *(kernel.as_ptr().offset(0x18) as *const u64);
            println!("Kernel {:X}:{:X} entry {:X}", KERNEL_PHYS, KERNEL_SIZE, KERNEL_ENTRY);
        }

        println!("Allocating stack {:X}", STACK_SIZE);
        unsafe {
            STACK_PHYS = allocate_zero_pages(STACK_SIZE as usize / page_size)? as u64;
            println!("Stack {:X}:{:X}", STACK_PHYS, STACK_SIZE);
        }

        println!("Allocating env {:X}", env.len());
        unsafe {
            ENV_PHYS = allocate_zero_pages((env.len() + page_size - 1) / page_size)? as u64;
            ENV_SIZE = env.len() as u64;
            ptr::copy(env.as_ptr(), ENV_PHYS as *mut u8, env.len());
            println!("Env {:X}:{:X}", ENV_PHYS, ENV_SIZE);
        }

        println!("Parsing and writing ACPI RSDP structures.");
        find_acpi_table_pointers();

        println!("Done!");
    }

    println!("Creating page tables");
    let page_phys = unsafe {
        paging_create(KERNEL_PHYS)?
    };

    println!("Entering kernel");
    unsafe {
        let key = memory_map();
        exit_boot_services(key);
    }

    unsafe {
        llvm_asm!("cli" : : : "memory" : "intel", "volatile");
        paging_enter(page_phys);
    }

    unsafe {
        llvm_asm!("mov rsp, $0" : : "r"(STACK_PHYS + PHYS_OFFSET + STACK_SIZE) : "memory" : "intel", "volatile");
        enter();
    }
}

fn draw_text(display: &mut ScaledDisplay, mut x: i32, y: i32, text: &str, color: Color) {
    for c in text.chars() {
        display.char(x, y, c, color);
        x += 8;
    }
}

fn draw_background(display: &mut ScaledDisplay) {
    let bg = Color::rgb(0x4a, 0xa3, 0xfd);

    display.set(bg);

    {
        let prompt = format!(
            "Redox Bootloader {} {}",
            env!("CARGO_PKG_VERSION"),
            env!("TARGET").split('-').next().unwrap_or("")
        );
        let x = (display.width() as i32 - prompt.len() as i32 * 8)/2;
        let y = display.height() as i32 - 32;
        draw_text(display, x, y, &prompt, Color::rgb(0xff, 0xff, 0xff));
    }
}

fn select_mode(output: &mut Output) -> Result<()> {
    // Read all available modes
    let mut modes = Vec::new();
    for i in 0..output.0.Mode.MaxMode {
        let mut mode_ptr = ::core::ptr::null_mut();
        let mut mode_size = 0;
        (output.0.QueryMode)(output.0, i, &mut mode_size, &mut mode_ptr)?;

        let mode = unsafe { &mut *mode_ptr };
        let w = mode.HorizontalResolution;
        let h = mode.VerticalResolution;

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

        modes.push((i, w, h, format!("{:>4}x{:<4} {:>3}:{:<3}", w, h, aspect_w, aspect_h)));
    }

    // Sort modes by pixel area, reversed
    modes.sort_by(|a, b| (b.1 * b.2).cmp(&(a.1 * a.2)));

    // Find current mode index
    let mut selected = output.0.Mode.Mode;

    // If there are no modes from querymode, don't change mode
    if modes.is_empty() {
        return Ok(());
    }

    let white = Color::rgb(0xff, 0xff, 0xff);
    let black = Color::rgb(0x00, 0x00, 0x00);
    let rows = 12;
    loop {
        {
            // Create a scaled display
            let mut display = Display::new(output);
            let mut display = ScaledDisplay::new(&mut display);

            draw_background(&mut display);

            let off_x = (display.width() as i32 - 60 * 8)/2;
            let mut off_y = 16;
            draw_text(
                &mut display,
                off_x, off_y,
                "Arrow keys and enter select mode",
                white
            );
            off_y += 24;

            let mut row = 0;
            let mut col = 0;
            for (i, w, h, text) in modes.iter() {
                if row >= rows as i32 {
                    col += 1;
                    row = 0;
                }

                let x = off_x + col * 20 * 8;
                let y = off_y + row * 16;

                let fg = if *i == selected {
                    display.rect(x - 8, y, text.len() as u32 * 8 + 16, 16, white);
                    black
                } else {
                    white
                };

                draw_text(&mut display, x, y, text, fg);

                row += 1;
            }

            display.sync();
        }

        match key(true)? {
            Key::Left => {
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
            Key::Right => {
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
            Key::Up => {
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
            Key::Down => {
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
            Key::Enter => {
                (output.0.SetMode)(output.0, selected)?;
                return Ok(());
            },
            _ => (),
        }
    }
}

fn pretty_pipe<T, F: FnMut() -> Result<T>>(output: &mut Output, f: F) -> Result<T> {
    let mut display = Display::new(output);

    let mut display = ScaledDisplay::new(&mut display);

    draw_background(&mut display);

    display.sync();

    {
        let cols = 80;
        let off_x = (display.width() as i32 - cols as i32 * 8)/2;
        let off_y = 16;
        let rows = (display.height() as i32 - 64 - off_y - 1) as usize/16;
        display.rect(off_x, off_y, cols as u32 * 8, rows as u32 * 16, Color::rgb(0, 0, 0));
        display.sync();

        let mut text = TextDisplay::new(display);
        text.off_x = off_x;
        text.off_y = off_y;
        text.cols = cols;
        text.rows = rows;
        text.pipe(f)
    }
}

pub fn main() -> Result<()> {
    if let Ok(mut output) = Output::one() {
        select_mode(&mut output)?;

        pretty_pipe(&mut output, inner)?;
    } else {
        inner()?;
    }

    Ok(())
}
