use super::{PhysAddr, VirtAddr, PAGE_SIZE};
use core::ops::{Add, AddAssign};

pub trait PageTable {
    // type Entry: Entry;

    /// Map a page of virtual address `addr` to the frame of physics address `target`
    /// Return the page table entry of the mapped virtual address
    fn map(&mut self, addr: VirtAddr, target: PhysAddr) -> &mut dyn Entry;

    /// Unmap a page of virtual address `addr`
    fn unmap(&mut self, addr: VirtAddr);

    /// Get the page table entry of a page of virtual address `addr`
    /// If its page do not exist, return `None`
    fn get_entry(&mut self, addr: VirtAddr) -> Option<&mut dyn Entry>;

    /// Get a mutable reference of the content of a page of virtual address `addr`
    fn get_page_slice_mut<'a>(&mut self, addr: VirtAddr) -> &'a mut [u8];

    /// When copied user data (in page fault handler)，maybe need to flush I/D cache.
    fn flush_cache_copy_user(&mut self, start: VirtAddr, end: VirtAddr, execute: bool);

    /// Read data from virtual address `addr`
    /// Used for testing with mock
    fn read(&mut self, _addr: VirtAddr) -> u8 {
        unimplemented!()
    }

    /// Write data to virtual address `addr`
    /// Used for testing with mock
    fn write(&mut self, _addr: VirtAddr, _data: u8) {
        unimplemented!()
    }
}

/// Extra methods of `PageTable` for non-trait-object usage
pub trait PageTableExt: PageTable + Sized {
    /// Create a new page table with kernel memory mapped
    fn new() -> Self {
        let mut pt = Self::new_bare();
        pt.map_kernel();
        pt
    }

    /// Create a new page table without kernel memory mapped
    fn new_bare() -> Self;

    /// Map kernel segments
    fn map_kernel(&mut self);

    /// TTBR on AArch64
    fn token(&self) -> u64;
    /// # Safety
    unsafe fn set_token(token: u64);
    fn active_token() -> u64;
    fn flush_tlb();

    /// Activate this page table
    /// # Safety
    unsafe fn activate(&self) {
        let old_token = Self::active_token();
        let new_token = self.token();
        debug!("switch table {:x?} -> {:x?}", old_token, new_token);
        if old_token != new_token {
            Self::set_token(new_token);
            Self::flush_tlb();
        }
    }

    /// Execute function `f` with this page table activated
    /// # Safety
    unsafe fn with<T>(&self, f: impl FnOnce() -> T) -> T {
        let old_token = Self::active_token();
        let new_token = self.token();
        debug!("switch table {:x?} -> {:x?}", old_token, new_token);
        if old_token != new_token {
            Self::set_token(new_token);
            Self::flush_tlb();
        }
        let ret = f();
        debug!("switch table {:x?} -> {:x?}", new_token, old_token);
        if old_token != new_token {
            Self::set_token(old_token);
            Self::flush_tlb();
        }
        ret
    }
}

/// Page Table Entry
pub trait Entry {
    /// Make all changes take effect.
    ///
    /// IMPORTANT!
    /// This must be called after any change to ensure it become effective.
    /// Usually it will cause a TLB/MMU flush.
    fn update(&mut self);
    /// A bit set by hardware when the page is accessed
    fn accessed(&self) -> bool;
    /// A bit set by hardware when the page is written
    fn dirty(&self) -> bool;
    /// Will PageFault when try to write page where writable=0
    fn writable(&self) -> bool;
    /// Will PageFault when try to access page where present=0
    fn present(&self) -> bool;

    fn clear_accessed(&mut self);
    fn clear_dirty(&mut self);
    fn set_writable(&mut self, value: bool);
    fn set_present(&mut self, value: bool);

    /// The target physics address in the entry
    /// Can be used for other purpose if present=0
    fn target(&self) -> PhysAddr;
    fn set_target(&mut self, target: PhysAddr);

    // For Copy-on-write
    fn writable_shared(&self) -> bool;
    fn readonly_shared(&self) -> bool;
    fn set_shared(&mut self, writable: bool);
    fn clear_shared(&mut self);

    // For Swap
    fn swapped(&self) -> bool;
    fn set_swapped(&mut self, value: bool);

    fn user(&self) -> bool;
    fn set_user(&mut self, value: bool);
    fn execute(&self) -> bool;
    fn set_execute(&mut self, value: bool);
    fn mmio(&self) -> u8;
    fn set_mmio(&mut self, value: u8);
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Page {
    number: usize,
}

impl Page {
    pub const SIZE: usize = PAGE_SIZE;

    /// Returns the start address of the page.
    pub fn start_address(&self) -> VirtAddr {
        self.number * PAGE_SIZE
    }

    /// Returns the page that contains the given virtual address.
    pub fn of_addr(addr: VirtAddr) -> Self {
        Page {
            number: addr / PAGE_SIZE,
        }
    }

    /// Returns a range of pages between address [begin, end)
    pub fn range_of(begin: VirtAddr, end: VirtAddr) -> PageRange {
        PageRange {
            start: Page::of_addr(begin),
            end: Page::of_addr(end - 1),
        }
    }
}

impl Add<usize> for Page {
    type Output = Self;
    fn add(self, rhs: usize) -> Self::Output {
        Page {
            number: self.number + rhs,
        }
    }
}

impl AddAssign<usize> for Page {
    fn add_assign(&mut self, rhs: usize) {
        *self = self.clone() + rhs;
    }
}

/// A range of pages with exclusive upper bound.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct PageRange {
    start: Page,
    end: Page,
}

impl Iterator for PageRange {
    type Item = Page;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start <= self.end {
            let page = self.start.clone();
            self.start += 1;
            Some(page)
        } else {
            None
        }
    }
}
