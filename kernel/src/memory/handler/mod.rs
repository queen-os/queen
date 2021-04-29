use super::*;
use alloc::boxed::Box;
use core::fmt::Debug;

mod byframe;
mod delay;
pub mod file;
mod linear;

pub use byframe::ByFrame;
pub use delay::Delay;
pub use file::File;
pub use linear::Linear;

pub trait MemoryHandler: Debug + Send + Sync + 'static {
    fn box_clone(&self) -> Box<dyn MemoryHandler>;

    /// Map `addr` in the page table
    /// Should set page flags here instead of in `page_fault_handler`
    fn map(&self, pt: &mut dyn PageTable, addr: VirtAddr, attr: &MemoryAttr);

    /// Unmap `addr` in the page table
    fn unmap(&self, pt: &mut dyn PageTable, addr: VirtAddr);

    /// Clone map `addr` from page table `src_pt` to `pt`.
    fn clone_map(
        &self,
        pt: &mut dyn PageTable,
        src_pt: &mut dyn PageTable,
        addr: VirtAddr,
        attr: &MemoryAttr,
    );

    /// Handle page fault on `addr`
    /// Return true if success, false if error
    fn handle_page_fault(&self, pt: &mut dyn PageTable, addr: VirtAddr) -> bool;
}

impl Clone for Box<dyn MemoryHandler> {
    fn clone(&self) -> Box<dyn MemoryHandler> {
        self.box_clone()
    }
}
