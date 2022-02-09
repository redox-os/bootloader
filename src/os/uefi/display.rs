use std::proto::Protocol;
use uefi::graphics::GraphicsOutput;
use uefi::guid::{Guid, GRAPHICS_OUTPUT_PROTOCOL_GUID};

pub struct Output(pub &'static mut GraphicsOutput);

impl Protocol<GraphicsOutput> for Output {
    fn guid() -> Guid {
        GRAPHICS_OUTPUT_PROTOCOL_GUID
    }

    fn new(inner: &'static mut GraphicsOutput) -> Self {
        Output(inner)
    }
}
