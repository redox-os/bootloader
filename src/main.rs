#![no_std]
#![feature(asm)]
#![feature(lang_items)]
#![feature(llvm_asm)]
#![cfg_attr(
    target_os = "uefi",
    no_main,
    feature(control_flow_enum),
    feature(try_trait_v2),
)]

#[cfg_attr(target_os = "none", macro_use)]
extern crate alloc;

#[cfg(target_os = "uefi")]
#[macro_use]
extern crate uefi_std as std;

#[macro_use]
mod os;

mod arch;
mod logger;
