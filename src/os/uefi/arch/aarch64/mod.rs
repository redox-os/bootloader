use alloc::{
    string::String,
};
use core::{mem, ptr, slice};
use std::fs::find;
use std::proto::Protocol;
use uefi::guid::Guid;
use uefi::memory::MemoryType;
use uefi::status::{Error, Result};

use super::super::{
    disk::DiskEfi,
    display::{Output},
};

use self::memory_map::memory_map;
use self::paging::paging;

mod memory_map;
mod paging;

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
    let entry_fn: extern "C" fn(dtb: u64) -> ! = mem::transmute(
        KERNEL_PHYSICAL + KERNEL_ENTRY - KERNEL_OFFSET
    );
    entry_fn(DTB_PHYSICAL);
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
    for (i, block_io) in DiskEfi::all().into_iter().enumerate() {
        if !block_io.0.Media.LogicalPartition {
            continue;
        }

        match redoxfs::FileSystem::open(block_io, Some(0)) {
            Ok(ok) => return Ok(ok),
            Err(err) => {
                log::error!("Failed to open RedoxFS on block I/O {}: {}", i, err);
            }
        }
    }
    panic!("Failed to find RedoxFS");
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

pub fn main() -> Result<()> {
    inner()?;

    Ok(())
}
