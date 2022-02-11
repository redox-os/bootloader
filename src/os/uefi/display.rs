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

const EDID_DISCOVERED_PROTOCOL_GUID: Guid = Guid(
    0x1c0c34f6, 0xd380, 0x41fa, [0xa0, 0x49, 0x8a, 0xd0, 0x6c, 0x1a, 0x66, 0xaa]
);

#[repr(C)]
pub struct EdidDiscoveredProtocol {
    pub SizeOfEdid: u32,
    pub Edid: *const u8,
}

pub struct EdidDiscovered(pub &'static mut EdidDiscoveredProtocol);

impl Protocol<EdidDiscoveredProtocol> for EdidDiscovered {
    fn guid() -> Guid {
        EDID_DISCOVERED_PROTOCOL_GUID
    }

    fn new(inner: &'static mut EdidDiscoveredProtocol) -> Self {
        EdidDiscovered(inner)
    }
}
