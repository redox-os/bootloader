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

const EDID_ACTIVE_PROTOCOL_GUID: Guid = Guid(
    0xbd8c1056, 0x9f36, 0x44ec, [0x92, 0xa8, 0xa6, 0x33, 0x7f, 0x81, 0x79, 0x86]
);

#[allow(non_snake_case)]
#[repr(C)]
pub struct EdidActiveProtocol {
    pub SizeOfEdid: u32,
    pub Edid: *const u8,
}

pub struct EdidActive(pub &'static mut EdidActiveProtocol);

impl Protocol<EdidActiveProtocol> for EdidActive {
    fn guid() -> Guid {
        EDID_ACTIVE_PROTOCOL_GUID
    }

    fn new(inner: &'static mut EdidActiveProtocol) -> Self {
        EdidActive(inner)
    }
}
