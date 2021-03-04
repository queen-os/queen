use core::ops::Range;
use fdt_rs::{base::*, prelude::*};

pub use fdt_rs::error::DevTreeError;
pub type FdtResult<T> = fdt_rs::error::Result<T>;

#[derive(Copy, Clone, Debug)]
pub struct DeviceTree<'dt>(DevTree<'dt>);

impl<'dt> DeviceTree<'dt> {
    /// Parse flattened device tree from `addr`.
    /// # Safety
    /// Must ensure `addr` is valid.
    pub unsafe fn from_raw(addr: usize) -> FdtResult<Self> {
        let fdt_header = core::slice::from_raw_parts(addr as *mut u8, DevTree::MIN_HEADER_SIZE);
        let len: usize = DevTree::read_totalsize(fdt_header)?;
        let fdt = core::slice::from_raw_parts(addr as *mut u8, len);

        Ok(DeviceTree(DevTree::new(fdt)?))
    }

    /// Construct the parsable DevTree object from the provided byte slice.
    /// # Safety
    /// Callers of this method the must guarantee the following:
    /// The passed buffer is 32-bit aligned.
    /// The passed buffer is exactly the length returned by Self::read_totalsize()
    #[inline]
    pub unsafe fn new(buf: &'dt [u8]) -> FdtResult<Self> {
        let fdt = DevTree::new(buf)?;

        Ok(DeviceTree(fdt))
    }

    #[inline]
    pub fn device_tree(&self) -> &DevTree {
        &self.0
    }

    #[inline]
    pub fn totalsize(&self) -> usize {
        self.0.totalsize()
    }

    #[inline]
    pub fn root(&self) -> Option<DevTreeNode> {
        self.0.root().ok().flatten()
    }

    #[inline]
    pub fn nodes(&self) -> iters::DevTreeNodeIter {
        self.0.nodes()
    }

    /// Search for a node of device tree that satisfies a predict.
    #[inline]
    pub fn find_node<P>(&self, mut predict: P) -> Option<DevTreeNode>
    where
        P: FnMut(&DevTreeNode) -> FdtResult<bool>,
    {
        self.0.nodes().find(&mut predict).ok().flatten()
    }

    /// Search for a node of device tree whose prop satisfies a predict.
    #[inline]
    pub fn find_node_with_prop<P>(&self, mut predict: P) -> Option<DevTreeNode>
    where
        P: FnMut(DevTreeProp) -> FdtResult<bool>,
    {
        self.find_node(|node| node.props().any(&mut predict))
    }

    #[inline]
    pub fn root_size_cells(&self) -> Option<usize> {
        utils::read_node_prop_u32(&self.root()?, "#size-cells", 0)
    }

    #[inline]
    pub fn root_address_cells(&self) -> Option<usize> {
        utils::read_node_prop_u32(&self.root()?, "#address-cells", 0)
    }

    #[inline]
    pub fn node_size_cells(&self, node: &DevTreeNode) -> Option<usize> {
        utils::read_node_prop_u32(node, "#size-cells", 0).or_else(|| self.root_size_cells())
    }

    #[inline]
    pub fn node_address_cells(&self, node: &DevTreeNode) -> Option<usize> {
        utils::read_node_prop_u32(node, "#address-cells", 0).or_else(|| self.root_address_cells())
    }

    /// Returns node's `reg` prop's `address..address+len` ranges.
    pub fn node_reg_range_iter<'a>(
        &self,
        node: &'a DevTreeNode<'a, 'dt>,
    ) -> Option<RegRangeIter<'a, 'dt>> {
        let address_cells = self.node_address_cells(node)?;
        let size_cells = self.node_size_cells(node)?;
        debug_assert_eq!(address_cells, size_cells);

        let reg_prop = utils::find_prop_by_name(node, "reg")?;
        let range_count = reg_prop.length() / 4 / address_cells / 2;

        Some(RegRangeIter {
            reg_prop,
            cells: address_cells,
            curr_range: 0,
            range_count,
        })
    }
}

pub struct RegRangeIter<'a, 'dt: 'a> {
    reg_prop: DevTreeProp<'a, 'dt>,
    /// 1 or 2
    cells: usize,
    curr_range: usize,
    /// count of address length pairs
    range_count: usize,
}

impl<'a, 'dt: 'a> RegRangeIter<'a, 'dt> {}

impl<'a, 'dt: 'a> Iterator for RegRangeIter<'a, 'dt> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.curr_range == self.range_count {
            return None;
        }
        let curr = self.curr_range * 2;
        self.curr_range += 1;
        let (addr, len) = match self.cells {
            1 => (
                self.reg_prop.u32(curr).ok()? as usize,
                self.reg_prop.u32(curr + 1).ok()? as usize,
            ),
            2 => (
                self.reg_prop.u64(curr).ok()? as usize,
                self.reg_prop.u64(curr + 1).ok()? as usize,
            ),
            _ => unreachable!(),
        };
        Some(addr..addr + len)
    }
}

mod utils {
    use super::*;

    pub fn read_node_prop_u32(node: &DevTreeNode, prop_name: &str, index: usize) -> Option<usize> {
        find_prop_by_name(node, prop_name)?
            .u32(index)
            .ok()
            .map(|x| x as usize)
    }

    pub fn read_node_prop_u64(node: &DevTreeNode, prop_name: &str, index: usize) -> Option<usize> {
        find_prop_by_name(node, prop_name)?
            .u64(index)
            .ok()
            .map(|x| x as usize)
    }

    pub fn find_prop_by_name<'a, 'dt: 'a>(
        node: &'a DevTreeNode<'a, 'dt>,
        prop_name: &str,
    ) -> Option<DevTreeProp<'a, 'dt>> {
        node.props()
            .find(|prop| Ok(prop.name()?.eq(prop_name)))
            .ok()
            .flatten()
    }
}

impl<'dt> DeviceTree<'dt> {
    /// Returns physical memory address `start..end`
    pub fn probe_memory(&self) -> Option<Range<usize>> {
        let mem_node = self.find_node_with_prop(|prop| {
            Ok(prop.name()?.eq("device_type") && prop.str()?.eq("memory"))
        })?;

        self.node_reg_range_iter(&mem_node)?.next()
    }
}
