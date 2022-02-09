use core::{ops::{ControlFlow, Try}, ptr};
use log::error;
use std::proto::Protocol;

use crate::os::OsVideoMode;
use crate::os::uefi::display::Output;

pub struct VideoModeIter {
    output_opt: Option<Output>,
    i: u32,
}

impl VideoModeIter {
    pub fn new() -> Self {
        Self {
            output_opt: Output::one().ok(),
            i: 0,
        }
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
                match (output.0.QueryMode)(output.0, id, &mut mode_size, &mut mode_ptr).branch() {
                    ControlFlow::Continue(_) => (),
                    ControlFlow::Break(err) => {
                        error!("Failed to read mode {}: {:?}", id, err);
                        continue;
                    }
                }

                //TODO: ensure mode_size is set correctly
                let mode = unsafe { ptr::read(mode_ptr) };

                let width = mode.HorizontalResolution;
                let height = mode.VerticalResolution;

                //TODO: support resolutions that are not perfect multiples of 4
                if width % 4 != 0 {
                    continue;
                }

                return Some(OsVideoMode {
                    id: id as u32,
                    width,
                    height,
                    // TODO
                    base: 0,
                });
            }
        }

        None
    }
}
