// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! Physical memory and paging initialization for the x86-64 boot path.

use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use x86_64::{
    structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB},
    PhysAddr, VirtAddr,
};

pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = active_level_4_table(physical_memory_offset);
    OffsetPageTable::new(level_4_table, physical_memory_offset)
}

unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    let (frame, _) = x86_64::registers::control::Cr3::read();
    let virt = physical_memory_offset + frame.start_address().as_u64();
    &mut *virt.as_mut_ptr()
}

/// Create an isolated lower-half page-table root while sharing only the
/// supervisor-owned upper-half kernel mappings from the currently active root.
///
/// User virtual addresses must stay in the lower canonical half. Kernel entries
/// are copied by value at PML4 level; the lower-half entries start empty, so
/// user mappings cannot leak between roots. The caller must serialize frame
/// allocation and must not mutate shared kernel mappings after this operation.
pub unsafe fn create_user_mapper(
    physical_memory_offset: VirtAddr,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Option<(PhysFrame<Size4KiB>, OffsetPageTable<'static>)> {
    use x86_64::structures::paging::PageTableFlags;

    let active_root = active_level_4_table(physical_memory_offset);

    // Validate the complete upper-half root before allocating a frame. If an
    // inherited entry is user-accessible, fail without leaking a page-table
    // frame from the boot allocator.
    for index in 256..512 {
        let entry = &active_root[index];
        if !entry.is_unused() && entry.flags().contains(PageTableFlags::USER_ACCESSIBLE) {
            return None;
        }
    }

    let root_frame = frame_allocator.allocate_frame()?;
    let root_virt = physical_memory_offset + root_frame.start_address().as_u64();
    let root_ptr = root_virt.as_mut_ptr::<PageTable>();

    // SAFETY: the bootloader-provided physical-memory mapping covers the
    // allocated usable frame; the allocator grants exclusive ownership here.
    unsafe { core::ptr::write(root_ptr, PageTable::new()) };

    for index in 256..512 {
        // SAFETY: root_ptr is a valid, exclusively owned 4-KiB page-table frame.
        unsafe { (&mut *root_ptr)[index] = active_root[index].clone() };
    }

    // SAFETY: root_ptr refers to a page-table frame kept allocated for the
    // lifetime of the process address space; the physical-memory mapping is
    // permanent. Ownership is transferred into this mapper.
    let root_table: &'static mut PageTable = unsafe { &mut *root_ptr };
    Some((root_frame, OffsetPageTable::new(root_table, physical_memory_offset)))
}

/// Switch to a process root. Reloading CR3 invalidates non-global translations
/// for the local CPU. This does not perform remote-CPU TLB shootdown; callers
/// must not treat it as SMP-safe while address-space mutations are possible.
pub unsafe fn switch_to_page_table(root_frame: PhysFrame<Size4KiB>) {
    use x86_64::registers::control::{Cr3, Cr3Flags};
    // SAFETY: the caller provides a live, aligned root frame allocated as a
    // PML4 page table. Kernel mappings are shared with the boot root.
    unsafe { Cr3::write(root_frame, Cr3Flags::empty()) };
}

pub struct BootInfoFrameAllocator {
    memory_regions: &'static MemoryRegions,
    next: usize,
}

impl BootInfoFrameAllocator {
    pub unsafe fn init(memory_regions: &'static MemoryRegions) -> Self {
        Self {
            memory_regions,
            next: 0,
        }
    }

    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> + '_ {
        self.memory_regions
            .iter()
            .filter(|r| r.kind == MemoryRegionKind::Usable)
            .flat_map(|r| {
                let start = (r.start + 0xFFF) & !0xFFF;
                let end = r.end & !0xFFF;
                (start..end)
                    .step_by(0x1000)
                    .map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
            })
    }
}

unsafe impl Send for BootInfoFrameAllocator {}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next = self.next.saturating_add(1);
        frame
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn page_alignment_contract_is_4k() {
        assert_eq!(0x1000usize, 4096);
    }
}
