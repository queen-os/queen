use core::ops::Range;
use fdt_rs::{base::*, prelude::*};

pub struct DeviceTree<'dt>(DevTree<'dt>);

impl<'dt> DeviceTree<'dt> {
    /// Parse flattened device tree from `addr`.
    /// # Safety
    /// Must ensure `addr` is valid.
    pub unsafe fn new(addr: usize) -> fdt_rs::error::Result<Self> {
        let fdt_header = core::slice::from_raw_parts(addr as *mut u8, DevTree::MIN_HEADER_SIZE);
        let len: usize = DevTree::read_totalsize(fdt_header)?;
        let fdt = core::slice::from_raw_parts(addr as *mut u8, len);

        Ok(DeviceTree(DevTree::new(fdt)?))
    }

    #[inline]
    pub fn device_tree(&self) -> DevTree {
        self.0
    }

    /// Returns physical memory address `start..end`
    pub fn probe_memory(&self) -> Option<Range<usize>> {
        let mem_node = self
            .0
            .nodes()
            .find(|node| node.name().map(|name| name.starts_with("memory")))
            .ok()
            .flatten()?;

        let reg = mem_node
            .props()
            .find(|prop| prop.name().map(|name| name == "reg"))
            .ok()
            .flatten()?;

        let start = reg.u64(0).ok()? as usize;
        let len = reg.u64(1).ok()? as usize;

        Some(Range {
            start,
            end: start + len,
        })
    }
}
