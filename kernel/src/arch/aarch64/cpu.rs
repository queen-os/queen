use aarch64::asm;
pub use aarch64::asm::nop;

pub fn halt() {
    asm::wfi();
}

pub fn wait_forever() -> ! {
    loop {
        asm::wfe();
    }
}

/// start other cpu
pub fn start_others() {
    unsafe {
        for cpu in 0..super::bsp::CPU_NUM {
            asm!("ldr x0, =0xc4000003");
            asm!("mov x1, {}", in(reg) cpu); // target CPU's MPIDR affinity
            asm!("ldr x2, =others_startup"); // entry point
            asm!("ldr x3, =0"); // context ID: put into target CPU's x0
            asm!("hvc 0");
        }
    }
}

pub fn id() -> usize {
    asm::cpuid()
}
