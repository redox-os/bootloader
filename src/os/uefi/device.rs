use alloc::{string::String, vec::Vec};
use core::{fmt::Write, mem, ptr, slice};
use uefi::{
    device::{
        DevicePath, DevicePathAcpiType, DevicePathBbsType, DevicePathEndType,
        DevicePathHardwareType, DevicePathMediaType, DevicePathMessagingType, DevicePathType,
    },
    guid::Guid,
    Handle,
};
use uefi_std::{loaded_image::LoadedImage, proto::Protocol};

use super::disk::DiskEfi;

#[derive(Debug)]
enum DevicePathRelation {
    This,
    Parent(usize),
    Child(usize),
    None,
}

fn device_path_relation(a_path: &DevicePath, b_path: &DevicePath) -> DevicePathRelation {
    let mut a_iter = DevicePathIter::new(a_path);
    let mut b_iter = DevicePathIter::new(b_path);
    loop {
        match (a_iter.next(), b_iter.next()) {
            (None, None) => return DevicePathRelation::This,
            (None, Some(_)) => return DevicePathRelation::Parent(b_iter.count()),
            (Some(_), None) => return DevicePathRelation::Child(a_iter.count()),
            (Some((a_node, a_data)), Some((b_node, b_data))) => {
                if a_node.Type != b_node.Type {
                    return DevicePathRelation::None;
                }

                if a_node.SubType != b_node.SubType {
                    return DevicePathRelation::None;
                }

                if a_data != b_data {
                    return DevicePathRelation::None;
                }
            }
        }
    }
}

pub struct DiskDevice {
    pub handle: Handle,
    pub disk: DiskEfi,
    pub device_path: DevicePathProtocol,
}

pub fn disk_device_priority() -> Vec<DiskDevice> {
    // Get the handle of the partition this program was loaded from, which should be the ESP
    let esp_handle = match LoadedImage::handle_protocol(std::handle()) {
        Ok(loaded_image) => loaded_image.0.DeviceHandle,
        Err(err) => {
            log::warn!("Failed to find LoadedImage protocol: {:?}", err);
            return Vec::new();
        }
    };

    // Get the device path of the ESP
    let esp_device_path = match DevicePathProtocol::handle_protocol(esp_handle) {
        Ok(ok) => ok,
        Err(err) => {
            log::warn!(
                "Failed to find device path protocol on {:?}: {:?}",
                esp_handle,
                err
            );
            return Vec::new();
        }
    };

    // Get all block I/O handles along with their block I/O implementations and device paths
    let handles = match DiskEfi::locate_handle() {
        Ok(ok) => ok,
        Err(err) => {
            log::warn!("Failed to find block I/O handles: {:?}", err);
            Vec::new()
        }
    };
    let mut devices = Vec::with_capacity(handles.len());
    for handle in handles {
        let disk = match DiskEfi::handle_protocol(handle) {
            Ok(ok) => ok,
            Err(err) => {
                log::warn!(
                    "Failed to find block I/O protocol on {:?}: {:?}",
                    handle,
                    err
                );
                continue;
            }
        };

        let device_path = match DevicePathProtocol::handle_protocol(handle) {
            Ok(ok) => ok,
            Err(err) => {
                log::warn!(
                    "Failed to find device path protocol on {:?}: {:?}",
                    handle,
                    err
                );
                continue;
            }
        };

        devices.push(DiskDevice {
            handle,
            disk,
            device_path,
        });
    }

    // Find possible boot disks
    let mut boot_disks = Vec::with_capacity(1);
    {
        let mut i = 0;
        while i < devices.len() {
            match device_path_relation(devices[i].device_path.0, esp_device_path.0) {
                DevicePathRelation::Parent(0) => {
                    boot_disks.push(devices.remove(i));
                    continue;
                }
                _ => (),
            }

            i += 1;
        }
    }

    // Find all children of possible boot devices
    let mut priority = Vec::with_capacity(devices.capacity());
    for boot_disk in boot_disks {
        let mut i = 0;
        while i < devices.len() {
            // Only prioritize non-ESP devices
            if devices[i].handle != esp_handle {
                match device_path_relation(devices[i].device_path.0, boot_disk.device_path.0) {
                    DevicePathRelation::Child(0) => {
                        priority.push(devices.remove(i));
                        continue;
                    }
                    _ => (),
                }
            }

            i += 1;
        }

        priority.push(boot_disk);
    }

    // Add any remaining devices
    priority.extend(devices);

    priority
}

#[repr(C, packed)]
#[allow(dead_code)]
struct DevicePathHarddrive {
    partition_number: u32,
    partition_start: u64,
    partition_size: u64,
    partition_signature: [u8; 16],
    partition_format: u8,
    signature_type: u8,
}

pub fn device_path_to_string(device_path: &DevicePath) -> String {
    let mut s = String::new();
    for (node, node_data) in DevicePathIter::new(device_path) {
        let read_u16 = |i: usize| -> u16 { (node_data[i] as u16) | (node_data[i + 1] as u16) << 8 };

        let read_u32 = |i: usize| -> u32 {
            (node_data[i] as u32)
                | (node_data[i + 1] as u32) << 8
                | (node_data[i + 2] as u32) << 16
                | (node_data[i + 3] as u32) << 24
        };

        if !s.is_empty() {
            s.push('/');
        }

        let _ = match DevicePathType::try_from(node.Type) {
            Ok(path_type) => match path_type {
                DevicePathType::Hardware => match DevicePathHardwareType::try_from(node.SubType) {
                    Ok(sub_type) => match sub_type {
                        DevicePathHardwareType::Pci if node_data.len() == 2 => {
                            let func = node_data[0];
                            let dev = node_data[1];
                            write!(s, "Pci(0x{:X},0x{:X})", dev, func)
                        }
                        _ => write!(s, "{:?} {:?} {:X?}", path_type, sub_type, node_data),
                    },
                    Err(()) => write!(s, "{:?} 0x{:02X} {:X?}", path_type, node.SubType, node_data),
                },
                DevicePathType::Acpi => match DevicePathAcpiType::try_from(node.SubType) {
                    Ok(sub_type) => match sub_type {
                        DevicePathAcpiType::Acpi if node_data.len() == 8 => {
                            let hid = read_u32(0);
                            let uid = read_u32(4);
                            if hid & 0xFFFF == 0x41D0 {
                                write!(s, "Acpi(PNP{:04X},0x{:X})", hid >> 16, uid)
                            } else {
                                write!(s, "Acpi(0x{:08X},0x{:X})", hid, uid)
                            }
                        }
                        _ => write!(s, "{:?} {:?} {:X?}", path_type, sub_type, node_data),
                    },
                    Err(()) => write!(s, "{:?} 0x{:02X} {:X?}", path_type, node.SubType, node_data),
                },
                DevicePathType::Messaging => {
                    match DevicePathMessagingType::try_from(node.SubType) {
                        Ok(sub_type) => match sub_type {
                            DevicePathMessagingType::Sata if node_data.len() == 6 => {
                                let hba_port = read_u16(0);
                                let multiplier_port = read_u16(2);
                                let logical_unit = read_u16(4);
                                if multiplier_port & (1 << 15) != 0 {
                                    write!(s, "Sata(0x{:X},0x{:X})", hba_port, logical_unit)
                                } else {
                                    write!(
                                        s,
                                        "Sata(0x{:X},0x{:X},0x{:X})",
                                        hba_port, multiplier_port, logical_unit
                                    )
                                }
                            }
                            DevicePathMessagingType::Usb if node_data.len() == 2 => {
                                let port = node_data[0];
                                let iface = node_data[1];
                                write!(s, "Usb(0x{:X},0x{:X})", port, iface)
                            }
                            DevicePathMessagingType::Nvme if node_data.len() == 12 => {
                                let nsid = read_u32(0);
                                let eui = &node_data[4..];
                                if eui == &[0, 0, 0, 0, 0, 0, 0, 0] {
                                    write!(s, "NVMe(0x{:X})", nsid)
                                } else {
                                    write!(
                                    s,
                                    "NVMe(0x{:X},{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X})",
                                    nsid,
                                    eui[0],
                                    eui[1],
                                    eui[2],
                                    eui[3],
                                    eui[4],
                                    eui[5],
                                    eui[6],
                                    eui[7],
                                )
                                }
                            }
                            _ => write!(s, "{:?} {:?} {:X?}", path_type, sub_type, node_data),
                        },
                        Err(()) => {
                            write!(s, "{:?} 0x{:02X} {:X?}", path_type, node.SubType, node_data)
                        }
                    }
                }
                DevicePathType::Media => match DevicePathMediaType::try_from(node.SubType) {
                    Ok(sub_type) => {
                        match sub_type {
                            DevicePathMediaType::Harddrive
                                if node_data.len() == mem::size_of::<DevicePathHarddrive>() =>
                            {
                                let harddrive = unsafe {
                                    ptr::read(node_data.as_ptr() as *const DevicePathHarddrive)
                                };
                                let partition_number = unsafe {
                                    ptr::read_unaligned(ptr::addr_of!(harddrive.partition_number))
                                };
                                match harddrive.signature_type {
                                    1 => {
                                        let id = unsafe {
                                            ptr::read(harddrive.partition_signature.as_ptr()
                                                as *const u32)
                                        };
                                        write!(s, "HD(0x{:X},MBR,0x{:X})", partition_number, id)
                                    }
                                    2 => {
                                        let guid = unsafe {
                                            ptr::read(harddrive.partition_signature.as_ptr()
                                                as *const Guid)
                                        };
                                        write!(
                                        s,
                                        "HD(0x{:X},GPT,{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X})",
                                        partition_number,
                                        guid.0,
                                        guid.1,
                                        guid.2,
                                        guid.3[0],
                                        guid.3[1],
                                        guid.3[2],
                                        guid.3[3],
                                        guid.3[4],
                                        guid.3[5],
                                        guid.3[6],
                                        guid.3[7],
                                    )
                                    }
                                    _ => {
                                        write!(
                                            s,
                                            "HD(0x{:X},0x{:X},{:X?})",
                                            partition_number,
                                            harddrive.signature_type,
                                            harddrive.partition_signature
                                        )
                                    }
                                }
                            }
                            DevicePathMediaType::Filepath => {
                                for chunk in node_data.chunks_exact(2) {
                                    let data = (chunk[0] as u16) | (chunk[1] as u16) << 8;
                                    match unsafe { char::from_u32_unchecked(data as u32) } {
                                        '\\' => s.push('/'),
                                        c => s.push(c),
                                    }
                                }
                                Ok(())
                            }
                            _ => write!(s, "{:?} {:?} {:X?}", path_type, sub_type, node_data),
                        }
                    }
                    Err(()) => write!(s, "{:?} 0x{:02X} {:X?}", path_type, node.SubType, node_data),
                },
                DevicePathType::Bbs => match DevicePathBbsType::try_from(node.SubType) {
                    Ok(sub_type) => match sub_type {
                        _ => write!(s, "{:?} {:?} {:X?}", path_type, sub_type, node_data),
                    },
                    Err(()) => write!(s, "{:?} 0x{:02X} {:X?}", path_type, node.SubType, node_data),
                },
                DevicePathType::End => match DevicePathEndType::try_from(node.SubType) {
                    Ok(sub_type) => match sub_type {
                        _ => write!(s, "{:?} {:?} {:X?}", path_type, sub_type, node_data),
                    },
                    Err(()) => write!(s, "{:?} 0x{:02X} {:X?}", path_type, node.SubType, node_data),
                },
            },
            Err(()) => {
                write!(
                    s,
                    "0x{:02X} 0x{:02X} {:X?}",
                    node.Type, node.SubType, node_data
                )
            }
        };
    }
    s
}

pub struct DevicePathProtocol(pub &'static mut DevicePath);

impl Protocol<DevicePath> for DevicePathProtocol {
    fn guid() -> Guid {
        uefi::guid::DEVICE_PATH_GUID
    }

    fn new(inner: &'static mut DevicePath) -> Self {
        Self(inner)
    }
}

pub struct LoadedImageDevicePathProtocol(pub &'static mut DevicePath);

impl Protocol<DevicePath> for LoadedImageDevicePathProtocol {
    fn guid() -> Guid {
        uefi::guid::LOADED_IMAGE_DEVICE_PATH_GUID
    }

    fn new(inner: &'static mut DevicePath) -> Self {
        Self(inner)
    }
}

pub struct DevicePathIter<'a> {
    device_path: &'a DevicePath,
    node_ptr: *const DevicePath,
}

impl<'a> DevicePathIter<'a> {
    pub fn new(device_path: &'a DevicePath) -> Self {
        Self {
            device_path,
            node_ptr: device_path as *const DevicePath,
        }
    }
}

impl<'a> Iterator for DevicePathIter<'a> {
    type Item = (&'a DevicePath, &'a [u8]);
    fn next(&mut self) -> Option<Self::Item> {
        let node = unsafe { &*self.node_ptr };

        if node.Type == DevicePathType::End as u8 {
            return None;
        }

        let node_data = unsafe {
            slice::from_raw_parts(
                self.node_ptr.add(1) as *mut u8,
                node.Length.saturating_sub(4) as usize,
            )
        };

        self.node_ptr = (self.node_ptr as usize + node.Length as usize) as *const DevicePath;

        Some((node, node_data))
    }
}
