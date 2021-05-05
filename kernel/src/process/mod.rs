use self::thread::{Tid, THREADS};
use crate::{
    consts::MAX_CPU_NUM,
    fs::FileHandle,
    memory::MemorySet,
    signal::{Siginfo, Signal, SignalAction, Sigset},
    sync::{Event, EventBus, MutexNoIrq},
};
use alloc::{
    collections::{BTreeMap, VecDeque},
    string::String,
    sync::{Arc, Weak},
    vec::Vec,
};
use spin::RwLock;

pub mod abi;
pub mod structs;
pub mod thread;

pub use thread::Thread;

/// Process ID type
pub type Pid = usize;
/// process group id type
pub type Pgid = i32;
pub type ProcessRef = Arc<MutexNoIrq<Process>>;
pub const PID_INIT: usize = 1;
pub static PROCESSES: RwLock<BTreeMap<Pid, ProcessRef>> = RwLock::new(BTreeMap::new());
static mut PROCESSORS: [Option<Arc<Thread>>; MAX_CPU_NUM] = [None; MAX_CPU_NUM];

/// Get current thread
///
/// `Thread` is a thread-local object.
/// It is safe to call this once, and pass `&mut Thread` as a function argument.
///
/// Don't use it unless necessary.
#[inline]
pub fn current_thread() -> Option<Arc<Thread>> {
    let cpu_id = crate::cpu::id();
    unsafe { PROCESSORS[cpu_id].clone() }
}

pub struct Process {
    /// Virtual memory
    pub vm: Arc<MutexNoIrq<MemorySet>>,

    /// Opened files
    pub files: BTreeMap<usize, FileHandle>,

    /// Current working directory
    pub cwd: String,

    /// Executable path
    pub exec_path: String,

    // /// Futex
    // pub futexes: BTreeMap<usize, Arc<Futex>>,

    // /// Semaphore
    // pub semaphores: SemProc,
    /// Pid i.e. tgid, usually the tid of first thread
    pub pid: Pid,

    //// Process group id
    pub pgid: Pgid,

    /// Parent process
    /// Avoid deadlock, put pid out
    pub parent: (Pid, Weak<MutexNoIrq<Process>>),

    /// Children process
    pub children: Vec<(Pid, Weak<MutexNoIrq<Process>>)>,

    /// Threads
    /// threads in the same process
    pub threads: Vec<Tid>,

    /// Events like exiting
    pub event_bus: Arc<MutexNoIrq<EventBus>>,

    /// Exit code
    pub exit_code: usize,

    // delivered signals, tid specified thread, -1 stands for any thread
    pub sig_queue: VecDeque<(Siginfo, isize)>,
    pub pending_sigset: Sigset,

    /// signal actions
    pub dispositions: [SignalAction; Signal::RTMAX + 1],
    // /// shared memory
    // pub shm_identifiers: ShmProc,
}

/// Return the process which thread tid is in
pub fn process_of(tid: usize) -> Option<ProcessRef> {
    PROCESSES
        .read()
        .iter()
        .map(|(_, proc)| proc.clone())
        .find(|proc| proc.lock().threads.contains(&tid))
}

/// Get process by pid
pub fn process(pid: usize) -> Option<ProcessRef> {
    PROCESSES.read().get(&pid).cloned()
}

/// Get process group by pgid
pub fn process_group(pgid: Pgid) -> Vec<ProcessRef> {
    PROCESSES
        .read()
        .iter()
        .map(|(_, proc)| proc.clone())
        .filter(|proc| proc.lock().pgid == pgid)
        .collect::<Vec<_>>()
}

/// Set pid and put itself to global process table.
pub fn add_to_process_table(process: ProcessRef, pid: Pid) {
    let mut process_table = PROCESSES.write();

    // set pid
    process.lock().pid = pid;

    // put to process table
    process_table.insert(pid, process.clone());
}

impl Process {
    /// Get lowest free fd
    fn get_free_fd(&self) -> usize {
        (0..).find(|i| !self.files.contains_key(i)).unwrap()
    }

    /// get the lowest available fd great than or equal to arg
    pub fn get_free_fd_from(&self, arg: usize) -> usize {
        (arg..).find(|i| !self.files.contains_key(i)).unwrap()
    }

    /// Add a file to the process, return its fd.
    pub fn add_file(&mut self, file: FileHandle) -> usize {
        let fd = self.get_free_fd();
        self.files.insert(fd, file);
        fd
    }

    // /// Get futex by addr
    // pub fn get_futex(&mut self, uaddr: usize) -> Arc<Futex> {
    //     if !self.futexes.contains_key(&uaddr) {
    //         self.futexes.insert(uaddr, Arc::new(Futex::new()));
    //     }
    //     self.futexes.get(&uaddr).unwrap().clone()
    // }

    /// Exit the process.
    /// Kill all threads and notify parent with the exit code.
    pub fn exit(&mut self, exit_code: usize) {
        // avoid some strange dead lock
        // self.files.clear(); this does not work sometime, for unknown reason
        // manually drop
        let fds = self.files.iter().map(|(fd, _)| *fd).collect::<Vec<_>>();
        for fd in fds.iter() {
            let file = self.files.remove(fd).unwrap();
            drop(file);
        }

        // notify parent and fill exit code
        self.event_bus.lock().set(Event::PROCESS_QUIT);
        if let Some(parent) = self.parent.1.upgrade() {
            parent
                .lock()
                .event_bus
                .lock()
                .set(Event::CHILD_PROCESS_QUIT);
        }
        self.exit_code = exit_code;

        // quit all threads
        // this must be after setting the value of subprocess, or the threads will be treated exit before actually exits
        // remove from thread table
        let mut thread_table = THREADS.write();
        for tid in self.threads.iter() {
            thread_table.remove(tid);
        }
        self.threads.clear();

        info!("process {} exit with {}", self.pid, exit_code);
    }

    pub fn exited(&self) -> bool {
        self.threads.is_empty()
    }
}
