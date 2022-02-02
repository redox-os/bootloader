use core::ptr;

use crate::THUNK_STACK_ADDR;

#[derive(Clone, Copy)]
#[repr(packed)]
pub struct ThunkData {
    pub di: u16,
    pub si: u16,
    pub bp: u16,
    sp: u16,
    pub bx: u16,
    pub dx: u16,
    pub cx: u16,
    pub ax: u16,
}

impl ThunkData {
    pub fn new() -> Self {
        Self {
            di: 0,
            si: 0,
            bp: 0,
            sp: THUNK_STACK_ADDR as u16,
            bx: 0,
            dx: 0,
            cx: 0,
            ax: 0,
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
