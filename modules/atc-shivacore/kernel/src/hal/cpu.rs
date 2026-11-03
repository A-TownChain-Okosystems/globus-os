// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! x86-64 CPU identification and conservative, CPUID-based feature gating.
//!
//! Hardware support and OS-enabled state are deliberately separate. AVX-family
//! instructions are usable only after XSAVE/OSXSAVE and the relevant XCR0 state
//! components have been confirmed.
use core::arch::x86_64::{__cpuid, __cpuid_count};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuVendor {
    Intel,
    Amd,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuFeatures {
    pub sse2: bool,
    pub sse41: bool,
    pub nx: bool,
    pub apic: bool,
    pub x2apic: bool,
    pub syscall: bool,
    pub one_gib_pages: bool,
    pub invariant_tsc: bool,
    pub smep: bool,
    pub smap: bool,
    pub xsave: bool,
    pub osxsave: bool,
    pub avx_hardware: bool,
    pub avx_usable: bool,
    pub avx2_hardware: bool,
    pub avx2_usable: bool,
    pub avx512f_hardware: bool,
    pub avx512f_usable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuInfo {
    pub vendor: CpuVendor,
    pub family: u8,
    pub model: u8,
    pub stepping: u8,
    pub max_basic_leaf: u32,
    pub max_extended_leaf: u32,
    /// Read only when CPUID reports OSXSAVE; otherwise remains None.
    pub xcr0: Option<u64>,
    pub features: CpuFeatures,
}

impl CpuInfo {
    pub fn detect() -> Self {
        let basic = __cpuid(0);
        let max_basic_leaf = basic.eax;
        let vendor = vendor_from_regs(basic.ebx, basic.ecx, basic.edx);
        let ext = __cpuid(0x8000_0000);
        let max_extended_leaf = ext.eax;

        let leaf1 = if max_basic_leaf >= 1 {
            __cpuid(1)
        } else {
            zero_cpuid()
        };
        let stepping = (leaf1.eax & 0xF) as u8;
        let base_family = ((leaf1.eax >> 8) & 0xF) as u8;
        let base_model = ((leaf1.eax >> 4) & 0xF) as u8;
        let ext_family = ((leaf1.eax >> 20) & 0xFF) as u8;
        let ext_model = ((leaf1.eax >> 16) & 0xF) as u8;
        let family = if base_family == 0xF {
            base_family.saturating_add(ext_family)
        } else {
            base_family
        };
        let model = if base_family == 0x6 || base_family == 0xF {
            base_model | (ext_model << 4)
        } else {
            base_model
        };

        let leaf7 = if max_basic_leaf >= 7 {
            __cpuid_count(7, 0)
        } else {
            zero_cpuid()
        };
        let ext1 = if max_extended_leaf >= 0x8000_0001 {
            __cpuid(0x8000_0001)
        } else {
            zero_cpuid()
        };
        let ext7 = if max_extended_leaf >= 0x8000_0007 {
            __cpuid(0x8000_0007)
        } else {
            zero_cpuid()
        };

        let osxsave = (leaf1.ecx & (1 << 27)) != 0;
        // XGETBV is legal only after OSXSAVE is set by the operating system.
        // Otherwise do not execute XGETBV and fail AVX gating closed.
        let xcr0 = if osxsave { Some(read_xcr0()) } else { None };

        Self {
            vendor,
            family,
            model,
            stepping,
            max_basic_leaf,
            max_extended_leaf,
            xcr0,
            features: features_from_leaves(leaf1, leaf7, ext1, ext7, xcr0),
        }
    }

    pub const fn vendor_name(&self) -> &'static str {
        match self.vendor {
            CpuVendor::Intel => "Intel",
            CpuVendor::Amd => "AMD",
            CpuVendor::Other => "Other",
        }
    }

    pub const fn boot_compatible(&self) -> bool {
        self.features.sse2 && self.features.nx && self.features.apic
    }
}

/// Read XCR0. The caller must first establish CPUID.1:ECX.OSXSAVE.
#[inline]
fn read_xcr0() -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: called only when CPUID reports OSXSAVE, which means CR4.OSXSAVE
    // is enabled and XGETBV(0) is architecturally valid.
    unsafe {
        core::arch::asm!(
            "xgetbv",
            in("ecx") 0_u32,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((high as u64) << 32) | low as u64
}

fn features_from_leaves(
    leaf1: core::arch::x86_64::CpuidResult,
    leaf7: core::arch::x86_64::CpuidResult,
    ext1: core::arch::x86_64::CpuidResult,
    ext7: core::arch::x86_64::CpuidResult,
    xcr0: Option<u64>,
) -> CpuFeatures {
    let xsave = (leaf1.ecx & (1 << 26)) != 0;
    let osxsave = (leaf1.ecx & (1 << 27)) != 0;
    let avx_hardware = (leaf1.ecx & (1 << 28)) != 0;
    let avx2_hardware = (leaf7.ebx & (1 << 5)) != 0;
    let avx512f_hardware = (leaf7.ebx & (1 << 16)) != 0;
    // XCR0[1:2] enables XMM and YMM state. AVX-512 also requires opmask,
    // ZMM_hi256 and hi16_ZMM state (bits 5, 6 and 7).
    let avx_state_enabled = xcr0.is_some_and(|value| value & 0x6 == 0x6);
    let avx512_state_enabled = xcr0.is_some_and(|value| value & 0xE6 == 0xE6);
    let avx_usable = xsave && osxsave && avx_hardware && avx_state_enabled;

    CpuFeatures {
        sse2: (leaf1.edx & (1 << 26)) != 0,
        sse41: (leaf1.ecx & (1 << 19)) != 0,
        apic: (leaf1.edx & (1 << 9)) != 0,
        x2apic: (leaf1.ecx & (1 << 21)) != 0,
        nx: (ext1.edx & (1 << 20)) != 0,
        syscall: (ext1.edx & (1 << 11)) != 0,
        one_gib_pages: (ext1.edx & (1 << 26)) != 0,
        invariant_tsc: (ext7.edx & (1 << 8)) != 0,
        smep: (leaf7.ebx & (1 << 7)) != 0,
        smap: (leaf7.ebx & (1 << 20)) != 0,
        xsave,
        osxsave,
        avx_hardware,
        avx_usable,
        avx2_hardware,
        avx2_usable: avx_usable && avx2_hardware,
        avx512f_hardware,
        avx512f_usable: avx_usable && avx512f_hardware && avx512_state_enabled,
    }
}

fn zero_cpuid() -> core::arch::x86_64::CpuidResult {
    core::arch::x86_64::CpuidResult {
        eax: 0,
        ebx: 0,
        ecx: 0,
        edx: 0,
    }
}

fn vendor_from_regs(ebx: u32, ecx: u32, edx: u32) -> CpuVendor {
    let bytes = [ebx.to_le_bytes(), edx.to_le_bytes(), ecx.to_le_bytes()];
    if bytes == [*b"Genu", *b"ineI", *b"ntel"] {
        CpuVendor::Intel
    } else if bytes == [*b"Auth", *b"enti", *b"cAMD"] {
        CpuVendor::Amd
    } else {
        CpuVendor::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feature_leaves(
        leaf1_ecx: u32,
        leaf7_ebx: u32,
    ) -> (
        core::arch::x86_64::CpuidResult,
        core::arch::x86_64::CpuidResult,
        core::arch::x86_64::CpuidResult,
        core::arch::x86_64::CpuidResult,
    ) {
        (
            core::arch::x86_64::CpuidResult {
                eax: 0,
                ebx: 0,
                ecx: leaf1_ecx,
                edx: (1 << 26) | (1 << 9),
            },
            core::arch::x86_64::CpuidResult {
                eax: 0,
                ebx: leaf7_ebx,
                ecx: 0,
                edx: 0,
            },
            core::arch::x86_64::CpuidResult {
                eax: 0,
                ebx: 0,
                ecx: 0,
                edx: (1 << 20) | (1 << 11) | (1 << 26),
            },
            core::arch::x86_64::CpuidResult {
                eax: 0,
                ebx: 0,
                ecx: 0,
                edx: 1 << 8,
            },
        )
    }

    #[test]
    fn detects_current_cpu_without_panicking() {
        let info = CpuInfo::detect();
        assert!(info.max_basic_leaf >= 1);
        assert!(info.max_extended_leaf >= 0x8000_0000);
    }

    #[test]
    fn vendor_parser_accepts_intel_and_amd() {
        assert_eq!(
            vendor_from_regs(0x756e6547, 0x6c65746e, 0x49656e69),
            CpuVendor::Intel
        );
        assert_eq!(
            vendor_from_regs(0x68747541, 0x444d4163, 0x69746e65),
            CpuVendor::Amd
        );
    }

    #[test]
    fn absent_feature_leaves_decode_as_unsupported() {
        let features =
            features_from_leaves(zero_cpuid(), zero_cpuid(), zero_cpuid(), zero_cpuid(), None);
        assert_eq!(
            features,
            CpuFeatures {
                sse2: false,
                sse41: false,
                nx: false,
                apic: false,
                x2apic: false,
                syscall: false,
                one_gib_pages: false,
                invariant_tsc: false,
                smep: false,
                smap: false,
                xsave: false,
                osxsave: false,
                avx_hardware: false,
                avx_usable: false,
                avx2_hardware: false,
                avx2_usable: false,
                avx512f_hardware: false,
                avx512f_usable: false,
            }
        );
    }

    #[test]
    fn baseline_feature_bits_are_decoded_from_their_defined_leaves() {
        let (leaf1, leaf7, ext1, ext7) = feature_leaves(1 << 21, 0);
        let features = features_from_leaves(leaf1, leaf7, ext1, ext7, None);
        assert!(features.sse2 && features.apic && features.x2apic);
        assert!(features.nx && features.syscall && features.one_gib_pages);
        assert!(features.invariant_tsc);
        assert!(!features.avx_usable);
    }

    #[test]
    fn avx_hardware_without_osxsave_or_xcr0_is_not_usable() {
        let (leaf1, leaf7, ext1, ext7) = feature_leaves((1 << 28) | (1 << 26), 1 << 5);
        let features = features_from_leaves(leaf1, leaf7, ext1, ext7, None);
        assert!(features.avx_hardware && features.avx2_hardware);
        assert!(!features.avx_usable && !features.avx2_usable);
    }

    #[test]
    fn avx2_requires_xmm_and_ymm_state_in_xcr0() {
        let ecx = (1 << 28) | (1 << 27) | (1 << 26);
        let (leaf1, leaf7, ext1, ext7) = feature_leaves(ecx, 1 << 5);
        let features = features_from_leaves(leaf1, leaf7, ext1, ext7, Some(0x2));
        assert!(!features.avx_usable && !features.avx2_usable);
        let features = features_from_leaves(leaf1, leaf7, ext1, ext7, Some(0x6));
        assert!(features.avx_usable && features.avx2_usable);
    }

    #[test]
    fn avx512_requires_all_extended_zmm_state_components() {
        let ecx = (1 << 28) | (1 << 27) | (1 << 26);
        let (leaf1, leaf7, ext1, ext7) = feature_leaves(ecx, 1 << 16);
        let features = features_from_leaves(leaf1, leaf7, ext1, ext7, Some(0x6));
        assert!(features.avx_usable);
        assert!(!features.avx512f_usable);
        let features = features_from_leaves(leaf1, leaf7, ext1, ext7, Some(0xE6));
        assert!(features.avx512f_hardware && features.avx512f_usable);
    }

    #[test]
    fn smep_and_smap_are_decoded_from_structured_leaf_seven() {
        let (leaf1, leaf7, ext1, ext7) = feature_leaves(0, (1 << 7) | (1 << 20));
        let features = features_from_leaves(leaf1, leaf7, ext1, ext7, None);
        assert!(features.smep && features.smap);
    }
}
