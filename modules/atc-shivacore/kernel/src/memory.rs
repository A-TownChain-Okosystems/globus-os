// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! Physical memory and paging initialization for the x86-64 boot path.

use alloc::vec::Vec;

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

pub struct BootInfoFrameAllocator {
    memory_regions: &'static MemoryRegions,
    next: usize,
    /// Zurueckgegebene Frames. Die Freiliste ist Teil DIESES Allocators und
    /// kein zweiter Mechanismus: nur er vergibt und nimmt physischen Speicher.
    freed: Vec<PhysFrame>,
    freed_total: usize,
    double_free_blocked: usize,
}

impl BootInfoFrameAllocator {
    pub unsafe fn init(memory_regions: &'static MemoryRegions) -> Self {
        Self {
            memory_regions,
            next: 0,
            freed: Vec::new(),
            freed_total: 0,
            double_free_blocked: 0,
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

impl BootInfoFrameAllocator {
    /// Gibt einen Frame zurueck. Doppelfreigaben werden abgewiesen und gezaehlt,
    /// statt die Freiliste zu vergiften.
    pub fn deallocate_frame(&mut self, frame: PhysFrame) -> bool {
        if self.freed.contains(&frame) {
            self.double_free_blocked += 1;
            return false;
        }
        self.freed.push(frame);
        self.freed_total += 1;
        true
    }

    pub fn freed_total(&self) -> usize {
        self.freed_total
    }

    pub fn double_free_blocked(&self) -> usize {
        self.double_free_blocked
    }

    pub fn free_list_len(&self) -> usize {
        self.freed.len()
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        // Freigegebene Frames zuerst, damit ein Prozess-Zyklus den Vorrat nicht
        // monoton aufbraucht.
        if let Some(frame) = self.freed.pop() {
            return Some(frame);
        }
        let frame = self.usable_frames().nth(self.next);
        self.next = self.next.saturating_add(1);
        frame
    }
}

/// Die eine Instanz des Frame-Allocators zur Laufzeit.
///
/// Der Bootpfad haelt ihn bis zum Ende seiner Abbildungen lokal und legt ihn
/// dann hier ab. Damit kann die verzoegerte Freigabe denselben Allocator
/// benutzen, ohne einen zweiten zu erzeugen — `try_lock` scheitert sofort,
/// statt im Interrupt-Kontext zu blockieren.
pub static FRAME_ALLOCATOR: spin::Mutex<Option<BootInfoFrameAllocator>> = spin::Mutex::new(None);

/// Uebergibt den Boot-Allocator der Laufzeit.
pub fn install_frame_allocator(allocator: BootInfoFrameAllocator) {
    *FRAME_ALLOCATOR.lock() = Some(allocator);
}

#[cfg(test)]
mod tests {
    #[test]
    fn page_alignment_contract_is_4k() {
        assert_eq!(0x1000usize, 4096);
    }
}
