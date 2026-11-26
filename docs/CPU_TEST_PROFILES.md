---
document_id: ATC-DOC-SHIVACORE-CPU-PROFILES-001
title: ShivaCore CPU Test Profiles
version: 0.1.0
status: draft
owner: A-TownChain-Okosystems
updated: 2026-10-18
---

# ShivaCore CPU Test Profiles

## Purpose and evidence boundary

These profiles define reproducible virtual CPU configurations for the x86-64 kernel CI. A QEMU profile is not a physical-hardware qualification and must not be reported as proof that a matching real CPU was tested. The QEMU model exposes a CPUID feature set; the kernel must use the features it actually observes, not infer them from a profile name.

The workflow currently builds the same conservative baseline kernel with -C target-cpu=x86-64. Do not compile the kernel globally for AVX, AVX2, AVX-512, a vendor, or a CPU generation. Optional SIMD paths must be gated at runtime and retain a baseline fallback.

## CI profile matrix

| Profile ID | QEMU CPU model | Purpose | Evidence class |
|---|---|---|---|
| intel-skylake | Skylake-Client-v1 | Existing Intel compatibility baseline | QEMU virtual CPU |
| amd-epyc | EPYC | Existing AMD server baseline; not a specific EPYC generation | QEMU virtual CPU |
| intel-haswell | Haswell | Older Intel generation and AVX2-capable hardware path | QEMU virtual CPU |
| amd-zen2 | EPYC-Rome | AMD Zen 2 / EPYC Rome feature exposure | QEMU virtual CPU |
| qemu-x86-64-baseline | qemu64 | Conservative legacy feature set and fallback paths | QEMU virtual CPU |
| qemu-x86-64-avx2 | Haswell,+avx2 | Explicit AVX2 feature-exposure test target | QEMU virtual CPU |

The Haswell and explicit-AVX2 profiles are intentionally distinct test cases: one is generation-oriented, the other validates a requested ISA feature override. The actual observed CPUID flags and runtime usability gates remain authoritative.

## Runtime feature contract

The CPU HAL reports both hardware capability and whether the OS has enabled the architectural state required to use it.

- Base boot gate: SSE2, NX, and local APIC.
- Security and platform inventory: NX, SMEP, SMAP, APIC, x2APIC, syscall, 1-GiB pages, and invariant TSC are reported independently.
- AVX hardware: CPUID leaf 1 ECX.AVX, XSAVE, and OSXSAVE are tracked separately.
- AVX usability: requires AVX hardware support, XSAVE, OSXSAVE, and XCR0 XMM/YMM bits (1 and 2).
- AVX2 usability: requires AVX usability plus CPUID leaf 7 EBX.AVX2.
- AVX-512F usability: requires AVX usability, CPUID leaf 7 EBX.AVX512F, and XCR0 bits 1, 2, 5, 6, and 7.
- If OSXSAVE is not set, XGETBV is not executed and AVX-family usability is false.
- Feature presence alone does not authorize use of an instruction path unless the OS-state gate is satisfied.

This inventory does not initialize XSAVE, modify CR4, configure XCR0, or implement SIMD context switching. Those remain prerequisites before any AVX-family instructions are executed by kernel code. Until those paths exist and are tested, report AVX/AVX2/AVX-512 as detected capabilities only, not as enabled kernel execution features.

## Required tests

1. CPUID leaf bounds: unavailable basic and extended leaves decode to unsupported values.
2. Feature decoding: SSE2, NX, APIC, x2APIC, invariant TSC, SMEP and SMAP bits come from the documented leaves.
3. AVX fail-closed behavior: AVX hardware without OSXSAVE/XCR0 is not usable.
4. AVX2 state: XCR0 must enable both XMM and YMM state.
5. AVX-512 state: XCR0 must enable XMM, YMM, opmask, ZMM_hi256 and hi16_ZMM.
6. Boot smoke tests: verify boot, CPU identification, scheduler/interrupt path, Ring-3 transition and IPC markers on each matrix entry.
7. Record exact commit SHA, profile ID, QEMU version, requested CPU model, observed vendor/signature/features, workflow/job result, and log artifact.

## Planned follow-up profiles

These are targets, not currently tested or supported profiles: Intel Broadwell, Ice Lake-SP, Sapphire Rapids, Alder Lake and Raptor Lake; AMD Zen 3, Zen 4 and Zen 5; resource-constrained x86-64; and KVM-backed virtual machines. Add a profile only after checking that the CI-installed QEMU exposes the exact CPU model and documenting the feature contract. Hybrid scheduling and KVM-specific validation require dedicated tests and must not be inferred from a CPU name alone.

## Physical hardware status

All profiles in the current CI matrix are QEMU virtual CPU profiles. Real Haswell, EPYC Rome, Skylake, Zen-family, hybrid Intel, or other physical systems remain unvalidated unless a separate hardware-run record ties the exact model, firmware, kernel SHA, test command, and logs together.
