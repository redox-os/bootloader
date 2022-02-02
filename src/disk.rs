use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use syscall::error::{Error, Result, EIO};

use crate::disk::Disk;
use crate::BLOCK_SIZE;

#[derive(Clone, Copy)]
#[repr(packed)]
pub struct DiskAddressPacket {
    size: u8,
    reserved: u8,
    blocks: u16,
    buffer: u16,
    segment: u16,
    address: u64,
}

impl DiskAddressPacket {
    pub fn from_block(block: u64) -> DiskAddressPacket {
        let blocks = BLOCK_SIZE / 512;
        DiskAddressPacket {
            size: mem::size_of::<DiskAddressPacket>(),
            reserved: 0,
            blocks,
            buffer: DISK_BIOS_ADDR,
            segment: 0,
            address: block * blocks,
        }
    }
}

pub struct DiskBios {
    thunk13: extern "C" fn(),
}

impl DiskBios {
    pub fn block_dap(block: u64) -> DiskAddressPacket {
    }
}

impl Disk for DiskBios {
    unsafe fn read_at(&mut self, block: u64, buffer: &mut [u8]) -> Result<usize> {
        let mut dap = DiskAddressPacket::from_block(block);

        try_disk!(self.file.seek(SeekFrom::Start(block * BLOCK_SIZE)));
        let count = try_disk!(self.file.read(buffer));
        Ok(count)
    }

    unsafe fn write_at(&mut self, block: u64, buffer: &[u8]) -> Result<usize> {
        try_disk!(self.file.seek(SeekFrom::Start(block * BLOCK_SIZE)));
        let count = try_disk!(self.file.write(buffer));
        Ok(count)
    }

    fn size(&mut self) -> Result<u64> {
        let size = try_disk!(self.file.seek(SeekFrom::End(0)));
        Ok(size)
    }
}
