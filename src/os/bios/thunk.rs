use core::ptr;

use super::THUNK_STACK_ADDR;

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(packed)]
pub struct ThunkData {
    pub edi: u32,
    pub esi: u32,
    pub ebp: u32,
    esp: u32,
    pub ebx: u32,
    pub edx: u32,
    pub ecx: u32,
    pub eax: u32,
}

impl ThunkData {
    pub fn new() -> Self {
        Self {
            edi: 0,
            esi: 0,
            ebp: 0,
            esp: THUNK_STACK_ADDR as u32,
            ebx: 0,
            edx: 0,
            ecx: 0,
            eax: 0,
        }
    }

    pub unsafe fn save(&self) {
        ptr::write((THUNK_STACK_ADDR - 16) as *mut ThunkData, *self);
    }

    pub unsafe fn load(&mut self) {
        *self = ptr::read((THUNK_STACK_ADDR - 16) as *const ThunkData);
    }

    pub unsafe fn with(&mut self, f: extern "C" fn()) {
        self.save();
        f();
        self.load();
    }
}
