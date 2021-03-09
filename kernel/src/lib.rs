#![no_std]
#![feature(lang_items)]
#![feature(naked_functions)]
#![feature(panic_info_message)]
#![feature(global_asm)]
#![feature(asm)]
#![feature(format_args_nl)]
#![feature(const_fn_fn_ptr_basics)]
#![feature(const_btree_new)]
#![feature(map_first_last)]

#[macro_use]
extern crate alloc;
#[macro_use]
extern crate log;

#[macro_use]
mod logging;
#[path = "arch/aarch64/mod.rs"]
pub mod arch;
mod backtrace;
pub mod consts;
pub mod drivers;
pub mod fs;
mod lang;
pub mod memory;
pub mod process;
pub mod sync;
pub mod task;

pub use arch::cpu;

pub fn kmain() -> ! {
    loop {
        cpu::halt();
    }
}
