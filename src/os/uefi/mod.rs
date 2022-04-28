use core::{
    ops::{ControlFlow, Try},
    ptr,
    slice
};
use std::{
    proto::Protocol,
};
use uefi::{
    reset::ResetType,
    memory::MemoryType,
    status::{Result, Status},
    system::SystemTable,
    text::TextInputKey,
};

use crate::os::{
    Os,
    OsKey,
    OsVideoMode,
};

use self::{
    disk::DiskEfi,
    display::{EdidActive, Output},
    video_mode::VideoModeIter,
};

mod acpi;
mod arch;
mod disk;
mod display;
mod dtb;
mod memory_map;
mod video_mode;

pub struct OsEfi {
    st: &'static SystemTable,
}

impl Os<
    DiskEfi,
    VideoModeIter
> for OsEfi {
    #[cfg(target_arch = "aarch64")]
    fn name(&self) -> &str {
        "aarch64/UEFI"
    }

    #[cfg(target_arch = "x86_64")]
    fn name(&self) -> &str {
        "x86_64/UEFI"
    }

    fn alloc_zeroed_page_aligned(&self, size: usize) -> *mut u8 {
        assert!(size != 0);

        let page_size = self.page_size();
        let pages = (size + page_size - 1) / page_size;

        let ptr = {
            // Max address mapped by src/arch paging code (8 GiB)
            let mut ptr = 0x2_0000_0000;
            status_to_result(
                (self.st.BootServices.AllocatePages)(
                    1, // AllocateMaxAddress
                    MemoryType::EfiRuntimeServicesData, // Keeps this memory out of free space list
                    pages,
                    &mut ptr
                )
            ).unwrap();
            ptr as *mut u8
        };

        assert!(!ptr.is_null());
        unsafe { ptr::write_bytes(ptr, 0, pages * page_size) };
        ptr
    }

    fn page_size(&self) -> usize {
        4096
    }

    fn filesystem(&self, password_opt: Option<&[u8]>) -> syscall::Result<redoxfs::FileSystem<DiskEfi>> {
        for block_io in DiskEfi::all().into_iter() {
            if !block_io.0.Media.LogicalPartition {
                continue;
            }

            match redoxfs::FileSystem::open(block_io, password_opt, Some(0), false) {
                Ok(ok) => return Ok(ok),
                Err(err) => match err.errno {
                    // Ignore header not found error
                    syscall::ENOENT => (),
                    // Return any other errors
                    _ => {
                        return Err(err)
                    }
                }
            }
        }
        Err(syscall::Error::new(syscall::ENOENT))
    }

    fn video_modes(&self) -> VideoModeIter {
        VideoModeIter::new()
    }

    fn set_video_mode(&self, mode: &mut OsVideoMode) {
        let output = Output::one().unwrap();
        status_to_result(
            (output.0.SetMode)(output.0, mode.id)
        ).unwrap();

        // Update frame buffer base
        mode.base = output.0.Mode.FrameBufferBase as u64;
    }

    fn best_resolution(&self) -> Option<(u32, u32)> {
        //TODO: get this per output
        match EdidActive::one() {
            Ok(efi_edid) => {
                let edid = unsafe {
                    slice::from_raw_parts(efi_edid.0.Edid, efi_edid.0.SizeOfEdid as usize)
                };

                if edid.len() > 0x3D {
                    Some((
                        (edid[0x38] as u32) | (((edid[0x3A] as u32) & 0xF0) << 4),
                        (edid[0x3B] as u32) | (((edid[0x3D] as u32) & 0xF0) << 4),
                    ))
                } else {
                    log::warn!("EFI EDID too small: {}", edid.len());
                    None
                }
            },
            Err(err) => {
                log::warn!("Failed to get EFI EDID: {:?}", err);

                // Fallback to the current output resolution
                match Output::one() {
                    Ok(output) => {
                        Some((
                            output.0.Mode.Info.HorizontalResolution,
                            output.0.Mode.Info.VerticalResolution,
                        ))
                    },
                    Err(err) => {
                        log::error!("Failed to get output: {:?}", err);
                        None
                    }
                }
            }
        }
    }

    fn get_key(&self) -> OsKey {
        //TODO: do not unwrap

        let mut index = 0;
        status_to_result(
            (self.st.BootServices.WaitForEvent)(1, &self.st.ConsoleIn.WaitForKey, &mut index)
        ).unwrap();

        let mut key = TextInputKey {
            ScanCode: 0,
            UnicodeChar: 0
        };
        status_to_result(
            (self.st.ConsoleIn.ReadKeyStroke)(self.st.ConsoleIn, &mut key)
        ).unwrap();

        match key.ScanCode {
            0 => match key.UnicodeChar {
                8 => OsKey::Backspace,
                13 => OsKey::Enter,
                w => match char::from_u32(w as u32) {
                    Some(c) => OsKey::Char(c),
                    None => OsKey::Other,
                },
            },
            1 => OsKey::Up,
            2 => OsKey::Down,
            3 => OsKey::Right,
            4 => OsKey::Left,
            8 => OsKey::Delete,
            _ => OsKey::Other,
        }
    }

    fn get_text_position(&self) -> (usize, usize) {
        (
            self.st.ConsoleOut.Mode.CursorColumn as usize,
            self.st.ConsoleOut.Mode.CursorRow as usize,
        )
    }

    fn set_text_position(&self, x: usize, y: usize) {
        status_to_result(
            (self.st.ConsoleOut.SetCursorPosition)(self.st.ConsoleOut, x, y)
        ).unwrap();
    }

    fn set_text_highlight(&self, highlight: bool) {
        let attr = if highlight { 0x70 } else { 0x07 };
        status_to_result(
            (self.st.ConsoleOut.SetAttribute)(self.st.ConsoleOut, attr)
        ).unwrap();
    }
}

fn status_to_result(status: Status) -> Result<usize> {
    match status.branch() {
        ControlFlow::Continue(ok) => Ok(ok),
        ControlFlow::Break(err) => Err(err),
    }
}

fn set_max_mode(output: &uefi::text::TextOutput) -> Result<()> {
    let mut max_i = None;
    let mut max_w = 0;
    let mut max_h = 0;

    for i in 0..output.Mode.MaxMode as usize {
        let mut w = 0;
        let mut h = 0;
        if (output.QueryMode)(output, i, &mut w, &mut h).branch().is_continue() {
            if w >= max_w && h >= max_h {
                max_i = Some(i);
                max_w = w;
                max_h = h;
            }
        }
    }

    if let Some(i) = max_i {
        (output.SetMode)(output, i)?;
    }

    Ok(())
}

#[no_mangle]
pub extern "C" fn main() -> Status {
    let uefi = std::system_table();

    let _ = (uefi.BootServices.SetWatchdogTimer)(0, 0, 0, ptr::null());

    if let Err(err) = set_max_mode(uefi.ConsoleOut) {
        println!("Failed to set max mode: {:?}", err);
    }

    if let Err(err) = arch::main() {
        panic!("App error: {:?}", err);
    }

    (uefi.RuntimeServices.ResetSystem)(ResetType::Cold, Status(0), 0, ptr::null());
}
