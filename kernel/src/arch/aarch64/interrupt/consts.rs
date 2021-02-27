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
