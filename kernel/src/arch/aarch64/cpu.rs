use aarch64::asm;

pub fn halt() {
    asm::wfi();
}

pub fn wait_forever() -> ! {
    loop {
        asm::wfe();
    }
}

pub fn id() -> usize {
    asm::cpuid()
}