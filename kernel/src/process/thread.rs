use super::{abi, add_to_process_table, structs::ElfExt, Pid, Process, PID_INIT};
use crate::{
    fs::{FileHandle, OpenOptions, FOLLOW_MAX_DEPTH, ROOT_INODE},
    memory::{
        handler::{ByFrame, Delay},
        GlobalFrameAlloc, MemoryAttr, MemorySet, PAGE_SIZE,
    },
    process::abi::ProcInitInfo,
    signal::{Signal, SignalAction, SignalStack, Sigset},
    sync::{
        spin::{MutexNoIrq, RwLock},
        EventBus,
    },
};
use aarch64::trap::UserContext;
use alloc::{
    boxed::Box,
    collections::{BTreeMap, VecDeque},
    string::String,
    sync::{Arc, Weak},
    vec::Vec,
};
use num_traits::FromPrimitive;
use core::{
    future::Future,
    mem::MaybeUninit,
    pin::Pin,
    task::{Context, Poll},
};
use queen_fs::INode;
use xmas_elf::{
    header,
    program::{Flags, SegmentData, Type},
    ElfFile,
};

pub type Tid = usize;
pub type ThreadRef = Arc<Thread>;
pub static THREADS: RwLock<BTreeMap<Tid, ThreadRef>> = RwLock::new(BTreeMap::new());

/// Mutable part of a thread struct
#[derive(Default)]
struct ThreadInner {
    context: Option<UserContext>,
    /// Kernel performs futex wake when thread exits.
    /// Ref: [http://man7.org/linux/man-pages/man2/set_tid_address.2.html]
    pub clear_child_tid: usize,
    /// Signal mask
    pub sig_mask: Sigset,
    /// signal alternate stack
    pub signal_alternate_stack: SignalStack,
}

pub struct Thread {
    pub inner: MutexNoIrq<ThreadInner>,
    pub process: Arc<MutexNoIrq<Process>>,
    pub vm: Arc<MutexNoIrq<MemorySet>>,
    pub tid: Tid,
}

impl Thread {
    /// Assign a tid and put itself to global thread table.
    pub fn add_to_table(mut self) -> Arc<Self> {
        let mut thread_table = THREADS.write();

        // assign tid, do not start from 0
        let tid = (PID_INIT..)
            .find(|i| thread_table.get(i).is_none())
            .unwrap();
        self.tid = tid;

        // put to thread table
        let self_ref = Arc::new(self);
        thread_table.insert(tid, self_ref.clone());

        self_ref
    }

    /// Construct virtual memory of a new user process from ELF at `inode`.
    /// Return `(MemorySet, entry_point, ustack_top)`
    pub fn new_user_vm(
        inode: &Arc<dyn INode>,
        args: Vec<String>,
        envs: Vec<String>,
        vm: &mut MemorySet,
    ) -> Result<(usize, usize), &'static str> {
        // Read ELF header
        // 0x3c0: magic number from ld-musl.so
        let mut data = [0u8; 0x3c0];
        inode
            .read_at(0, &mut data)
            .map_err(|_| "failed to read from INode")?;

        // Parse ELF
        let elf = ElfFile::new(&data)?;

        // Check ELF type
        match elf.header.pt2.type_().as_type() {
            header::Type::Executable => {}
            header::Type::SharedObject => {}
            _ => return Err("ELF is not executable or shared object"),
        }

        // Check ELF arch
        match elf.header.pt2.machine().as_machine() {
            header::Machine::AArch64 => {}
            _ => return Err("invalid ELF arch"),
        }

        // auxiliary vector
        let mut auxv = {
            let mut map = BTreeMap::new();
            if let Some(phdr_vaddr) = elf.get_phdr_vaddr() {
                map.insert(abi::AT_PHDR, phdr_vaddr as usize);
            }
            map.insert(abi::AT_PHENT, elf.header.pt2.ph_entry_size() as usize);
            map.insert(abi::AT_PHNUM, elf.header.pt2.ph_count() as usize);
            map.insert(abi::AT_PAGESZ, PAGE_SIZE);
            map
        };

        // entry point
        let mut entry_addr = elf.header.pt2.entry_point() as usize;
        // Make page table
        vm.clear();
        let bias = elf.make_memory_set(vm, inode);

        // Check interpreter (for dynamic link)
        // When interpreter is used, map both dynamic linker and executable
        if let Ok(loader_path) = elf.get_interpreter() {
            info!("Handling interpreter... offset={:x}", bias);
            // assuming absolute path
            let interp_inode = ROOT_INODE
                .lookup_follow(loader_path, FOLLOW_MAX_DEPTH)
                .map_err(|_| "interpreter not found")?;
            // load loader by bias and set aux vector.
            let mut interp_data: [u8; 0x3c0] = unsafe { MaybeUninit::zeroed().assume_init() };
            interp_inode
                .read_at(0, &mut interp_data)
                .map_err(|_| "failed to read from INode")?;
            let elf_interp = ElfFile::new(&interp_data)?;
            elf_interp.append_as_interpreter(&interp_inode, vm, bias);

            // update auxiliary vector
            auxv.insert(abi::AT_ENTRY, elf.header.pt2.entry_point() as usize);
            auxv.insert(abi::AT_BASE, bias);

            // use interpreter as actual entry point
            debug!("entry point: {:x}", elf.header.pt2.entry_point() as usize);
            entry_addr = elf_interp.header.pt2.entry_point() as usize + bias;
        }

        // User stack
        use crate::consts::{USER_STACK_OFFSET, USER_STACK_SIZE};
        let mut ustack_top = {
            let ustack_buttom = USER_STACK_OFFSET;
            let ustack_top = USER_STACK_OFFSET + USER_STACK_SIZE;

            // user stack except top 4 pages
            vm.push(
                ustack_buttom,
                ustack_top - PAGE_SIZE * 4,
                MemoryAttr::default().user().execute(),
                Delay::new(GlobalFrameAlloc),
                "user_stack_delay",
            );

            // We are going to write init info now. So map the last 4 pages eagerly.
            vm.push(
                ustack_top - PAGE_SIZE * 4,
                ustack_top,
                MemoryAttr::default().user().execute(), // feature
                ByFrame::new(GlobalFrameAlloc),
                "user_stack",
            );
            ustack_top
        };

        // Make init info
        let init_info = ProcInitInfo { args, envs, auxv };
        unsafe {
            vm.with(|| ustack_top = init_info.push_at(ustack_top));
        }

        Ok((entry_addr, ustack_top))
    }

    /// Make a new user process from ELF `data`
    pub fn new_user(
        inode: &Arc<dyn INode>,
        exec_path: &str,
        args: Vec<String>,
        envs: Vec<String>,
    ) -> ThreadRef {
        // get virtual memory info
        let mut vm = MemorySet::new();
        let (entry_addr, ustack_top) = Self::new_user_vm(inode, args, envs, &mut vm).unwrap();

        let vm_token = vm.token();
        let vm = Arc::new(MutexNoIrq::new(vm));

        // initial fds
        let mut files = BTreeMap::new();
        files.insert(
            0,
            FileHandle::new(
                crate::fs::TTY.clone(),
                OpenOptions {
                    read: true,
                    write: false,
                    append: false,
                },
                String::from("/dev/tty"),
                false,
                false,
            ),
        );
        files.insert(
            1,
            FileHandle::new(
                crate::fs::TTY.clone(),
                OpenOptions {
                    read: false,
                    write: true,
                    append: false,
                },
                String::from("/dev/tty"),
                false,
                false,
            ),
        );
        files.insert(
            2,
            FileHandle::new(
                crate::fs::TTY.clone(),
                OpenOptions {
                    read: false,
                    write: true,
                    append: false,
                },
                String::from("/dev/tty"),
                false,
                false,
            ),
        );

        // user context
        let mut context = UserContext::default();
        context.set_ip(entry_addr);
        context.set_sp(ustack_top);

        // arch specific
        #[cfg(target_arch = "aarch64")]
        {
            // F | A | D | EL0
            context.spsr = 0b1101_00_0000;
        }

        let thread = Thread {
            inner: MutexNoIrq::new(ThreadInner {
                context: Some(context),
                clear_child_tid: 0,
                sig_mask: Sigset::default(),
                signal_alternate_stack: SignalStack::default(),
            }), // allocated below
            vm: vm.clone(),
            tid: 0,
            process: Arc::new(MutexNoIrq::new(Process {
                vm,
                files,
                cwd: String::from("/"),
                exec_path: String::from(exec_path),
                pid: 0, // allocated later
                pgid: 0,
                parent: (0, Weak::new()),
                children: Vec::new(),
                threads: Vec::new(),
                exit_code: 0,
                pending_sigset: Sigset::empty(),
                sig_queue: VecDeque::new(),
                dispositions: [SignalAction::default(); Signal::RTMAX + 1],
                event_bus: EventBus::new(),
            })),
        };

        let res = thread.add_to_table();

        // set pid to tid
        add_to_process_table(res.process.clone(), res.tid);

        res
    }

    /// Fork a new process from current one
    /// Only current process is persisted
    pub fn fork(&self, tf: &UserContext) -> ThreadRef {
        // clone virtual memory
        let vm = self.vm.lock().clone();
        let vm_token = vm.token();
        let vm = Arc::new(MutexNoIrq::new(vm));

        // context of new thread
        let mut context = tf.clone();
        context.set_syscall_ret(0);

        let mut process = self.process.lock();

        let new_process = Arc::new(MutexNoIrq::new(Process {
            vm: vm.clone(),
            files: process.files.clone(), // share open file descriptions
            cwd: process.cwd.clone(),
            exec_path: process.exec_path.clone(),
            pid: 0, // assigned later
            pgid: process.pgid,
            parent: (process.pid, Arc::downgrade(&self.process)),
            children: Vec::new(),
            threads: Vec::new(),
            exit_code: 0,
            pending_sigset: Sigset::empty(),
            sig_queue: VecDeque::new(),
            dispositions: process.dispositions.clone(),
            event_bus: EventBus::new(),
        }));

        // new thread
        // this part in linux manpage seems ambiguous:
        // Each of the threads in a process has its own signal mask.
        // A child created via fork(2) inherits a copy of its parent's signal
        // mask; the signal mask is preserved across execve(2).
        let sig_mask = self.inner.lock().sig_mask;
        let sigaltstack = self.inner.lock().signal_alternate_stack;
        let new_thread = Thread {
            tid: 0, // allocated below
            inner: MutexNoIrq::new(ThreadInner {
                context: Some(context),
                clear_child_tid: 0,
                sig_mask,
                signal_alternate_stack: sigaltstack,
            }),
            vm,
            process: new_process,
        }
        .add_to_table();

        // link thread and process
        let child_pid = new_thread.tid;
        add_to_process_table(new_thread.process.clone(), new_thread.tid);
        new_thread.process.lock().threads.push(new_thread.tid);

        // link to parent
        process
            .children
            .push((child_pid, Arc::downgrade(&new_thread.process)));

        new_thread
    }

    /// Create a new thread in the same process.
    pub fn new_clone(
        &self,
        context: &UserContext,
        stack_top: usize,
        tls: usize,
        clear_child_tid: usize,
    ) -> ThreadRef {
        let vm_token = self.vm.lock().token();
        let mut new_context = context.clone();
        new_context.set_syscall_ret(0);
        new_context.set_sp(stack_top);
        new_context.set_tls(tls);
        let thread_context = new_context;

        let sig_mask = self.inner.lock().sig_mask;
        let signal_stack = self.inner.lock().signal_alternate_stack;
        let thread = Thread {
            tid: 0,
            inner: MutexNoIrq::new(ThreadInner {
                clear_child_tid,
                context: Some(thread_context),
                sig_mask,
                signal_alternate_stack: signal_stack,
            }),
            vm: self.vm.clone(),
            process: self.process.clone(),
        };
        let res = thread.add_to_table();
        res.process.lock().threads.push(res.tid);
        res
    }

    pub fn begin_running(&self) -> UserContext {
        self.inner.lock().context.take().unwrap()
    }

    pub fn end_running(&self, ctx: UserContext) {
        self.inner.lock().context = Some(ctx);
    }

    /// this thread has signal to handle
    pub fn has_signal_to_handle(&self) -> bool {
        self.process
            .lock()
            .sig_queue
            .iter()
            .find(|(info, tid)| {
                let tid = *tid;
                // targets me and not masked
                (tid == -1 || tid as usize == self.tid)
                    && !self
                        .inner
                        .lock()
                        .sig_mask
                        .contains(FromPrimitive::from_i32(info.signo).unwrap())
            })
            .is_some()
    }
}
