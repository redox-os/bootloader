use core::{mem, ptr};
use redoxfs::{BLOCK_SIZE, Disk};
use syscall::error::{Error, Result, EIO};

use super::{DISK_ADDRESS_PACKET_ADDR, DISK_BIOS_ADDR, ThunkData};

const SECTOR_SIZE: u64 = 512;
const BLOCKS_PER_SECTOR: u64 = BLOCK_SIZE / SECTOR_SIZE;
// 32 sectors is the amount allocated for DISK_BIOS_ADDR
// 127 sectors is the maximum for many BIOSes
const MAX_SECTORS: u64 = 32;
const MAX_BLOCKS: u64 = MAX_SECTORS * SECTOR_SIZE / BLOCK_SIZE;

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(packed)]
pub struct DiskAddressPacket {
    size: u8,
    reserved: u8,
    sectors: u16,
    buffer: u16,
    segment: u16,
    address: u64,
}

impl DiskAddressPacket {
    pub fn from_block(block: u64, count: u64) -> DiskAddressPacket {
        let address = block * BLOCKS_PER_SECTOR;
        let sectors = count * BLOCKS_PER_SECTOR;
        assert!(sectors <= MAX_SECTORS);
        DiskAddressPacket {
            size: mem::size_of::<DiskAddressPacket>() as u8,
            reserved: 0,
            sectors: sectors as u16,
            buffer: DISK_BIOS_ADDR as u16,
            segment: 0,
            address,
        }
    }
}

pub struct DiskBios {
    boot_disk: u8,
    thunk13: extern "C" fn(),
}

impl DiskBios {
    pub fn new(boot_disk: u8, thunk13: extern "C" fn()) -> Self {
        Self { boot_disk, thunk13 }
    }
}

impl Disk for DiskBios {
    unsafe fn read_at(&mut self, block: u64, buffer: &mut [u8]) -> Result<usize> {
        for (i, chunk) in buffer.chunks_mut((MAX_BLOCKS * BLOCK_SIZE) as usize).enumerate() {
            let mut dap = DiskAddressPacket::from_block(
                block + i as u64 * MAX_BLOCKS,
                chunk.len() as u64 / BLOCK_SIZE
            );
            ptr::write(DISK_ADDRESS_PACKET_ADDR as *mut DiskAddressPacket, dap);

            let mut data = ThunkData::new();
            data.eax = 0x4200;
            data.edx = self.boot_disk as u32;
            data.esi = DISK_ADDRESS_PACKET_ADDR as u32;

            data.with(self.thunk13);

            //TODO: return result on error
            assert_eq!({ data.eax }, 0);

            //TODO: check blocks transferred
            dap = ptr::read(DISK_ADDRESS_PACKET_ADDR as *mut DiskAddressPacket);

            ptr::copy(DISK_BIOS_ADDR as *const u8, chunk.as_mut_ptr(), chunk.len());
        }

        Ok(buffer.len())
    }

    unsafe fn write_at(&mut self, block: u64, buffer: &[u8]) -> Result<usize> {
        log::error!(
            "DiskBios::write_at(0x{:X}, 0x{:X}:0x{:X}) not allowed",
            block,
            buffer.as_ptr() as usize,
            buffer.len()
        );
        Err(Error::new(EIO))
    }

    fn size(&mut self) -> Result<u64> {
        log::error!("DiskBios::size not implemented");
        Err(Error::new(EIO))
    }
}
