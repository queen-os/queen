//! Trap handler

use core::convert::TryFrom;

use super::{
    syndrome::{Fault, Syndrome},
    IRQ_MANAGER,
};
use crate::drivers::irq::IrqManager;
use aarch64::{registers::*, trap::TrapFrame};
use num_enum::TryFromPrimitive;

#[derive(Debug, PartialEq, Eq, Copy, Clone, TryFromPrimitive)]
#[repr(u8)]
pub enum Kind {
    Synchronous = 0,
    Irq = 1,
    Fiq = 2,
    SError = 3,
}

impl Kind {
    #[inline]
    fn from(x: u8) -> Kind {
        Kind::try_from(x).expect("Bad kind")
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, TryFromPrimitive)]
#[repr(u8)]
pub enum Source {
    CurrentSpEl0 = 0,
    CurrentSpElx = 1,
    LowerAArch64 = 2,
    LowerAArch32 = 3,
}

impl Source {
    #[inline]
    fn from(x: u8) -> Source {
        Source::try_from(x).expect("Bad source")
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct Info {
    source: Source,
    kind: Kind,
}

/// This function is called when an exception occurs. The `info` parameter
/// specifies the source and kind of exception that has occurred. The `esr` is
/// the value of the exception syndrome register. Finally, `tf` is a pointer to
/// the trap frame for the exception.
#[no_mangle]
pub extern "C" fn trap_handler(tf: &mut TrapFrame) {
    let info: Info = Info {
        source: Source::from((tf.trap_num & 0xFFFF) as u8),
        kind: Kind::from((tf.trap_num >> 16) as u8),
    };
    let esr = ESR_EL1.get() as u32;
    trace!(
        "Exception @ CPU{}: {:?}, ESR: {:#x}, ELR: {:#x?}",
        crate::arch::cpu::id(),
        info,
        esr,
        tf.elr
    );
    match info.kind {
        Kind::Synchronous => {
            let syndrome = Syndrome::from(esr);
            trace!("ESR: {:#x?}, Syndrome: {:?}", esr, syndrome);
            // syndrome is only valid with sync
            match syndrome {
                Syndrome::DataAbort { kind, level: _ }
                | Syndrome::InstructionAbort { kind, level: _ } => match kind {
                    Fault::Translation | Fault::AccessFlag | Fault::Permission => {
                        let addr = FAR_EL1.get() as usize;
                        if !crate::memory::handle_page_fault(addr) {
                            panic!("\nEXCEPTION: Page Fault @ {:#x}", addr);
                        }
                    }
                    _ => panic!(),
                },
                _ => panic!(),
            }
        }
        Kind::Irq => {
            IRQ_MANAGER.wait().handle_pending_irqs();
        }
        _ => panic!(),
    }
    trace!("Exception end");
}
