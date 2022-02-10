use core::ptr;
use log::error;

use crate::os::OsVideoMode;

use super::{ThunkData, VBE_CARD_INFO_ADDR, VBE_MODE_INFO_ADDR};

#[derive(Clone, Copy, Debug)]
#[repr(packed)]
pub struct VbeFarPtr {
    pub offset: u16,
    pub segment: u16,
}

impl VbeFarPtr {
    pub unsafe fn as_ptr<T>(&self) -> *const T {
        (((self.segment as usize) << 4) + (self.offset as usize)) as *const T
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(packed)]
pub struct VbeCardInfo {
    pub signature: [u8; 4],
    pub version: u16,
    pub oemstring: VbeFarPtr,
    pub capabilities: [u8; 4],
    pub videomodeptr: VbeFarPtr,
    pub totalmemory: u16,
    pub oemsoftwarerev: u16,
    pub oemvendornameptr: VbeFarPtr,
    pub oemproductnameptr: VbeFarPtr,
    pub oemproductrevptr: VbeFarPtr,
    pub reserved: [u8; 222],
    pub oemdata: [u8; 256],
}

#[derive(Clone, Copy, Debug)]
#[repr(packed)]
pub struct VbeModeInfo {
    pub attributes: u16,
    pub win_a: u8,
    pub win_b: u8,
    pub granularity: u16,
    pub winsize: u16,
    pub segment_a: u16,
    pub segment_b: u16,
    pub winfuncptr: u32,
    pub bytesperscanline: u16,
    pub xresolution: u16,
    pub yresolution: u16,
    pub xcharsize: u8,
    pub ycharsize: u8,
    pub numberofplanes: u8,
    pub bitsperpixel: u8,
    pub numberofbanks: u8,
    pub memorymodel: u8,
    pub banksize: u8,
    pub numberofimagepages: u8,
    pub unused: u8,
    pub redmasksize: u8,
    pub redfieldposition: u8,
    pub greenmasksize: u8,
    pub greenfieldposition: u8,
    pub bluemasksize: u8,
    pub bluefieldposition: u8,
    pub rsvdmasksize: u8,
    pub rsvdfieldposition: u8,
    pub directcolormodeinfo: u8,
    pub physbaseptr: u32,
    pub offscreenmemoryoffset: u32,
    pub offscreenmemsize: u16,
    pub reserved: [u8; 206],
}

pub struct VideoModeIter {
    thunk10: extern "C" fn(),
    mode_ptr: *const u16,
}

impl VideoModeIter {
    pub fn new(thunk10: extern "C" fn()) -> Self {
        // Get card info
        let mut data = ThunkData::new();
        data.eax = 0x4F00;
        data.edi = VBE_CARD_INFO_ADDR as u32;
        unsafe { data.with(thunk10); }
        let mode_ptr = if data.eax == 0x004F {
            let card_info = unsafe { ptr::read(VBE_CARD_INFO_ADDR as *const VbeCardInfo) };
            unsafe { card_info.videomodeptr.as_ptr::<u16>() }
        } else {
            error!("Failed to read VBE card info: 0x{:04X}", { data.eax });
            ptr::null()
        };
        Self {
            thunk10,
            mode_ptr
        }
    }
}

impl Iterator for VideoModeIter {
    type Item = OsVideoMode;
    fn next(&mut self) -> Option<Self::Item> {
        if self.mode_ptr.is_null() {
            return None;
        }

        loop {
            // Set bit 14 to get linear frame buffer
            let mode = unsafe { *self.mode_ptr } | (1 << 14);
            if mode == 0xFFFF {
                return None;
            }
            self.mode_ptr = unsafe { self.mode_ptr.add(1) };

            // Get mode info
            let mut data = ThunkData::new();
            data.eax = 0x4F01;
            data.ecx = mode as u32;
            data.edi = VBE_MODE_INFO_ADDR as u32;
            unsafe { data.with(self.thunk10); }
            if data.eax == 0x004F {
                let mode_info = unsafe { ptr::read(VBE_MODE_INFO_ADDR as *const VbeModeInfo) };

                // We only support 32-bits per pixel modes
                if mode_info.bitsperpixel != 32 {
                    continue;
                }

                let width = mode_info.xresolution as u32;
                let height = mode_info.yresolution as u32;

                //TODO: support resolutions that are not perfect multiples of 4
                if width % 4 != 0 {
                    continue;
                }

                return Some(OsVideoMode {
                    id: mode as u32,
                    width,
                    height,
                    base: mode_info.physbaseptr as u64,
                });
            } else {
                error!("Failed to read VBE mode 0x{:04X} info: 0x{:04X}", mode, { data.eax });
            }
        }
    }
}
