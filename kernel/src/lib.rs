#![no_std]
#![feature(lang_items)]
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
extern crate num_derive;
#[macro_use]
extern crate log;

#[macro_use]
mod logging;
#[macro_use]
mod lang;

#[path = "arch/aarch64/mod.rs"]
pub mod arch;
mod backtrace;
pub mod consts;
pub mod drivers;
pub mod fs;
pub mod memory;
pub mod process;
pub mod sync;
pub mod task;
pub mod syscall;
pub mod signal;

pub use arch::cpu;

pub fn kmain() -> ! {
    loop {
        cpu::halt();
    }
}
