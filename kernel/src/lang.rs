#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    if let Some(args) = info.message() {
        println!("\nKernel panic: {}", args);
    } else {
        println!("\nKernel panic!");
    }
    crate::cpu::wait_forever();
}

#[lang = "oom"]
fn oom(_: core::alloc::Layout) -> ! {
    panic!("out of memory");
}
