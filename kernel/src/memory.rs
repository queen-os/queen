use crate::consts::PHYSICAL_MEMORY_OFFSET;

/// Convert physical address to virtual address
#[inline]
pub const fn phys_to_virt(paddr: u64) -> u64 {
    // PHYSICAL_MEMORY_OFFSET as u64 + paddr
    paddr
}