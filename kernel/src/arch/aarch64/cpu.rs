use aarch64::asm;
use core::arch::asm;

pub use aarch64::asm::{halt, nop};

pub fn wait_forever() -> ! {
    loop {
        asm::wfe();
    }
}

/// start other cpu
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn start_other_cpu() {
    for cpu in 1..super::bsp::CPU_NUM {
        asm!("ldr x0, =0xc4000003");
        asm!("mov x1, {}", in(reg) cpu); // target CPU's MPIDR affinity
        asm!("adr x2, other_cpu_startup");
        asm!("ldr x3, =0"); // context ID: put into target CPU's x0
        asm!("hvc 0");
    }
}

pub fn id() -> usize {
    asm::cpuid()
}

/// Generates an ISB (instruction synchronization barrier) instruction or equivalent CP15 instruction.
/// # Safety
#[inline]
pub unsafe fn isb() {
    aarch64::barrier::isb(aarch64::barrier::SY);
}
