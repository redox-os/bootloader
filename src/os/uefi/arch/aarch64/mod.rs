use alloc::{
    string::String,
    vec::Vec,
};
use core::{mem, ptr, slice};
use orbclient::{Color, Renderer};
use std::fs::find;
use std::proto::Protocol;
use uefi::guid::Guid;
use uefi::memory::MemoryType;
use uefi::status::{Error, Result};

use super::super::{
    disk::DiskEfi,
    display::{Display, ScaledDisplay, Output},
    key::{key, Key},
    text::TextDisplay,
};

use self::memory_map::memory_map;
use self::paging::paging;

mod memory_map;
mod paging;
mod partitions;

static KERNEL: &'static str = "\\redox_bootloader\\kernel";

static KERNEL_OFFSET: u64 = 0xFFFF_FF00_0000_0000;

static KERNEL_PHYSICAL: u64 = 0x4000_0000;
static mut KERNEL_SIZE: u64 = 0;
static mut KERNEL_ENTRY: u64 = 0;

static mut DTB_PHYSICAL: u64 = 0;

#[no_mangle]
pub extern "C" fn __chkstk() {
    //TODO
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
    let entry_fn: extern "C" fn(dtb: u64) -> ! = mem::transmute((
        KERNEL_PHYSICAL + KERNEL_ENTRY - KERNEL_OFFSET
    ));
    entry_fn(DTB_PHYSICAL);
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

static DTB_GUID: Guid = Guid(0xb1b621d5, 0xf19c, 0x41a5, [0x83, 0x0b, 0xd9, 0x15, 0x2c, 0x69, 0xaa, 0xe0]);

fn find_dtb() -> Result<()> {
    let cfg_tables = std::system_table().config_tables();
    for cfg_table in cfg_tables.iter() {
        if cfg_table.VendorGuid == DTB_GUID {
            unsafe {
                DTB_PHYSICAL = cfg_table.VendorTable as u64;
                println!("DTB: {:X}", DTB_PHYSICAL);
            }
            return Ok(());
        }
    }
    println!("Failed to find DTB");
    Err(Error::NotFound)
}

fn redoxfs() -> Result<redoxfs::FileSystem<DiskEfi>> {
    // TODO: Scan multiple partitions for a kernel.
    // TODO: pass block_opt for performance reasons
    redoxfs::FileSystem::open(get_correct_block_io()?, None)
        .map_err(|_| Error::DeviceError)
}

const MB: usize = 1024 * 1024;

fn inner() -> Result<()> {
    find_dtb()?;

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

        println!("Copying Kernel...");
        unsafe {
            KERNEL_SIZE = kernel.len() as u64;
            println!("Size: {}", KERNEL_SIZE);
            KERNEL_ENTRY = *(kernel.as_ptr().offset(0x18) as *const u64);
            println!("Entry: {:X}", KERNEL_ENTRY);
            ptr::copy(kernel.as_ptr(), KERNEL_PHYSICAL as *mut u8, kernel.len());
        }

        println!("Done!");
    }

    unsafe {
        let key = memory_map();
        exit_boot_services(key);
    }

    unsafe {
        asm!("msr daifset, #2");
        paging();
    }

    unsafe {
        enter();
    }
}

fn select_mode(output: &mut Output) -> Result<u32> {
    loop {
        for i in 0..output.0.Mode.MaxMode {
            let mut mode_ptr = ::core::ptr::null_mut();
            let mut mode_size = 0;
            (output.0.QueryMode)(output.0, i, &mut mode_size, &mut mode_ptr)?;

            let mode = unsafe { &mut *mode_ptr };
            let w = mode.HorizontalResolution;
            let h = mode.VerticalResolution;

            print!("\r{}x{}: Is this OK? (y)es/(n)o", w, h);

            if key(true)? == Key::Character('y') {
                println!("");

                return Ok(i);
            }
        }
    }
}

fn pretty_pipe<T, F: FnMut() -> Result<T>>(f: F) -> Result<T> {
    let mut output = Output::one()?;
    let mut display = Display::new(&mut output);
    let mut display = ScaledDisplay::new(&mut display);

    {
        let bg = Color::rgb(0x4a, 0xa3, 0xfd);

        display.set(bg);

        {
            let prompt = format!(
                "Redox Bootloader {} {}",
                env!("CARGO_PKG_VERSION"),
                env!("TARGET").split('-').next().unwrap_or("")
            );
            let mut x = (display.width() as i32 - prompt.len() as i32 * 8)/2;
            let y = display.height() as i32 - 32;
            for c in prompt.chars() {
                display.char(x, y, c, Color::rgb(0xff, 0xff, 0xff));
                x += 8;
            }
        }

        display.sync();
    }

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
    inner()?;

    /* TODO
    if let Ok(mut output) = Output::one() {
        let mut splash = Image::new(0, 0);
        {
            println!("Loading Splash...");
            if let Ok(image) = image::bmp::parse(&SPLASHBMP) {
                splash = image;
            }
            println!(" Done");
        }

        /* TODO
        let mode = pretty_pipe(&splash, || {
            select_mode(&mut output)
        })?;
        (output.0.SetMode)(output.0, mode)?;
        */

        pretty_pipe(&splash, inner)?;
    } else {
        inner()?;
    }
    */

    Ok(())
}
