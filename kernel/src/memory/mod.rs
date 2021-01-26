use crate::consts::{KERNEL_OFFSET, MEMORY_OFFSET, PHYSICAL_MEMORY_OFFSET};
use aarch64::paging::PageSize;
use bitmap_allocator::BitAlloc;
use spin::Mutex;

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

pub type FrameAlloc = bitmap_allocator::BitAlloc1M;
pub static FRAME_ALLOCATOR: Mutex<FrameAlloc> = Mutex::new(FrameAlloc::DEFAULT);

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

pub fn alloc_frame() -> Option<PhysAddr> {
    // get the real address of the alloc frame
    let ret = FRAME_ALLOCATOR
        .lock()
        .alloc()
        .map(|id| id as u64 * PAGE_SIZE + MEMORY_OFFSET);
    trace!("Allocate frame: {:x?}", ret);
    ret
}

pub fn dealloc_frame(target: PhysAddr) {
    trace!("Deallocate frame: {:x}", target);
    FRAME_ALLOCATOR
        .lock()
        .dealloc(((target - MEMORY_OFFSET) / PAGE_SIZE) as usize);
}

pub fn alloc_frame_contiguous(size: usize, align_log2: usize) -> Option<PhysAddr> {
    // get the real address of the alloc frame
    let ret = FRAME_ALLOCATOR
        .lock()
        .alloc_contiguous(size, align_log2)
        .map(|id| id as u64 * PAGE_SIZE + MEMORY_OFFSET);
    trace!("Allocate frame: {:x?}", ret);
    ret
}
