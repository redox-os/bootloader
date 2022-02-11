use uefi::guid::Guid;
use uefi::status::{Error, Result};

static DTB_GUID: Guid = Guid(0xb1b621d5, 0xf19c, 0x41a5, [0x83, 0x0b, 0xd9, 0x15, 0x2c, 0x69, 0xaa, 0xe0]);

pub(crate) fn find_dtb() -> Result<u64> {
    let cfg_tables = std::system_table().config_tables();
    for cfg_table in cfg_tables.iter() {
        if cfg_table.VendorGuid == DTB_GUID {
            let addr = cfg_table.VendorTable as u64;
            println!("DTB: {:X}", addr);
            return Ok(addr);
        }
    }
    println!("Failed to find DTB");
    Err(Error::NotFound)
}
