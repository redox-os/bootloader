#![no_std]
#![feature(lang_items)]
#![feature(llvm_asm)]

mod panic;

#[no_mangle]
pub unsafe extern "C" fn kstart() -> ! {
    loop {}
}
