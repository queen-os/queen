use core::ops::Range;

use super::bsp::{PERIPHERALS_END, PERIPHERALS_START};
use crate::{
    consts::KERNEL_OFFSET,
    memory::{
        as_lower_range, handler::Linear, init_heap, MMIOType, MemoryAttr, MemorySet,
        FRAME_ALLOCATOR, PAGE_SIZE,
    },
    sync::spin::MutexNoIrq as Mutex,
};
use aarch64::{
    paging::Frame,
    registers::{RegisterReadWrite, FAR_EL1},
    translation::{local_invalidate_tlb_all, ttbr_el1_write},
};

static KERNEL_MEMORY_SET: Mutex<Option<MemorySet>> = Mutex::new(None);

#[derive(Debug, Clone)]
pub struct MemInitOpts {
    phys_mem_range: Range<usize>,
}

impl MemInitOpts {
    pub fn new(phys_mem_range: Range<usize>) -> Self {
        Self { phys_mem_range }
    }
}

pub fn init(opts: MemInitOpts) {
    init_heap();
    init_frame_allocator(&opts);
    map_kernel(&opts);
    info!("memory: init end");
}

pub fn init_other() {
    if let Some(ms) = KERNEL_MEMORY_SET.lock().as_mut() {
        unsafe { ms.get_page_table_mut().activate_as_kernel() };
    }
}

fn init_frame_allocator(MemInitOpts { phys_mem_range }: &MemInitOpts) {
    let page_start = (as_lower_range(symbol_addr!("_end")) - phys_mem_range.start) / PAGE_SIZE;
    let page_end = (phys_mem_range.len() - 1) / PAGE_SIZE + 1;
    FRAME_ALLOCATOR.lock().insert(page_start..page_end);
    info!("FrameAllocator init end");
}

/// Create fine-grained mappings for the kernel
fn map_kernel(MemInitOpts { phys_mem_range }: &MemInitOpts) {
    let offset = -(KERNEL_OFFSET as isize);
    let mut ms = MemorySet::new();
    ms.push(
        symbol_addr!("stext"),
        symbol_addr!("etext"),
        MemoryAttr::default().execute().readonly(),
        Linear::new(offset),
        "text",
    );
    ms.push(
        symbol_addr!("sdata"),
        symbol_addr!("edata"),
        MemoryAttr::default(),
        Linear::new(offset),
        "data",
    );
    ms.push(
        symbol_addr!("srodata"),
        symbol_addr!("erodata"),
        MemoryAttr::default().readonly(),
        Linear::new(offset),
        "rodata",
    );

    ms.push(
        symbol_addr!("sbss"),
        symbol_addr!("ebss"),
        MemoryAttr::default(),
        Linear::new(offset),
        "bss",
    );
    ms.push(
        symbol_addr!("bootstack"),
        symbol_addr!("bootstacktop"),
        MemoryAttr::default(),
        Linear::new(offset),
        "kstack",
    );
    ms.push(
        PERIPHERALS_START,
        PERIPHERALS_END,
        MemoryAttr::default().mmio(MMIOType::Device as u8),
        Linear::new(0),
        "peripherals",
    );

    let page_table = ms.get_page_table_mut();
    page_table.map_physical_memory(phys_mem_range.start, phys_mem_range.end);
    unsafe { page_table.activate_as_kernel() };
    *KERNEL_MEMORY_SET.lock() = Some(ms);

    info!("map kernel end");
}

/// map the I/O memory range into the kernel page table
pub fn ioremap(paddr: usize, len: usize, name: &'static str) -> usize {
    let offset = -(KERNEL_OFFSET as isize);
    let vaddr = paddr.wrapping_add(KERNEL_OFFSET);
    if let Some(ms) = KERNEL_MEMORY_SET.lock().as_mut() {
        ms.push(
            vaddr,
            vaddr + len,
            MemoryAttr::default().mmio(MMIOType::NormalNonCacheable as u8),
            Linear::new(offset),
            name,
        );
        return vaddr;
    }
    0
}

pub fn set_page_table(vmtoken: usize) {
    ttbr_el1_write(0, Frame::of_addr(vmtoken as u64));
    local_invalidate_tlb_all();
}

pub fn get_page_fault_addr() -> usize {
    FAR_EL1.get() as usize
}
