use redoxfs::Disk;

#[cfg(all(target_arch = "x86", target_os = "none"))]
pub use self::bios::*;

#[cfg(all(target_arch = "x86", target_os = "none"))]
#[macro_use]
mod bios;

#[cfg(target_os = "uefi")]
pub use self::uefi::*;

#[cfg(target_os = "uefi")]
#[macro_use]
mod uefi;

#[derive(Clone, Copy, Debug)]
pub enum OsKey {
    Left,
    Right,
    Up,
    Down,
    Enter,
    Other,
}

#[derive(Clone, Copy, Debug)]
pub enum OsMemoryKind {
    Free,
    Reclaim,
    Reserved,
}

#[derive(Clone, Copy, Debug)]
pub struct OsMemoryEntry {
    pub base: u64,
    pub size: u64,
    pub kind: OsMemoryKind,
}

#[derive(Clone, Copy, Debug)]
pub struct OsVideoMode {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub base: u64,
}

pub trait Os<
    D: Disk,
    M: Iterator<Item=OsMemoryEntry>,
    V: Iterator<Item=OsVideoMode>
> {
    fn disk(&self) -> D;

    fn memory(&self) -> M;

    fn video_modes(&self) -> V;
    fn set_video_mode(&self, id: u32);

    fn get_key(&self) -> OsKey;

    fn get_text_position(&self) -> (usize, usize);
    fn set_text_position(&self, x: usize, y: usize);
    fn set_text_highlight(&self, highlight: bool);
}
