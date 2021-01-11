#![feature(lang_items)]
#![feature(naked_functions)]
#![feature(panic_info_message)]
#![feature(global_asm)]
#![feature(format_args_nl)]
#![no_std]

#[path = "arch/aarch64/mod.rs"]
pub mod arch;
pub mod memory;
mod print;
pub use arch::cpu;

pub fn kmain() -> ! {
    println!("[0] Hello From Rust!");
    loop {
        cpu::halt();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    if let Some(args) = info.message() {
        println!("\nKernel panic: {}", args);
    } else {
        println!("\nKernel panic!");
    }
    cpu::wait_forever();
}
