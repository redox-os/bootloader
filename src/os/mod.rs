#[cfg(all(target_arch = "x86", target_os = "none"))]
pub use self::bios::*;

#[cfg(all(target_arch = "x86", target_os = "none"))]
mod bios;

#[cfg(target_os = "uefi")]
pub use self::uefi::*;

#[cfg(target_os = "uefi")]
mod uefi;
