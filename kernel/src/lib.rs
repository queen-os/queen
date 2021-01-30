#![feature(lang_items)]
#![feature(naked_functions)]
#![feature(panic_info_message)]
#![feature(global_asm)]
#![feature(asm)]
#![feature(format_args_nl)]
#![no_std]

#[macro_use]
extern crate alloc;
#[macro_use]
extern crate log;

#[macro_use]
mod logging;
#[path = "arch/aarch64/mod.rs"]
pub mod arch;
pub mod consts;
pub mod memory;
mod lang;
pub use arch::cpu;

pub fn kmain() -> ! {
    loop {
        cpu::halt();
    }
}
