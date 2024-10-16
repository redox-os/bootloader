use core::ptr;
use log::error;
use uefi::status::Status;

use crate::os::uefi::display::Output;
use crate::os::OsVideoMode;

pub struct VideoModeIter {
    output_opt: Option<Output>,
    i: u32,
}

impl VideoModeIter {
    pub fn new(output_opt: Option<Output>) -> Self {
        Self { output_opt, i: 0 }
    }
}

impl Iterator for VideoModeIter {
    type Item = OsVideoMode;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(ref mut output) = self.output_opt {
            while self.i < output.0.Mode.MaxMode {
                let id = self.i;
                self.i += 1;

                let mut mode_ptr = ::core::ptr::null_mut();
                let mut mode_size = 0;
                match (output.0.QueryMode)(output.0, id, &mut mode_size, &mut mode_ptr) {
                    Status::SUCCESS => (),
                    err => {
                        error!("Failed to read mode {}: {:?}", id, err);
                        continue;
                    }
                }

                //TODO: ensure mode_size is set correctly
                let mode = unsafe { ptr::read(mode_ptr) };

                let width = mode.HorizontalResolution;
                let height = mode.VerticalResolution;
                let stride = mode.PixelsPerScanLine;

                return Some(OsVideoMode {
                    id: id as u32,
                    width,
                    height,
                    stride,
                    // Base is retrieved later by setting the mode
                    base: 0,
                });
            }
        }

        None
    }
}
