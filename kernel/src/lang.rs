#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    if let Some(args) = info.message() {
        println!("\nKernel panic: {}", args);
    } else {
        println!("\nKernel panic!");
    }
    // crate::backtrace::backtrace();
    crate::cpu::wait_forever();
}

#[lang = "oom"]
fn oom(_: core::alloc::Layout) -> ! {
    panic!("out of memory");
}

/// Get the address of a symbol, using `adrp` instruction.
#[macro_export]
#[allow(unused_unsafe)]
macro_rules! symbol_addr {
    ($symbol: ident) => {
        {
            let x: usize;
            #[allow(unused_unsafe)]
            unsafe {
                asm!(concat!("adrp {}, ", stringify!($symbol)), out(reg) x);
            }
            x
        }
    };
}