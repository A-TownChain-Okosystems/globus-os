// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! ShivaCore M1.2 VMM — fail-closed validation primitives.
//!
//! This module deliberately contains only deterministic validation and policy
//! primitives. Hardware page-table mutation remains a separate M1.2 step.
#![allow(dead_code)]

pub const PAGE_SIZE: u64 = 4096;
pub const PAGE_MASK: u64 = PAGE_SIZE - 1;

/// User virtual address space is a half-open interval: [BASE, TOP).
pub const USER_SPACE_BASE: u64 = 0x0000_0000_0001_0000;
pub const USER_SPACE_TOP_EXCLUSIVE: u64 = 0x0000_8000_0000_0000;

/// Supervisor-only HHDM window, also half-open: [BASE, END).
pub const HHDM_BASE: u64 = 0xFFFF_8000_0000_0000;
pub const HHDM_END: u64 = 0xFFFF_C000_0000_0000;

/// M1.2 physical-address baseline. Future hardware discovery may derive this
/// from CPUID, but this milestone intentionally freezes a 48-bit baseline.
pub const MAX_PHYS_ADDR_WIDTH: u64 = 48;
pub const MAX_PHYS_ADDR: u64 = (1u64 << MAX_PHYS_ADDR_WIDTH) - 1;
pub const PHYS_ADDR_MASK: u64 = 0x0000_FFFF_FFFF_F000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmmError {
    AddressNotCanonical,
    InvalidPhysicalAddress,
    InvalidPageFlags,
    PrivilegeViolation,
    RangeOutOfBounds,
    HugePageNotSupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtAddr(pub u64);

impl VirtAddr {
    pub const fn new(addr: u64) -> Result<Self, VmmError> {
        let upper = addr >> 48;
        let expected = if ((addr >> 47) & 1) == 0 { 0 } else { 0xFFFF };
        if upper != expected {
            return Err(VmmError::AddressNotCanonical);
        }
        Ok(Self(addr))
    }

    pub const fn is_user(self) -> bool {
        self.0 >= USER_SPACE_BASE && self.0 < USER_SPACE_TOP_EXCLUSIVE
    }

    pub const fn is_hhdm(self) -> bool {
        self.0 >= HHDM_BASE && self.0 < HHDM_END
    }

    pub const fn page_offset(self) -> u64 {
        self.0 & PAGE_MASK
    }

    pub const fn page_base(self) -> u64 {
        self.0 & !PAGE_MASK
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysFrame(pub u64);

impl PhysFrame {
    pub const fn new(addr: u64) -> Result<Self, VmmError> {
        if addr & PAGE_MASK != 0 || addr > (MAX_PHYS_ADDR & !PAGE_MASK) {
            return Err(VmmError::InvalidPhysicalAddress);
        }
        Ok(Self(addr))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MappingFlags {
    pub writable: bool,
    pub user_accessible: bool,
    pub executable: bool,
    pub global: bool,
    pub cache_disable: bool,
}

impl MappingFlags {
    pub const fn user_read_only() -> Self {
        Self {
            writable: false,
            user_accessible: true,
            executable: false,
            global: false,
            cache_disable: false,
        }
    }

    pub const fn user_read_write() -> Self {
        Self {
            writable: true,
            user_accessible: true,
            executable: false,
            global: false,
            cache_disable: false,
        }
    }

    pub const fn user_execute_read() -> Self {
        Self {
            writable: false,
            user_accessible: true,
            executable: true,
            global: false,
            cache_disable: false,
        }
    }

    pub const fn kernel_rw_nx() -> Self {
        Self {
            writable: true,
            user_accessible: false,
            executable: false,
            global: false,
            cache_disable: false,
        }
    }

    pub const fn validate(self) -> Result<(), VmmError> {
        if self.writable && self.executable {
            Err(VmmError::InvalidPageFlags)
        } else {
            Ok(())
        }
    }

    pub fn to_x86_flags(self) -> Result<u64, VmmError> {
        self.validate()?;
        let mut flags = 1u64; // Present
        if self.writable {
            flags |= 1 << 1;
        }
        if self.user_accessible {
            flags |= 1 << 2;
        }
        if self.cache_disable {
            flags |= 1 << 4;
        }
        if self.global {
            flags |= 1 << 8;
        }
        if !self.executable {
            flags |= 1 << 63;
        }
        Ok(flags)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserAccess {
    Read,
    Write,
    Execute,
}

pub const fn validate_user_range(ptr: u64, len: u64) -> Result<(), VmmError> {
    if ptr == 0 || len == 0 || ptr < USER_SPACE_BASE || ptr >= USER_SPACE_TOP_EXCLUSIVE {
        return Err(VmmError::RangeOutOfBounds);
    }

    let end = match ptr.checked_add(len) {
        Some(value) => value,
        None => return Err(VmmError::RangeOutOfBounds),
    };

    if end > USER_SPACE_TOP_EXCLUSIVE {
        return Err(VmmError::RangeOutOfBounds);
    }

    Ok(())
}

/// Validate the destination address before any page-table allocation.
/// User mappings are allowed only inside the exact userspace interval;
/// merely being below HHDM is not sufficient.
pub fn validate_mapping_target(virt: u64, flags: MappingFlags) -> Result<(), VmmError> {
    VirtAddr::new(virt)?;
    flags.validate()?;

    if flags.user_accessible && !(virt >= USER_SPACE_BASE && virt < USER_SPACE_TOP_EXCLUSIVE) {
        return Err(VmmError::PrivilegeViolation);
    }

    Ok(())
}

/// HHDM entries must remain supervisor-only, writable, and NX.
pub const fn validate_hhdm_entry(
    virt: u64,
    writable: bool,
    user: bool,
    nx: bool,
) -> Result<(), VmmError> {
    if virt < HHDM_BASE || virt >= HHDM_END {
        return Err(VmmError::RangeOutOfBounds);
    }
    if !writable || user || !nx {
        return Err(VmmError::PrivilegeViolation);
    }
    Ok(())
}

/// x86_64 bit 7 is PS only at PDPT/PD levels. At PT level it is PAT, and
/// PML4 has no PS bit. Keeping this level-aware prevents accidental rejection
/// or acceptance of entries based on the wrong architectural semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageTableLevel {
    Pml4,
    Pdpt,
    Pd,
    Pt,
}

impl PageTableLevel {
    pub const fn huge_page_bit_is_ps(self) -> bool {
        matches!(self, Self::Pdpt | Self::Pd)
    }

    pub const fn reject_huge_page(self, entry: u64) -> Result<(), VmmError> {
        if self.huge_page_bit_is_ps() && (entry & (1 << 7)) != 0 {
            Err(VmmError::HugePageNotSupported)
        } else {
            Ok(())
        }
    }
}

/// Bounded bootstrap mapping used before the full VMM is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapMapping {
    pub phys_base: u64,
    pub phys_end: u64,
    pub virt_offset: u64,
}

impl BootstrapMapping {
    pub const fn new(phys_base: u64, phys_end: u64, virt_offset: u64) -> Result<Self, VmmError> {
        if phys_base >= phys_end || phys_base & PAGE_MASK != 0 || phys_end & PAGE_MASK != 0 {
            return Err(VmmError::InvalidPhysicalAddress);
        }
        Ok(Self {
            phys_base,
            phys_end,
            virt_offset,
        })
    }

    pub const fn translate(&self, phys: u64, size: u64) -> Result<u64, VmmError> {
        if size == 0 {
            return Err(VmmError::InvalidPhysicalAddress);
        }

        let end = match phys.checked_add(size) {
            Some(value) => value,
            None => return Err(VmmError::InvalidPhysicalAddress),
        };

        if phys < self.phys_base || end > self.phys_end {
            return Err(VmmError::InvalidPhysicalAddress);
        }

        match phys.checked_add(self.virt_offset) {
            Some(value) => Ok(value),
            None => Err(VmmError::InvalidPhysicalAddress),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_bounds_are_half_open() {
        assert!(validate_user_range(USER_SPACE_BASE, 1).is_ok());
        assert!(validate_user_range(USER_SPACE_TOP_EXCLUSIVE - 1, 1).is_ok());
        assert_eq!(
            validate_user_range(USER_SPACE_TOP_EXCLUSIVE, 1),
            Err(VmmError::RangeOutOfBounds)
        );
        assert_eq!(
            validate_user_range(USER_SPACE_TOP_EXCLUSIVE - 1, 2),
            Err(VmmError::RangeOutOfBounds)
        );
        assert_eq!(
            validate_user_range(USER_SPACE_BASE, u64::MAX),
            Err(VmmError::RangeOutOfBounds)
        );
    }

    #[test]
    fn exact_upper_boundary_is_valid_only_when_end_equals_top() {
        assert!(validate_user_range(USER_SPACE_TOP_EXCLUSIVE - 0x1000, 0x1000).is_ok());
        assert_eq!(
            validate_user_range(USER_SPACE_TOP_EXCLUSIVE - 0x1000, 0x1001),
            Err(VmmError::RangeOutOfBounds)
        );
        assert_eq!(
            validate_user_range(USER_SPACE_TOP_EXCLUSIVE, 1),
            Err(VmmError::RangeOutOfBounds)
        );
    }

    #[test]
    fn kernel_high_half_cannot_be_user() {
        assert_eq!(
            validate_mapping_target(HHDM_BASE - 0x1000, MappingFlags::user_read_only()),
            Err(VmmError::PrivilegeViolation)
        );
        assert_eq!(
            validate_mapping_target(HHDM_BASE, MappingFlags::user_read_only()),
            Err(VmmError::PrivilegeViolation)
        );
        assert_eq!(
            validate_mapping_target(USER_SPACE_TOP_EXCLUSIVE, MappingFlags::user_read_only()),
            Err(VmmError::PrivilegeViolation)
        );
    }

    #[test]
    fn user_mapping_inside_exact_interval_is_allowed() {
        assert!(validate_mapping_target(USER_SPACE_BASE, MappingFlags::user_read_only()).is_ok());
        assert!(validate_mapping_target(
            USER_SPACE_TOP_EXCLUSIVE - PAGE_SIZE,
            MappingFlags::user_read_only()
        )
        .is_ok());
    }

    #[test]
    fn wx_is_rejected() {
        let flags = MappingFlags {
            writable: true,
            user_accessible: true,
            executable: true,
            global: false,
            cache_disable: false,
        };
        assert_eq!(flags.to_x86_flags(), Err(VmmError::InvalidPageFlags));
    }

    #[test]
    fn physical_baseline_is_48_bit() {
        assert!(PhysFrame::new(MAX_PHYS_ADDR & !PAGE_MASK).is_ok());
        assert_eq!(
            PhysFrame::new(1 << 48),
            Err(VmmError::InvalidPhysicalAddress)
        );
        assert_eq!(
            PhysFrame::new(0x1001),
            Err(VmmError::InvalidPhysicalAddress)
        );
    }

    #[test]
    fn ps_is_level_specific() {
        assert!(!PageTableLevel::Pml4.huge_page_bit_is_ps());
        assert!(PageTableLevel::Pdpt.huge_page_bit_is_ps());
        assert!(PageTableLevel::Pd.huge_page_bit_is_ps());
        assert!(!PageTableLevel::Pt.huge_page_bit_is_ps());
        assert!(PageTableLevel::Pt.reject_huge_page(1 << 7).is_ok()); // PAT
        assert_eq!(
            PageTableLevel::Pd.reject_huge_page(1 << 7),
            Err(VmmError::HugePageNotSupported)
        );
    }

    #[test]
    fn hhdm_is_supervisor_rw_nx() {
        assert!(validate_hhdm_entry(HHDM_BASE, true, false, true).is_ok());
        assert_eq!(
            validate_hhdm_entry(HHDM_BASE, false, false, true),
            Err(VmmError::PrivilegeViolation)
        );
        assert_eq!(
            validate_hhdm_entry(HHDM_BASE, true, true, true),
            Err(VmmError::PrivilegeViolation)
        );
        assert_eq!(
            validate_hhdm_entry(HHDM_BASE, true, false, false),
            Err(VmmError::PrivilegeViolation)
        );
        assert_eq!(
            validate_hhdm_entry(HHDM_END, true, false, true),
            Err(VmmError::RangeOutOfBounds)
        );
    }

    #[test]
    fn bootstrap_translation_is_contained() {
        let mapping = BootstrapMapping::new(0x1000, 0x5000, HHDM_BASE).unwrap();
        assert_eq!(
            mapping.translate(0x2000, 0x1000).unwrap(),
            HHDM_BASE + 0x2000
        );
        assert!(mapping.translate(0x4000, 0x1000).is_ok());
        assert_eq!(
            mapping.translate(0x4000, 0x1001),
            Err(VmmError::InvalidPhysicalAddress)
        );
        assert_eq!(
            mapping.translate(u64::MAX, 1),
            Err(VmmError::InvalidPhysicalAddress)
        );
    }
}
