use core::ptr::NonNull;

use crate::consts::{KERNEL_HEAP_SIZE, KERNEL_OFFSET, MEMORY_OFFSET, PHYSICAL_MEMORY_OFFSET};
use aarch64::paging::PageSize;
use spin::Lazy;

pub mod handler;
mod memory_set;
mod paging;

pub use handler::MemoryHandler;
pub use memory_set::{MemoryArea, MemoryAttr, MemorySet};
pub use paging::{Entry, PageTable, PageTableExt};

pub enum VMError {
    InvalidPtr,
}
pub type VMResult<T> = Result<T, VMError>;

pub type PhysAddr = u64;
pub type VirtAddr = u64;
pub type Page = aarch64::paging::Page<aarch64::paging::Size4KiB>;
pub type Frame = aarch64::paging::Frame<aarch64::paging::Size4KiB>;

pub type FrameAlloc = allocators::frame::buddy_system::LockedFrameAlloc;
pub static FRAME_ALLOCATOR: Lazy<FrameAlloc> = Lazy::new(FrameAlloc::new);

pub type HeapAlloc = allocators::heap::explicit_free_list::LockedHeapAlloc;
#[global_allocator]
pub static HEAP_ALLOCATOR: HeapAlloc = HeapAlloc::new();

pub const PAGE_SIZE: u64 = aarch64::paging::Size4KiB::SIZE;

/// Convert physical address to virtual address
#[inline]
pub const fn phys_to_virt(addr: PhysAddr) -> VirtAddr {
    PHYSICAL_MEMORY_OFFSET as u64 + addr
}

/// Convert virtual address to physical address
#[inline]
pub const fn virt_to_phys(addr: VirtAddr) -> PhysAddr {
    addr - PHYSICAL_MEMORY_OFFSET
}

/// Convert virtual address to the offset of kernel
#[inline]
pub const fn kernel_offset(addr: VirtAddr) -> VirtAddr {
    addr - KERNEL_OFFSET
}

pub fn alloc_frames(count: usize) -> Option<PhysAddr> {
    // get the real address of the alloc frame
    FRAME_ALLOCATOR.lock().alloc(count).map(|id| {
        let frame = id as u64 * PAGE_SIZE + MEMORY_OFFSET;
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
    static mut HEAP: [u8; KERNEL_HEAP_SIZE] = [0; KERNEL_HEAP_SIZE];
    unsafe {
        HEAP_ALLOCATOR
            .lock()
            .init(NonNull::new_unchecked(HEAP.as_mut_ptr()), KERNEL_HEAP_SIZE);
    }
}