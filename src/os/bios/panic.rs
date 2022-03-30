//! Intrinsics for panic handling

use core::alloc::Layout;
use core::arch::asm;
use core::panic::PanicInfo;

#[lang = "eh_personality"]
#[no_mangle]
pub extern "C" fn rust_eh_personality() {}

/// Required to handle panics
#[panic_handler]
#[no_mangle]
pub extern "C" fn rust_begin_unwind(info: &PanicInfo) -> ! {
    unsafe {
        println!("BOOTLOADER PANIC:\n{}", info);
        loop {
            asm!("hlt");
        }
    }
}

#[lang = "oom"]
#[no_mangle]
#[allow(improper_ctypes_definitions)] // Layout is not repr(C)
pub extern fn rust_oom(_layout: Layout) -> ! {
    panic!("memory allocation failed");
}

#[allow(non_snake_case)]
#[no_mangle]
/// Required to handle panics
pub extern "C" fn _Unwind_Resume() -> ! {
    loop {
        unsafe {
            asm!("hlt");
        }
    }
}
