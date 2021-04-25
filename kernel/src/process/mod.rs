pub mod thread;
// pub mod structs;
pub mod abi;

/// Process ID type
pub type Pid = usize;
pub const PID_INIT: usize = 1;

pub struct Process {
    pub pid: Pid,
}
