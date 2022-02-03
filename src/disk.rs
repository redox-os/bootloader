use core::{mem, ptr};
use redoxfs::{BLOCK_SIZE, Disk};
use syscall::error::{Error, Result, EIO};

use crate::{DISK_ADDRESS_PACKET_ADDR, DISK_BIOS_ADDR, ThunkData};

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
            size: mem::size_of::<DiskAddressPacket>() as u8,
            reserved: 0,
            blocks: blocks as u16,
            buffer: DISK_BIOS_ADDR as u16,
            segment: 0,
            address: block * blocks,
        }
    }
}

pub struct DiskBios {
    thunk13: extern "C" fn(),
}

impl DiskBios {
    pub fn new(thunk13: extern "C" fn()) -> Self {
        Self { thunk13 }
    }
}

impl Disk for DiskBios {
    unsafe fn read_at(&mut self, block: u64, buffer: &mut [u8]) -> Result<usize> {
        for (i, chunk) in buffer.chunks_mut(BLOCK_SIZE as usize).enumerate() {
            let mut dap = DiskAddressPacket::from_block(block);
            ptr::write(DISK_ADDRESS_PACKET_ADDR as *mut DiskAddressPacket, dap);

            let mut data = ThunkData::new();
            data.ax = 0x4200;
            //TODO: get original drive number!
            data.dx = 0x0080;
            data.si = DISK_ADDRESS_PACKET_ADDR as u16;

            data.with(self.thunk13);

            //TODO: return result on error
            assert_eq!(data.ax, 0);

            //TODO: check blocks transferred
            dap = ptr::read(DISK_ADDRESS_PACKET_ADDR as *mut DiskAddressPacket);

            ptr::copy(DISK_BIOS_ADDR as *const u8, chunk.as_mut_ptr(), chunk.len());
        }

        Ok(buffer.len())
    }

    unsafe fn write_at(&mut self, block: u64, buffer: &[u8]) -> Result<usize> {
        //TODO
        Ok(0)
    }

    fn size(&mut self) -> Result<u64> {
        //TODO
        Ok(0)
    }
}
