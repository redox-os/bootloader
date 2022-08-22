use alloc::alloc::{alloc_zeroed, Layout};
use core::{
    convert::TryFrom,
    slice,
};
use linked_list_allocator::LockedHeap;
use spin::Mutex;

use crate::KernelArgs;
use crate::logger::LOGGER;
use crate::os::{Os, OsKey, OsVideoMode};

use self::disk::DiskBios;
use self::memory_map::memory_map;
use self::thunk::ThunkData;
use self::vbe::VideoModeIter;
use self::vga::{VgaTextColor, Vga};

#[macro_use]
mod macros;

mod disk;
mod memory_map;
mod panic;
mod thunk;
mod vbe;
mod vga;

// Real mode memory allocation, for use with thunk
// 0x500 to 0x7BFF is free
const DISK_BIOS_ADDR: usize = 0x1000; // 16384 bytes, ends at 0x4FFF
const VBE_CARD_INFO_ADDR: usize = 0x5000; // 512 bytes, ends at 0x51FF
const VBE_MODE_INFO_ADDR: usize = 0x5200; // 256 bytes, ends at 0x52FF
const VBE_EDID_ADDR: usize = 0x5300; // 128 bytes, ends at 0x537F
const MEMORY_MAP_ADDR: usize = 0x5380; // 24 bytes, ends at 0x5397
const DISK_ADDRESS_PACKET_ADDR: usize = 0x5398; // 16 bytes, ends at 0x53A7
const THUNK_STACK_ADDR: usize = 0x7C00; // Grows downwards
const VGA_ADDR: usize = 0xB8000;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub(crate) static VGA: Mutex<Vga> = Mutex::new(
    unsafe { Vga::new(VGA_ADDR, 80, 25) }
);

pub struct OsBios {
    boot_disk: usize,
    thunk10: extern "C" fn(),
    thunk13: extern "C" fn(),
    thunk15: extern "C" fn(),
    thunk16: extern "C" fn(),
}

impl Os<
    DiskBios,
    VideoModeIter
> for OsBios {
    fn name(&self) -> &str {
        "x86/BIOS"
    }

    fn alloc_zeroed_page_aligned(&self, size: usize) -> *mut u8 {
        assert!(size != 0);

        let page_size = self.page_size();
        let pages = (size + page_size - 1) / page_size;

        let ptr = unsafe {
            alloc_zeroed(Layout::from_size_align(
                pages * page_size,
                page_size
            ).unwrap())
        };

        assert!(!ptr.is_null());
        ptr
    }

    fn page_size(&self) -> usize {
        4096
    }

    fn filesystem(&self, password_opt: Option<&[u8]>) -> syscall::Result<redoxfs::FileSystem<DiskBios>> {
        let disk = DiskBios::new(u8::try_from(self.boot_disk).unwrap(), self.thunk13);

        //TODO: get block from partition table
        let block = crate::MIBI as u64 / redoxfs::BLOCK_SIZE;
        redoxfs::FileSystem::open(disk, password_opt, Some(block), false)
    }

    fn video_modes(&self) -> VideoModeIter {
        VideoModeIter::new(self.thunk10)
    }

    fn set_video_mode(&self, mode: &mut OsVideoMode) {
        // Set video mode
        let mut data = ThunkData::new();
        data.eax = 0x4F02;
        data.ebx = mode.id;
        unsafe { data.with(self.thunk10); }
        //TODO: check result
    }

    fn best_resolution(&self) -> Option<(u32, u32)> {
        let mut data = ThunkData::new();
        data.eax = 0x4F15;
        data.ebx = 0x01;
        data.ecx = 0;
        data.edx = 0;
        data.edi = VBE_EDID_ADDR as u32;
        unsafe { data.with(self.thunk10); }

        if data.eax == 0x4F {
            let edid = unsafe {
                slice::from_raw_parts(VBE_EDID_ADDR as *const u8, 128)
            };

            Some((
                (edid[0x38] as u32) | (((edid[0x3A] as u32) & 0xF0) << 4),
                (edid[0x3B] as u32) | (((edid[0x3D] as u32) & 0xF0) << 4),
            ))
        } else {
            log::warn!("Failed to get VBE EDID: 0x{:X}", { data.eax });
            None
        }
    }

    fn get_key(&self) -> OsKey {
        // Read keypress
        let mut data = ThunkData::new();
        unsafe { data.with(self.thunk16); }
        match (data.eax >> 8) as u8 {
            0x4B => OsKey::Left,
            0x4D => OsKey::Right,
            0x48 => OsKey::Up,
            0x50 => OsKey::Down,
            0x0E => OsKey::Backspace,
            0x53 => OsKey::Delete,
            0x1C => OsKey::Enter,
            _ => match data.eax as u8 {
                0 => OsKey::Other,
                b => OsKey::Char(b as char),
            }
        }
    }

    fn get_text_position(&self) -> (usize, usize) {
        let vga = VGA.lock();
        (vga.x, vga.y)
    }

    fn set_text_position(&self, x: usize, y: usize) {
        //TODO: ensure this is inside bounds!
        let mut vga = VGA.lock();
        vga.x = x;
        vga.y = y;
    }

    fn set_text_highlight(&self, highlight: bool) {
        let mut vga = VGA.lock();
        if highlight {
            vga.bg = VgaTextColor::Gray;
            vga.fg = VgaTextColor::Black;
        } else {
            vga.bg = VgaTextColor::Black;
            vga.fg = VgaTextColor::Gray;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn start(
    kernel_entry: extern "C" fn(
        page_table: usize,
        stack: u64,
        func: u64,
        args: *const KernelArgs,
        long_mode: usize,
    ) -> !,
    boot_disk: usize,
    thunk10: extern "C" fn(),
    thunk13: extern "C" fn(),
    thunk15: extern "C" fn(),
    thunk16: extern "C" fn(),
) -> ! {
    {
        // Make sure we are in mode 3 (80x25 text mode)
        let mut data = ThunkData::new();
        data.eax = 0x03;
        data.with(thunk10);
    }

    {
        // Disable cursor
        let mut data = ThunkData::new();
        data.eax = 0x0100;
        data.ecx = 0x3F00;
        data.with(thunk10);
    }

    // Clear screen
    VGA.lock().clear();

    // Set logger
    LOGGER.init();

    let mut os = OsBios {
        boot_disk,
        thunk10,
        thunk13,
        thunk15,
        thunk16,
    };

    let (heap_start, heap_size) = memory_map(os.thunk15)
        .expect("No memory for heap");

    ALLOCATOR.lock().init(heap_start, heap_size);

    let (page_phys, func, args) = crate::main(&mut os);

    kernel_entry(
        page_phys,
        args.stack_base + args.stack_size + if crate::KERNEL_64BIT {
            crate::arch::x64::PHYS_OFFSET as u64
        } else {
            crate::arch::x32::PHYS_OFFSET as u64
        },
        func,
        &args,
        if crate::KERNEL_64BIT { 1 } else { 0 },
    );
}
