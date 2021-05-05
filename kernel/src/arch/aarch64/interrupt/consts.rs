#![allow(non_upper_case_globals)]
use super::syndrome::{Fault, Syndrome};
use aarch64::registers::*;

pub fn is_page_fault(trap: usize) -> bool {
    // 2: from lower el, sync error
    if trap != 0x2 {
        return false;
    }

    // determine by esr
    let esr = ESR_EL1.get() as u32;
    let syndrome = Syndrome::from(esr);
    match syndrome {
        Syndrome::DataAbort { kind, level: _ } | Syndrome::InstructionAbort { kind, level: _ } => {
            matches!(
                kind,
                Fault::Translation | Fault::AccessFlag | Fault::Permission
            )
        }
        _ => false,
    }
}

// from el0, irq
pub const IrqMax: usize = 0x10002;
pub const IrqMin: usize = 0x10002;

// from el0, sync
pub const Syscall: usize = 0x00002;

#[inline]
pub fn is_syscall(trap: usize) -> bool {
    trap == Syscall
}

#[inline]
pub fn is_irq(trap: usize) -> bool {
    IrqMin <= trap && trap <= IrqMax
}
