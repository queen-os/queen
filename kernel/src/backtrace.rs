//! Provide backtrace upon panic
use core::{mem::size_of, arch::asm};

extern "C" {
    fn stext();
    fn etext();
}

/// Returns the current frame pointer or stack base pointer
#[inline]
pub fn fp() -> usize {
    let ptr: usize;
    unsafe {
        asm!("mov {}, x29", out(reg) ptr, options(pure, nomem, nostack));
    }
    ptr
}

/// Returns the current link register or return address
#[inline]
pub fn lr() -> usize {
    let ptr: usize;
    unsafe {
        asm!("mov {}, x30", out(reg) ptr, options(pure, nomem, nostack));
    }
    ptr
}

// Print the backtrace starting from the caller
#[no_mangle]
pub fn backtrace() {
    unsafe {
        let mut current_pc = lr();
        let mut current_fp = fp();
        let mut stack_num = 0;

        println!("=== QueenOS stack trace BEGIN ===");

        while current_pc >= stext as usize
            && current_pc <= etext as usize
            && current_fp as usize != 0
        {
            // print current backtrace
            println!(
                "#{:02} PC: {:#018X} FP: {:#018X}",
                stack_num,
                current_pc - size_of::<usize>(),
                current_fp
            );

            stack_num += 1;

            {
                current_fp = *(current_fp as *const usize);
                if current_fp < crate::arch::consts::KERNEL_OFFSET {
                    break;
                }
                if current_fp != 0 {
                    current_pc = *(current_fp as *const usize).offset(1);
                }
            }
        }
        println!("=== QueenOS stack trace END   ===");
    }
}
