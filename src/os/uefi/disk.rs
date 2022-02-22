use core::ops::{ControlFlow, Try};
use redoxfs::{BLOCK_SIZE, Disk};
use syscall::{EIO, Error, Result};
use std::proto::Protocol;
use uefi::guid::{Guid, BLOCK_IO_GUID};
use uefi::block_io::BlockIo as UefiBlockIo;

pub struct DiskEfi(pub &'static mut UefiBlockIo);

impl Protocol<UefiBlockIo> for DiskEfi {
    fn guid() -> Guid {
        BLOCK_IO_GUID
    }

    fn new(inner: &'static mut UefiBlockIo) -> Self {
        Self(inner)
    }
}

impl Disk for DiskEfi {
    unsafe fn read_at(&mut self, block: u64, buffer: &mut [u8]) -> Result<usize> {
        let block_size = self.0.Media.BlockSize as u64;

        let lba = block * BLOCK_SIZE / block_size;

        match (self.0.ReadBlocks)(self.0, self.0.Media.MediaId, lba, buffer.len(), buffer.as_mut_ptr()).branch() {
            ControlFlow::Continue(_) => Ok(buffer.len()),
            ControlFlow::Break(err) => {
                println!("DiskEfi::read_at 0x{:X} failed: {:?}", block, err);
                Err(Error::new(EIO))
            }
        }
    }

    unsafe fn write_at(&mut self, block: u64, _buffer: &[u8]) -> Result<usize> {
        println!("DiskEfi::write_at 0x{:X} not implemented", block);
        Err(Error::new(EIO))
    }

    fn size(&mut self) -> Result<u64> {
        println!("DiskEfi::size not implemented");
        Err(Error::new(EIO))
    }
}
