use core::{mem::size_of, ptr::NonNull};

use crate::consts::{KERNEL_HEAP_SIZE, KERNEL_OFFSET, MEMORY_OFFSET, PHYSICAL_MEMORY_OFFSET};
use spin::Lazy;

pub mod handler;
mod memory_set;
mod paging;

pub use crate::arch::paging::*;
pub use handler::MemoryHandler;
pub use memory_set::{MemoryArea, MemoryAttr};
pub use paging::{Entry, Page, PageRange, PageTable, PageTableExt};

pub enum VmError {
    InvalidPtr,
}
pub type VmResult<T> = Result<T, VmError>;

pub type PhysAddr = usize;
pub type VirtAddr = usize;

pub type MemorySet = memory_set::MemorySet<PageTableImpl>;

pub type FrameAlloc = allocators::frame::buddy_system::LockedFrameAlloc;
pub static FRAME_ALLOCATOR: Lazy<FrameAlloc> = Lazy::new(FrameAlloc::new);

pub type HeapAlloc = allocators::heap::explicit_free_list::LockedHeapAlloc;
#[global_allocator]
pub static HEAP_ALLOCATOR: HeapAlloc = HeapAlloc::new();

pub const PAGE_SIZE: usize = 1 << 12;

/// Convert physical address to virtual address
#[inline]
pub const fn phys_to_virt(addr: PhysAddr) -> VirtAddr {
    addr + PHYSICAL_MEMORY_OFFSET
}

/// Convert virtual address to physical address
#[inline]
pub const fn virt_to_phys(addr: VirtAddr) -> PhysAddr {
    addr - PHYSICAL_MEMORY_OFFSET
}

/// Convert virtual address to the offset of kernel
#[inline]
pub const fn as_lower_range(addr: VirtAddr) -> VirtAddr {
    addr & !KERNEL_OFFSET
}

#[inline]
pub const fn as_upper_range(addr: VirtAddr) -> VirtAddr {
    addr | KERNEL_OFFSET
}

pub fn alloc_frames(count: usize) -> Option<PhysAddr> {
    // get the real address of the alloc frame
    FRAME_ALLOCATOR.lock().alloc(count).map(|id| {
        let frame = id * PAGE_SIZE + MEMORY_OFFSET;
        trace!("Allocate frame: {:x?}", frame);
        frame
    })
}

pub fn dealloc_frames(target: PhysAddr, count: usize) {
    trace!("Deallocate frame: {:x}", target);
    FRAME_ALLOCATOR
        .lock()
        .dealloc((target / PAGE_SIZE) as usize, count);
}

pub fn init_heap() {
    const LEN: usize = KERNEL_HEAP_SIZE / size_of::<usize>();
    static mut HEAP: [usize; LEN] = [0; LEN];
    unsafe {
        HEAP_ALLOCATOR.lock().init(
            NonNull::new_unchecked(HEAP.as_mut_ptr().cast()),
            KERNEL_HEAP_SIZE,
        );
    }
}

/// Handle page fault at `addr`.
/// Return true to continue, false to halt.
pub fn handle_page_fault(addr: usize) -> bool {
    debug!("page fault from kernel @ {:#x}", addr);
    // TODO
    // let thread = current_thread().unwrap();
    // let mut lock = thread.vm.lock();
    // lock.handle_page_fault(addr)
    false
}
