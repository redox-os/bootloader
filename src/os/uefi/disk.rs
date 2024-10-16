use core::slice;
use redoxfs::{Disk, BLOCK_SIZE, RECORD_SIZE};
use std::proto::Protocol;
use syscall::{Error, Result, EINVAL, EIO};
use uefi::block_io::BlockIo as UefiBlockIo;
use uefi::guid::{Guid, BLOCK_IO_GUID};

pub struct DiskEfi(pub &'static mut UefiBlockIo, &'static mut [u8]);

impl Protocol<UefiBlockIo> for DiskEfi {
    fn guid() -> Guid {
        BLOCK_IO_GUID
    }

    fn new(inner: &'static mut UefiBlockIo) -> Self {
        // Hack to get aligned buffer
        let block = unsafe {
            let ptr = super::alloc_zeroed_page_aligned(RECORD_SIZE as usize);
            slice::from_raw_parts_mut(ptr, RECORD_SIZE as usize)
        };

        Self(inner, block)
    }
}

impl Disk for DiskEfi {
    unsafe fn read_at(&mut self, block: u64, buffer: &mut [u8]) -> Result<usize> {
        // Optimization for live disks
        if let Some(live) = crate::LIVE_OPT {
            if block >= live.0 {
                let start = ((block - live.0) * BLOCK_SIZE) as usize;
                let end = start + buffer.len();
                if end <= live.1.len() {
                    buffer.copy_from_slice(&live.1[start..end]);
                    return Ok(buffer.len());
                }
            }
        }

        // Use aligned buffer if necessary
        let mut ptr = buffer.as_mut_ptr();
        if self.0.Media.IoAlign != 0 {
            if (ptr as usize) % (self.0.Media.IoAlign as usize) != 0 {
                if buffer.len() <= self.1.len() {
                    ptr = self.1.as_mut_ptr();
                } else {
                    println!(
                        "DiskEfi::read_at 0x{:X} requires alignment, ptr = 0x{:p}, len = 0x{:x}",
                        block,
                        ptr,
                        buffer.len()
                    );
                    return Err(Error::new(EINVAL));
                }
            }
        }

        let block_size = self.0.Media.BlockSize as u64;
        let lba = block * BLOCK_SIZE / block_size;

        match (self.0.ReadBlocks)(self.0, self.0.Media.MediaId, lba, buffer.len(), ptr) {
            status if status.is_success() => {
                // Copy to original buffer if using aligned buffer
                if ptr != buffer.as_mut_ptr() {
                    let (left, _) = self.1.split_at(buffer.len());
                    buffer.copy_from_slice(left);
                }
                Ok(buffer.len())
            }
            err => {
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
