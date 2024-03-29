//! Page table implementations for aarch64.

use crate::{
    consts::PHYSICAL_MEMORY_OFFSET,
    memory::{alloc_frames, dealloc_frames, phys_to_virt, Entry, PageTable, PageTableExt},
};
use aarch64::{
    addr::{align_down, align_up, PhysAddr, ALIGN_2MIB},
    cache::*,
    paging::{
        mapper::{Mapper, OffsetPageTable},
        memory_attribute::*,
        table::{PageTable as RawPageTable, PageTableEntry, PageTableFlags as EF},
        Frame, FrameAllocator, FrameDeallocator, Page as PageAllSizes, Size2MiB, Size4KiB,
    },
    translation::{invalidate_tlb_vaddr, local_invalidate_tlb_all, ttbr_el1_read, ttbr_el1_write},
};
use core::mem::ManuallyDrop;
use log::*;

type Page = PageAllSizes<Size4KiB>;

pub struct PageTableImpl {
    page_table: OffsetPageTable<'static>,
    root_frame: Frame,
    entry: Option<PageEntry>,
}

pub struct PageEntry(&'static mut PageTableEntry, Page);

impl PageTable for PageTableImpl {
    fn map(&mut self, addr: usize, target: usize) -> &mut dyn Entry {
        let flags = EF::default_page() | EF::PXN | EF::UXN;
        let attr = MairNormal::attr_value();
        unsafe {
            self.page_table
                .map_to(
                    Page::of_addr(addr as u64),
                    Frame::of_addr(target as u64),
                    flags,
                    attr,
                    &mut FrameAllocatorForAarch64,
                )
                .unwrap()
                .flush();
        }
        self.get_entry(addr).expect("fail to get entry")
    }

    fn unmap(&mut self, addr: usize) {
        self.page_table
            .unmap(Page::of_addr(addr as u64))
            .unwrap()
            .1
            .flush();
    }

    fn get_entry(&mut self, vaddr: usize) -> Option<&mut dyn Entry> {
        let page = Page::of_addr(vaddr as u64);
        if let Ok(e) = self.page_table.get_entry_mut(page) {
            let e = unsafe { &mut *(e as *mut PageTableEntry) };
            self.entry = Some(PageEntry(e, page));
            Some(self.entry.as_mut().unwrap())
        } else {
            None
        }
    }

    fn get_page_slice_mut<'a>(&mut self, addr: usize) -> &'a mut [u8] {
        let frame = self
            .page_table
            .translate_page(Page::of_addr(addr as u64))
            .unwrap();
        let vaddr = phys_to_virt(frame.start_address().as_usize());
        unsafe { core::slice::from_raw_parts_mut(vaddr as *mut u8, 0x1000) }
    }

    fn flush_cache_copy_user(&mut self, start: usize, end: usize, execute: bool) {
        if execute {
            // clean D-cache to PoU to ensure new instructions has been written
            // into memory
            DCache::<Clean, PoU>::flush_range(start as usize, end as usize, ISH);
            // invalidate I-cache to PoU to ensure old instructions has been
            // flushed
            if get_l1_icache_policy() == L1ICachePolicy::PIPT {
                // Cortex-A57 use PIPT, address translation is transparent
                ICache::<Invalidate, PoU>::flush_range(start as usize, end as usize, ISH);
            } else {
                // Cortex-A53 (raspi3) use VIPT, the effect of invalidation is
                // only visible to the VA, need to invalidate the entire
                // I-cache to invalidate all aliases of a PA.
                ICache::flush_all();
            }
        }
    }
}

fn frame_to_page_table(frame: Frame) -> *mut RawPageTable {
    let vaddr = phys_to_virt(frame.start_address().as_usize());
    vaddr as *mut RawPageTable
}

#[repr(u8)]
pub enum MMIOType {
    Normal = 0,
    Device = 1,
    NormalNonCacheable = 2,
    Unsupported = 3,
}

// TODO: software dirty bit needs to be reconsidered
impl Entry for PageEntry {
    fn update(&mut self) {
        invalidate_tlb_vaddr(self.1.start_address());
    }

    fn present(&self) -> bool {
        self.0.flags().contains(EF::VALID)
    }
    fn accessed(&self) -> bool {
        self.0.flags().contains(EF::AF)
    }
    fn writable(&self) -> bool {
        self.0.flags().contains(EF::WRITE)
    }
    fn dirty(&self) -> bool {
        self.hw_dirty() && self.sw_dirty()
    }

    fn clear_accessed(&mut self) {
        self.as_flags().remove(EF::AF);
    }
    fn clear_dirty(&mut self) {
        self.as_flags().remove(EF::DIRTY);
        self.as_flags().insert(EF::AP_RO);
    }
    fn set_writable(&mut self, value: bool) {
        self.as_flags().set(EF::AP_RO, !value);
        self.as_flags().set(EF::WRITE, value);
    }
    fn set_present(&mut self, value: bool) {
        self.as_flags().set(EF::VALID, value);
    }
    fn target(&self) -> usize {
        self.0.addr().as_usize()
    }
    fn set_target(&mut self, target: usize) {
        self.0
            .set_addr(PhysAddr::new(target as u64), self.0.flags(), self.0.attr());
    }
    fn writable_shared(&self) -> bool {
        self.0.flags().contains(EF::WRITABLE_SHARED)
    }
    fn readonly_shared(&self) -> bool {
        self.0.flags().contains(EF::READONLY_SHARED)
    }
    fn set_shared(&mut self, writable: bool) {
        let flags = self.as_flags();
        flags.set(EF::WRITABLE_SHARED, writable);
        flags.set(EF::READONLY_SHARED, !writable);
    }
    fn clear_shared(&mut self) {
        self.as_flags()
            .remove(EF::WRITABLE_SHARED | EF::READONLY_SHARED);
    }
    fn user(&self) -> bool {
        self.0.flags().contains(EF::AP_EL0)
    }
    fn swapped(&self) -> bool {
        self.0.flags().contains(EF::SWAPPED)
    }
    fn set_swapped(&mut self, value: bool) {
        self.as_flags().set(EF::SWAPPED, value);
    }
    fn set_user(&mut self, value: bool) {
        self.as_flags().set(EF::AP_EL0, value);
        self.as_flags().set(EF::nG, value); // set non-global to use ASID
    }
    fn execute(&self) -> bool {
        if self.user() {
            !self.0.flags().contains(EF::UXN)
        } else {
            !self.0.flags().contains(EF::PXN)
        }
    }
    fn set_execute(&mut self, value: bool) {
        if self.user() {
            self.as_flags().set(EF::UXN, !value);
            self.as_flags().set(EF::PXN, true);
        } else {
            self.as_flags().set(EF::PXN, !value);
            self.as_flags().set(EF::UXN, true)
        }
    }
    fn mmio(&self) -> u8 {
        let attr = self.0.attr().value;
        let mmio_type = if attr == MairNormal::attr_value().value {
            MMIOType::Normal
        } else if attr == MairDevice::attr_value().value {
            MMIOType::Device
        } else if attr == MairNormalNonCacheable::attr_value().value {
            MMIOType::NormalNonCacheable
        } else {
            MMIOType::Unsupported
        };
        mmio_type as u8
    }
    fn set_mmio(&mut self, value: u8) {
        let attr = match value {
            0 => MairNormal::attr_value(),
            1 => MairDevice::attr_value(),
            2 => MairNormalNonCacheable::attr_value(),
            _ => return,
        };
        self.0.set_attr(attr);
    }
}

impl PageEntry {
    fn read_only(&self) -> bool {
        self.0.flags().contains(EF::AP_RO)
    }
    fn hw_dirty(&self) -> bool {
        self.writable() && !self.read_only()
    }
    fn sw_dirty(&self) -> bool {
        self.0.flags().contains(EF::DIRTY)
    }
    fn as_flags(&mut self) -> &mut EF {
        unsafe { &mut *(self.0 as *mut _ as *mut EF) }
    }
}

impl PageTableImpl {
    /// Unsafely get the current active page table.
    /// Using ManuallyDrop to wrap the page table: this is how `core::mem::forget` is implemented now.
    /// # Safety
    pub unsafe fn active() -> ManuallyDrop<Self> {
        let frame = Frame::of_addr(PageTableImpl::active_token() as u64);
        let table = &mut *frame_to_page_table(frame);
        ManuallyDrop::new(PageTableImpl {
            page_table: OffsetPageTable::new(table, PHYSICAL_MEMORY_OFFSET.into()),
            root_frame: frame,
            entry: None,
        })
    }

    /// The method for getting the kernel page table.
    /// In aarch64 case kernel page table and user page table are two different tables.
    /// # Safety
    pub unsafe fn kernel_table() -> ManuallyDrop<Self> {
        let frame = Frame::of_addr(ttbr_el1_read(1).start_address().as_u64());
        let table = &mut *frame_to_page_table(frame);
        ManuallyDrop::new(PageTableImpl {
            page_table: OffsetPageTable::new(table, PHYSICAL_MEMORY_OFFSET.into()),
            root_frame: frame,
            entry: None,
        })
    }

    /// Activate as kernel page table (TTBR1_EL1).
    /// Used in `arch::memory::map_kernel()`.
    /// # Safety
    pub unsafe fn activate_as_kernel(&self) {
        ttbr_el1_write(1, Frame::of_addr(self.token() as u64));
        local_invalidate_tlb_all();
    }

    /// Map physical memory [start, end)
    /// to virtual space [phys_to_virt(start), phys_to_virt(end))
    pub fn map_physical_memory(&mut self, start: usize, end: usize) {
        let aligned_start = align_down(start as u64, ALIGN_2MIB);
        let aligned_end = align_up(end as u64, ALIGN_2MIB);
        let flags = EF::default_block() | EF::UXN | EF::PXN;
        let attr = MairNormal::attr_value();
        for frame in Frame::<Size2MiB>::range_of(aligned_start, aligned_end) {
            let paddr = frame.start_address();
            let vaddr = phys_to_virt(paddr.as_usize());
            let page = PageAllSizes::<Size2MiB>::of_addr(vaddr as u64);
            unsafe {
                self.page_table
                    .map_to(page, frame, flags, attr, &mut FrameAllocatorForAarch64)
                    .expect("failed to map physical memory")
                    .flush();
            }
        }
        info!(
            "Mapped physical memory from 0x{:X}-0x{:X} to 0x{:X}-0x{:X}.",
            start,
            end,
            phys_to_virt(start),
            phys_to_virt(end)
        );
    }
}

impl PageTableExt for PageTableImpl {
    fn new_bare() -> Self {
        let target = alloc_frames(1).expect("failed to allocate frame");
        let frame = Frame::of_addr(target as u64);
        let table = unsafe { &mut *frame_to_page_table(frame) };
        table.clear();
        unsafe {
            PageTableImpl {
                page_table: OffsetPageTable::new(table, PHYSICAL_MEMORY_OFFSET.into()),
                root_frame: frame,
                entry: None,
            }
        }
    }

    fn map_kernel(&mut self) {
        // kernel page table is based on TTBR0_EL1 and will never change.
    }

    fn token(&self) -> u64 {
        self.root_frame.start_address().as_u64() // as TTBR0_EL1
    }

    unsafe fn set_token(token: u64) {
        ttbr_el1_write(0, Frame::of_addr(token));
    }

    fn active_token() -> u64 {
        ttbr_el1_read(0).start_address().as_u64()
    }

    fn flush_tlb() {
        local_invalidate_tlb_all();
    }
}

impl Drop for PageTableImpl {
    fn drop(&mut self) {
        info!("PageTable dropping: {:?}", self.root_frame);
        dealloc_frames(self.root_frame.start_address().as_usize(), 1);
    }
}

struct FrameAllocatorForAarch64;

unsafe impl FrameAllocator<Size4KiB> for FrameAllocatorForAarch64 {
    fn allocate_frame(&mut self) -> Option<Frame> {
        alloc_frames(1).map(|addr| Frame::of_addr(addr as u64))
    }
}

impl FrameDeallocator<Size4KiB> for FrameAllocatorForAarch64 {
    fn deallocate_frame(&mut self, frame: Frame) {
        dealloc_frames(frame.start_address().as_usize(), 1);
    }
}
