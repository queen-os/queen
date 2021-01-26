use super::*;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Linear {
    offset: i64,
}

impl MemoryHandler for Linear {
    fn box_clone(&self) -> Box<dyn MemoryHandler> {
        Box::new(self.clone())
    }

    fn map(&self, pt: &mut dyn PageTable, addr: VirtAddr, attr: &MemoryAttr) {
        let target = (addr as i64 + self.offset) as PhysAddr;
        let entry = pt.map(addr, target);
        attr.apply(entry);
    }

    fn unmap(&self, pt: &mut dyn PageTable, addr: VirtAddr) {
        pt.unmap(addr);
    }

    fn clone_map(
        &self,
        pt: &mut dyn PageTable,
        _src_pt: &mut dyn PageTable,
        addr: VirtAddr,
        attr: &MemoryAttr,
    ) {
        self.map(pt, addr, attr);
    }

    fn handle_page_fault(&self, _pt: &mut dyn PageTable, _addr: VirtAddr) -> bool {
        false
    }
}

impl Linear {
    pub fn new(offset: i64) -> Self {
        Linear { offset }
    }
}
