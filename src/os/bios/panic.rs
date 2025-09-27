//! Intrinsics for panic handling

use core::alloc::Layout;
use core::arch::asm;
use core::panic::PanicInfo;

/// Required to handle panics
#[panic_handler]
pub fn rust_begin_unwind(info: &PanicInfo) -> ! {
    unsafe {
        println!("BOOTLOADER PANIC:\n{}", info);
        loop {
            asm!("hlt");
        }
    }
}

#[alloc_error_handler]
#[allow(improper_ctypes_definitions)] // Layout is not repr(C)
pub extern "C" fn rust_oom(_layout: Layout) -> ! {
    panic!("memory allocation failed");
}
