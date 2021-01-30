use super::bsp::{MEMORY_END, MEMORY_START};
use crate::{
    consts::MEMORY_OFFSET,
    memory::{init_heap, FRAME_ALLOCATOR, PAGE_SIZE},
};

pub fn init() {
    init_heap();
    init_frame_allocator();
}

fn init_frame_allocator() {
    let page_start = ((MEMORY_START - MEMORY_OFFSET) / PAGE_SIZE) as usize;
    let page_end = ((MEMORY_END - MEMORY_OFFSET - 1) / PAGE_SIZE + 1) as usize;
    FRAME_ALLOCATOR.lock().insert(page_start..page_end);
    info!("FrameAllocator init end");
}
