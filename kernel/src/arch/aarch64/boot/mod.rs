use super::{
    bsp::{MEMORY_END, MEMORY_START, PERIPHERALS_END, PERIPHERALS_START},
    consts::KERNEL_OFFSET,
};
use crate::memory::phys_to_virt;
use aarch64::{
    addr::{align_down, align_up, ALIGN_2MIB},
    asm::cpuid,
    barrier, cache,
    paging::{
        memory_attribute::*, Frame, Page, PageTable, PageTableAttribute as Attr,
        PageTableFlags as EF, Size2MiB, Size4KiB,
    },
    registers::*,
    translation,
};

global_asm!(include_str!("entry.S"));

#[link_section = ".text.boot"]
fn map_2mib(p2: &mut PageTable, start: usize, end: usize, flag: EF, attr: Attr) {
    let aligned_start = align_down(start as u64, ALIGN_2MIB);
    let aligned_end = align_up(end as u64, ALIGN_2MIB);
    for frame in Frame::<Size2MiB>::range_of(aligned_start, aligned_end) {
        let paddr = frame.start_address();
        let page = Page::<Size2MiB>::of_addr(paddr.as_usize() as u64);
        p2[page.p2_index()].set_block::<Size2MiB>(paddr, flag, attr);
    }
}

#[no_mangle]
#[link_section = ".text.boot"]
pub extern "C" fn create_init_paging() {
    let p4 = unsafe { &mut *(page_table_lvl4 as *mut PageTable) };
    let p3 = unsafe { &mut *(page_table_lvl3 as *mut PageTable) };
    let p2_0 = unsafe { &mut *(page_table_lvl2_0 as *mut PageTable) };
    let p2_1 = unsafe { &mut *(page_table_lvl2_1 as *mut PageTable) };
    p4.clear();
    p3.clear();
    p2_0.clear();
    p2_1.clear();

    let frame_lvl3 = Frame::<Size4KiB>::of_addr(page_table_lvl3 as usize as u64);
    let frame_lvl2_0 = Frame::<Size4KiB>::of_addr(page_table_lvl2_0 as usize as u64);
    let frame_lvl2_1 = Frame::<Size4KiB>::of_addr(page_table_lvl2_1 as usize as u64);

    // 0x0000_0000_0000 ~ 0x0080_0000_0000
    p4[0].set_frame(frame_lvl3, EF::default_table(), Attr::new(0, 0, 0));
    // 0x8000_0000_0000 ~ 0x8080_0000_0000
    p4[256].set_frame(frame_lvl3, EF::default_table(), Attr::new(0, 0, 0));

    // 0x0000_0000 ~ 0x4000_0000
    p3[0].set_frame(frame_lvl2_0, EF::default_table(), Attr::new(0, 0, 0));
    // 0x4000_0000 ~ 0x8000_0000
    p3[1].set_frame(frame_lvl2_1, EF::default_table(), Attr::new(0, 0, 0));

    let block_flags = EF::default_block() | EF::UXN;
    // normal memory
    map_2mib(
        p2_1,
        MEMORY_START,
        MEMORY_END,
        block_flags,
        MairNormal::attr_value(),
    );
    // device memory
    map_2mib(
        p2_0,
        PERIPHERALS_START,
        PERIPHERALS_END,
        block_flags | EF::PXN,
        MairDevice::attr_value(),
    );
}

#[no_mangle]
#[link_section = ".text.boot"]
pub unsafe extern "C" fn enable_mmu() {
    MAIR_EL1.write(
        MAIR_EL1::Attr0.val(MairNormal::config_value())
            + MAIR_EL1::Attr1.val(MairDevice::config_value())
            + MAIR_EL1::Attr2.val(MairNormalNonCacheable::config_value()),
    );

    // Configure various settings of stage 1 of the EL1 translation regime.
    let ips = ID_AA64MMFR0_EL1.read(ID_AA64MMFR0_EL1::PARange);
    TCR_EL1.write(
        TCR_EL1::TBI1::Ignored
            + TCR_EL1::TBI0::Ignored
            + TCR_EL1::AS::ASID16Bits
            + TCR_EL1::IPS.val(ips)
            + TCR_EL1::TG1::KiB_4
            + TCR_EL1::SH1::Inner
            + TCR_EL1::ORGN1::WriteBack_ReadAlloc_WriteAlloc_Cacheable
            + TCR_EL1::IRGN1::WriteBack_ReadAlloc_WriteAlloc_Cacheable
            + TCR_EL1::EPD1::EnableTTBR1Walks
            + TCR_EL1::A1.val(0)
            + TCR_EL1::T1SZ.val(16)
            + TCR_EL1::TG0::KiB_4
            + TCR_EL1::SH0::Inner
            + TCR_EL1::ORGN0::WriteBack_ReadAlloc_WriteAlloc_Cacheable
            + TCR_EL1::IRGN0::WriteBack_ReadAlloc_WriteAlloc_Cacheable
            + TCR_EL1::EPD0::EnableTTBR0Walks
            + TCR_EL1::T0SZ.val(16),
    );

    // Set both TTBR0_EL1 and TTBR1_EL1
    let frame_lvl4 = Frame::<Size4KiB>::of_addr(page_table_lvl4 as usize as u64);
    translation::ttbr_el1_write(0, frame_lvl4);
    translation::ttbr_el1_write(1, frame_lvl4);
    translation::local_invalidate_tlb_all();

    // Set new stack pointer and link register.
    SP.set((KERNEL_OFFSET + bootstacktop as usize - (cpuid() << 18)) as u64);
    LR.set(LR.get() + KERNEL_OFFSET as u64);

    barrier::isb(barrier::SY);
    // Enable the MMU and turn on data and instruction caching.
    SCTLR_EL1.modify(SCTLR_EL1::M::Enable + SCTLR_EL1::C::Cacheable + SCTLR_EL1::I::Cacheable);
    // Force MMU init to complete before next instruction
    barrier::isb(barrier::SY);

    // Invalidate the local I-cache so that any instructions fetched
    // speculatively from the PoC are discarded
    cache::ICache::local_flush_all();
}

#[no_mangle]
#[link_section = ".text.boot"]
pub extern "C" fn clear_bss() {
    let start = sbss as usize;
    let end = ebss as usize;
    let step = core::mem::size_of::<usize>();
    for i in (start..end).step_by(step) {
        unsafe { (i as *mut usize).write(0) };
    }
}

extern "C" {
    fn sbss();
    fn ebss();
    fn page_table_lvl4();
    fn page_table_lvl3();
    fn page_table_lvl2_0();
    fn page_table_lvl2_1();
    fn _start();
    fn _end();
    fn bootstacktop();
}
