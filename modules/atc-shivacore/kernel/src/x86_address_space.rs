// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! x86-64 page-table roots for isolated user address spaces.
//!
//! This module deliberately does not switch CR3 implicitly and does not own/free
//! page-table frames. Callers must keep each root and all reachable page-table
//! frames alive for as long as a process can run. Only supervisor-only upper-half
//! PML4 entries are inherited from the active kernel root; lower-half user
//! mappings are never copied between processes.

use x86_64::{
    registers::control::{Cr3, Cr3Flags},
    structures::paging::{FrameAllocator, Mapper, OffsetPageTable, PageTable, PageTableFlags, PhysFrame, Size4KiB},
    PhysAddr, VirtAddr,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressSpaceError {
    OutOfFrames,
    InvalidPhysicalMapping,
}

/// Owns the physical frame containing one PML4 root.
///
/// The root frame and intermediate page-table frames must not be freed while
/// this value is live or while any CPU can still execute in this address space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserAddressSpaceRoot {
    root: PhysFrame<Size4KiB>,
}

impl UserAddressSpaceRoot {
    /// Create a new PML4 root with the active kernel's supervisor-only upper
    /// half. The lower half starts empty so processes cannot inherit each
    /// other's user mappings.
    ///
    /// # Safety
    /// physical_memory_offset must be the bootloader-provided direct mapping
    /// for every physical frame touched here, and frame_allocator must return
    /// unique, usable frames that are not already allocated.
    pub unsafe fn new(
        physical_memory_offset: VirtAddr,
        frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    ) -> Result<Self, AddressSpaceError> {
        let (kernel_root_frame, _) = Cr3::read();
        let kernel_root_addr = physical_memory_offset
            .as_u64()
            .checked_add(kernel_root_frame.start_address().as_u64())
            .ok_or(AddressSpaceError::InvalidPhysicalMapping)?;
        let kernel_root_virt = VirtAddr::try_new(kernel_root_addr)
            .map_err(|_| AddressSpaceError::InvalidPhysicalMapping)?;
        let kernel_root_ptr = kernel_root_virt.as_ptr::<PageTable>();

        let root_frame = frame_allocator
            .allocate_frame()
            .ok_or(AddressSpaceError::OutOfFrames)?;
        let root_addr = physical_memory_offset
            .as_u64()
            .checked_add(root_frame.start_address().as_u64())
            .ok_or(AddressSpaceError::InvalidPhysicalMapping)?;
        let root_virt = VirtAddr::try_new(root_addr)
            .map_err(|_| AddressSpaceError::InvalidPhysicalMapping)?;
        let root_ptr = root_virt.as_mut_ptr::<PageTable>();

        // Initialize the complete table before copying any inherited entries.
        root_ptr.write(PageTable::new());
        let kernel_root = &*kernel_root_ptr;
        let new_root = &mut *root_ptr;

        // x86-64 canonical upper-half PML4 slots. A root entry with U/S clear
        // keeps every mapping below it supervisor-only, even if a lower entry
        // was misconfigured with USER_ACCESSIBLE.
        for index in 256..512 {
            let entry = &kernel_root[index];
            if !entry.is_unused()
                && entry.flags().contains(PageTableFlags::PRESENT)
                && !entry.flags().contains(PageTableFlags::USER_ACCESSIBLE)
            {
                new_root[index] = entry.clone();
            }
        }

        Ok(Self { root: root_frame })
    }

    /// Return the physical PML4 frame address for context metadata.
    pub fn physical_root(&self) -> PhysAddr {
        self.root.start_address()
    }

    /// Activate this address space on the current CPU.
    ///
    /// This reloads CR3 and therefore flushes non-global local TLB entries.
    /// It is not an SMP shootdown and must not be treated as one; remote CPUs
    /// that may have used this root require an explicit IPI/TLB invalidation
    /// protocol before mappings are changed or frames are reclaimed.
    ///
    /// # Safety
    /// All kernel mappings needed by the current execution path, stacks, IDT,
    /// GDT/TSS, interrupt handlers, and per-CPU data must be valid in this root.
    pub unsafe fn activate(&self) {
        Cr3::write(self.root, Cr3Flags::empty());
    }

    /// Build a mapper over this root for constructing this process's mappings.
    ///
    /// # Safety
    /// The direct physical mapping must remain valid. The returned mapper must
    /// not outlive the root's page-table frame or the physical-memory mapping.
    pub unsafe fn mapper(
        &self,
        physical_memory_offset: VirtAddr,
    ) -> Result<OffsetPageTable<'static>, AddressSpaceError> {
        let root_addr = physical_memory_offset
            .as_u64()
            .checked_add(self.root.start_address().as_u64())
            .ok_or(AddressSpaceError::InvalidPhysicalMapping)?;
        let root_virt = VirtAddr::try_new(root_addr)
            .map_err(|_| AddressSpaceError::InvalidPhysicalMapping)?;
        let root_ptr = root_virt.as_mut_ptr::<PageTable>();
        Ok(OffsetPageTable::new(&mut *root_ptr, physical_memory_offset))
    }
}
