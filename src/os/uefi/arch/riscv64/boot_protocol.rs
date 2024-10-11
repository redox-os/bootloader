use std::proto::Protocol;
use uefi::guid::Guid;
use uefi::status::{Result, Status};

#[derive(Debug)]
#[repr(C)]
struct RiscVEfiBootProtocol {
    pub revision: u64,
    pub efi_get_boot_hartid:
        unsafe extern "efiapi" fn(this: *mut Self, phartid: *mut usize) -> Status,
}

impl RiscVEfiBootProtocol {
    pub const GUID: Guid = Guid::parse_str("ccd15fec-6f73-4eec-8395-3e69e4b940bf");
    // pub const REVISION: u64 = 0x00010000;
}

struct RiscVEfiBoot(pub &'static mut RiscVEfiBootProtocol);

impl Protocol<RiscVEfiBootProtocol> for RiscVEfiBoot {
    fn guid() -> Guid {
        RiscVEfiBootProtocol::GUID
    }

    fn new(inner: &'static mut RiscVEfiBootProtocol) -> Self {
        Self(inner)
    }
}

impl RiscVEfiBoot {
    pub fn efi_get_boot_hartid(&mut self) -> Result<usize> {
        let mut boot_hartid: usize = 0;
        match unsafe { (self.0.efi_get_boot_hartid)(self.0, &mut boot_hartid) } {
            ok if ok.is_success() => Ok(boot_hartid),
            err => Err(err),
        }
    }
}

pub fn efi_get_boot_hartid() -> Result<usize> {
    let handles = RiscVEfiBoot::locate_handle()?;
    let handle = handles.first().ok_or(Status::NOT_FOUND)?;
    let mut proto = RiscVEfiBoot::handle_protocol(*handle)?;
    proto.efi_get_boot_hartid()
}
