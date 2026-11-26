---
document_id: ATC-DOC-GLOOS-HW-QUAL-001
title: "GlobusOS Bare-Metal Hardware Qualification Matrix"
version: 1.0.0
status: DRAFT
owner: A-TownChain-Okosystems
baseline_branch: main
baseline_commit: 4c62f28fe28fbd4de6053e99a4622406d1c33626
updated: 2026-11-26
---

# GlobusOS Bare-Metal Hardware Qualification Matrix

## 1. Purpose and release boundary

This matrix defines the reproducible qualification path for booting GlobusOS directly
on x86-64 hardware, without a host operating system. It separates:

1. source and unit-test evidence;
2. virtual firmware/device-model evidence in QEMU;
3. physical-machine boot evidence;
4. per-device driver qualification; and
5. release qualification for a declared hardware compatibility list.

A successful build or QEMU boot is not evidence that a physical PC boots. A successful
physical boot on one machine is not evidence that all PCs, all firmware revisions, or
all devices are supported.

The initial target is x86-64 Intel and AMD PCs. ARM64 and other architectures are
out of scope until a separate native architecture port and qualification plan exist.

## 2. Normative evidence statuses

| Status | Meaning |
|---|---|
| NOT_TESTED | No test has been executed. |
| PLANNED | Test case is defined but not implemented or executed. |
| IMPLEMENTED | Test harness or source implementation exists; no pass is implied. |
| TESTED | Test ran; result and exact test input are recorded. |
| PASS | Acceptance criteria passed on the recorded target. |
| FAIL | Acceptance criteria failed; retain logs and failure signature. |
| BLOCKED | Test cannot run because a prerequisite is missing. |
| RESIDUAL | Known limitation remains and is explicitly not qualified. |
| VERIFIED | Exact-SHA and target-bound evidence reviewed and accepted. |

Do not promote a QEMU result to physical-hardware PASS. Do not promote an individual
device result to whole-platform compatibility. Do not use CI green as a substitute
for boot-log and artifact evidence.

## 3. Existing-first implementation ownership

| Concern | Canonical owner | Qualification rule |
|---|---|---|
| Kernel entry, CPU HAL, paging, traps | `modules/atc-shivacore/kernel/` | No parallel kernel or HAL implementation. |
| Boot image and firmware handoff | `modules/atc-shivacore/boot/` | BIOS and UEFI paths are separate test cases. |
| ACPI/PCI/device inventory contracts | `system/devices/` | Discovery is not device initialization or driver success. |
| CPU profile expansion | Existing PR #91 | Reuse this work; do not create a second CPU profile implementation. |
| PCI/ACPI inventory integration | Existing PR #93 | Reuse this work; do not duplicate discovery or inventory logic. |
| Boot CI | `.github/workflows/shivacore-kernel.yml` | Every required acceptance marker must be an independent fail-closed check. |

PR #91 and PR #93 are open work at the time this matrix is authored. Their contents
must not be represented as merged, tested, or verified until their exact PR heads
have completed the required checks.

## 4. Virtual platform matrix (QEMU)

The workflow currently exercises Intel Skylake and AMD EPYC CPU models in BIOS and
UEFI modes. The expanded CPU profile work is tracked in PR #91. The matrix below is
the target qualification set; entries not yet present in the merged workflow remain
PLANNED.

| ID | CPU profile | Firmware | vCPU | Purpose | Current status |
|---|---|---|---:|---|---|
| QEMU-CPU-001 | Intel Skylake-Client-v1 | BIOS | 1 | Existing baseline boot | IMPLEMENTED; exact-head run required |
| QEMU-CPU-002 | Intel Skylake-Client-v1 | UEFI/OVMF | 1 | Existing UEFI baseline | IMPLEMENTED; exact-head run required |
| QEMU-CPU-003 | AMD EPYC | BIOS | 1 | Existing AMD baseline | IMPLEMENTED; exact-head run required |
| QEMU-CPU-004 | AMD EPYC | UEFI/OVMF | 1 | Existing AMD UEFI baseline | IMPLEMENTED; exact-head run required |
| QEMU-CPU-005 | Intel Haswell | BIOS + UEFI | 1 | Older Intel feature baseline | PLANNED; PR #91 |
| QEMU-CPU-006 | AMD Zen 2 / EPYC Rome | BIOS + UEFI | 1 | Older AMD feature baseline | PLANNED; PR #91 |
| QEMU-CPU-007 | qemu64 conservative baseline | BIOS + UEFI | 1 | Minimum declared x86-64 CPU baseline | PLANNED; PR #91 |
| QEMU-CPU-008 | Explicit AVX2 profile | BIOS + UEFI | 1 | Feature detection only; no AVX execution claim | PLANNED; PR #91 |
| QEMU-SMP-001 | Supported Intel profile | BIOS + UEFI | 2 | AP startup and per-CPU initialization | BLOCKED until AP startup is implemented |
| QEMU-SMP-002 | Supported AMD profile | BIOS + UEFI | 4 | Multi-AP startup and synchronization | BLOCKED until AP startup is implemented |
| QEMU-FW-001 | Supported baseline CPU | UEFI/OVMF | 1 | Secure Boot enabled, signed image and rejection test | BLOCKED; signing/trust chain not qualified |
| QEMU-FW-002 | Supported baseline CPU | UEFI/OVMF + TPM 2.0 device | 1 | Measured boot and event-log validation | BLOCKED; TPM measurement path not qualified |

QEMU acceptance must record the exact QEMU version, command line, CPU model, vCPU
count, firmware binary SHA-256, disk-image SHA-256, source SHA, serial log, process
exit code, and artifact SHA-256. A timeout is acceptable only when the expected
milestones were observed and the process did not panic, triple-fault, or reset.

The current kernel does not establish AP startup merely because SMP data structures
or a multi-vCPU QEMU machine are present. SMP tests remain BLOCKED until AP startup,
per-CPU initialization, interrupt routing, synchronization, and scheduler integration
are implemented and tested.

## 5. Firmware and boot-mode matrix

| ID | Firmware/boot mode | Required acceptance | Status |
|---|---|---|---|
| FW-001 | Legacy BIOS | Boot image loads, kernel entry, CPU HAL, ACPI, memory, Ring-3 and scheduler milestones | IMPLEMENTED in QEMU workflow; exact-head evidence required |
| FW-002 | UEFI without Secure Boot | UEFI image loads and reaches the same kernel acceptance milestones | IMPLEMENTED in QEMU workflow; exact-head evidence required |
| FW-003 | UEFI Secure Boot enabled | Valid signed image boots; unsigned and tampered images are rejected | BLOCKED |
| FW-004 | UEFI with TPM 2.0 | Measurements and event log are validated against expected PCR policy | BLOCKED |
| FW-005 | ACPI variations | RSDP, RSDT/XSDT, MADT, HPET and MCFG parsing reject malformed lengths, checksums and bounds | PARTIAL; adversarial coverage required |
| FW-006 | Firmware without optional tables | Kernel degrades safely or reports an explicit unsupported requirement | PLANNED |
| FW-007 | Firmware update / NVRAM reset | Boot entry recovery and deterministic fallback behavior | PLANNED |

BIOS and UEFI are distinct boot paths. Secure Boot and TPM measured boot are not
implied by ordinary UEFI boot.

## 6. Device and bus qualification matrix

Discovery, resource validation, driver binding, device initialization, data transfer,
error handling, and DMA isolation are separate gates. A device is not qualified by
being listed in an inventory.

| ID | Device class | Minimum test coverage | Current status |
|---|---|---|---|
| DEV-PCI-001 | PCI configuration space | Enumerate bus 0, multifunction devices, bridges and subordinate buses | RESIDUAL; PR #93 adds discovery contracts |
| DEV-PCI-002 | PCI BARs | 32/64-bit BAR sizing, I/O vs memory BARs, alignment, overflow and bounds | RESIDUAL |
| DEV-PCI-003 | PCIe bridges | Nested bridge traversal and invalid topology rejection | PLANNED |
| DEV-IRQ-001 | Local APIC / IOAPIC | Timer and device IRQ delivery, masking, EOI/ACK and routing | PARTIAL; physical routing unverified |
| DEV-DMA-001 | Intel VT-d / AMD-Vi IOMMU | Hardware table programming, invalidation, fault path and isolation negative tests | BLOCKED; software model is not hardware isolation |
| DEV-STOR-001 | NVMe | Controller reset, admin queue, identify, I/O queues, read/write/flush and timeout recovery | RESIDUAL; physical controller qualification absent |
| DEV-STOR-002 | SATA/AHCI | Controller discovery, port reset, identify and block I/O | PLANNED |
| DEV-USB-001 | USB xHCI | Controller init, root hub, keyboard and mass-storage enumeration | PLANNED |
| DEV-NET-001 | Ethernet NIC | PCI match, link, RX/TX, DHCP/static configuration and reset recovery | PLANNED |
| DEV-GPU-001 | Integrated GPU/framebuffer | Firmware framebuffer fallback and safe display initialization | PARTIAL; vendor driver support not qualified |
| DEV-GPU-002 | Discrete GPU | Device-specific driver, memory management, modeset and reset | RESIDUAL |
| DEV-AUDIO-001 | HD Audio | Codec discovery, playback and error recovery | PLANNED |
| DEV-TPM-001 | TPM 2.0 | Device discovery, command transport, PCR read and measured boot validation | BLOCKED |
| DEV-RNG-001 | CPU/platform entropy | Source identification, health checks and fail-closed behavior | RESIDUAL |

No device driver may bypass the canonical device-service authorization boundary.
Raw PCI configuration access, MMIO mapping, interrupt routing, and DMA authority
remain privileged operations. Aurora and ordinary userspace services must not gain
ambient hardware access.

## 7. Physical PC qualification matrix

Physical qualification is performed on named machines with exact component and
firmware identities. The initial matrix is a target list, not a statement that the
machines are already available or tested.

| ID | Vendor/platform target | Firmware variants | Required evidence | Status |
|---|---|---|---|---|
| PHY-INTEL-001 | Intel Haswell desktop/laptop | Legacy BIOS and UEFI where available | Cold boot, warm reboot, ACPI, storage, USB, network | NOT_TESTED |
| PHY-INTEL-002 | Intel Skylake desktop/laptop | UEFI; Secure Boot separately | Same, plus PCIe and power-state observations | NOT_TESTED |
| PHY-INTEL-003 | Modern Intel Core platform | UEFI, Secure Boot on/off | CPU features, AP startup, NVMe, iGPU, USB, NIC | NOT_TESTED |
| PHY-AMD-001 | AMD Zen 2 Ryzen desktop | UEFI | CPU features, AP startup, NVMe, USB, NIC, IOMMU | NOT_TESTED |
| PHY-AMD-002 | AMD EPYC platform | UEFI | Multi-socket/multi-core, NUMA, AP startup, IOMMU, storage | NOT_TESTED |
| PHY-AMD-003 | Modern AMD Ryzen platform | UEFI, Secure Boot on/off | CPU features, AP startup, NVMe, iGPU/discrete GPU, USB, NIC | NOT_TESTED |
| PHY-OEM-001 | OEM laptop platform | Vendor UEFI implementation | Battery, ACPI, keyboard, touchpad, Wi-Fi and suspend/resume | NOT_TESTED |
| PHY-OEM-002 | OEM desktop/workstation | Vendor UEFI implementation | PCIe topology, GPU, NVMe, NIC, TPM and recovery | NOT_TESTED |

Each physical test record must include:

- manufacturer, exact model, motherboard/system product ID and board revision;
- CPU model, stepping, microcode revision and CPUID feature report;
- firmware vendor, version, date, boot mode and Secure Boot state;
- RAM size/type/configuration;
- GPU, NVMe/SATA, NIC, Wi-Fi, USB controller and TPM identifiers;
- source commit SHA and built image SHA-256;
- boot medium and exact boot settings;
- serial/debug log, visible failure evidence and test operator;
- cold-boot and warm-reboot outcomes;
- per-device result, known limitations and reproducibility notes.

Serial numbers, asset tags, MAC addresses and other personal or organization-specific
identifiers must be redacted from public artifacts unless disclosure is explicitly
authorized.

## 8. Common boot acceptance gates

Every BIOS/UEFI QEMU profile and every physical platform must use the applicable
subset of these gates. Each gate is checked independently and fails closed.

| Gate | Acceptance criterion |
|---|---|
| BOOT-001 | Firmware/bootloader transfers control to the expected kernel entry point. |
| CPU-001 | CPU vendor, family/model/stepping and required baseline features are decoded safely. |
| CPU-002 | Unsupported required CPU features produce a clear, controlled rejection. |
| ACPI-001 | Firmware table signatures, lengths, checksums and address bounds are validated. |
| MEM-001 | Physical memory map is validated before allocator use; reserved regions remain reserved. |
| IRQ-001 | GDT/IDT and required interrupt controller paths initialize without unexpected fault. |
| RING3-001 | A known user process enters Ring 3 and returns through the expected syscall/trap path. |
| SCHED-001 | Existing scheduler acceptance proves the required context-switch and register checkpoints. |
| SMP-001 | Every expected AP reaches its per-CPU ready state; no AP is counted from a declared topology alone. |
| PCI-001 | PCI enumeration is bounded and handles bridges, multifunction devices and malformed resources. |
| DMA-001 | DMA is disabled until hardware IOMMU isolation and fault handling pass. |
| STORAGE-001 | Read/write/flush and timeout/error recovery pass on the actual controller. |
| SECURITY-001 | Secure Boot signature acceptance and rejection tests pass when that mode is claimed. |
| TPM-001 | TPM measurements and event log agree with the defined boot policy when measured boot is claimed. |
| RECOVERY-001 | Failed boot and failed update paths recover without silently weakening security policy. |

A test runner must fail when a required marker is absent. It must also fail on kernel
panic, triple fault, unexpected reboot, malformed evidence, or missing artifacts.
Do not treat a long-running QEMU process alone as success.

## 9. Evidence and artifact contract

Every CI and physical run records a machine-readable manifest with at least:

```text
schema=globusos.hardware-evidence.v1
repository=A-TownChain-Okosystems/globus-os
source_sha=<full commit SHA>
workflow_run_id=<GitHub Actions run or physical-run ID>
workflow_job=<job name or physical test session>
test_case_id=<matrix ID>
target_type=QEMU|PHYSICAL
cpu_vendor=<vendor>
cpu_model=<model>
firmware_mode=BIOS|UEFI
firmware_version=<version or QEMU firmware SHA>
vcpus=<count>
device_inventory_sha256=<hash of normalized inventory>
boot_image_sha256=<hash>
serial_log_sha256=<hash>
result=PASS|FAIL|BLOCKED|NOT_TESTED
failure_signature=<stable ID or none>
```

For CI, retain the workflow run, job, step, raw log and uploaded artifact. For
physical testing, retain the signed test record and raw logs in the approved evidence
location. All claims must bind to the exact source SHA and exact test target.

## 10. Release qualification levels

| Level | Meaning | Release claim allowed |
|---|---|---|
| L0 Build | Kernel and boot image build | Buildable for declared target |
| L1 QEMU BIOS | BIOS QEMU acceptance passes | BIOS boot in the tested QEMU configuration |
| L2 QEMU UEFI | UEFI QEMU acceptance passes | UEFI boot in the tested QEMU configuration |
| L3 Physical boot | Named physical machines pass boot gates | Boots on the explicitly listed machine configurations |
| L4 Device qualified | Required device classes pass their physical tests | Listed devices are supported on listed platforms |
| L5 Platform qualified | Full release suite, recovery, security and regression tests pass | Compatibility only for the published Hardware Compatibility List |

There is no universal-PC certification level. The public Hardware Compatibility
List must identify exact tested configurations and unsupported devices. "Runs on
every PC" is prohibited unless a future formal conformance program defines and
proves a bounded compatibility claim.

## 11. Immediate work order

1. Fix the malformed BIOS acceptance block so ACPI and both Ramdisk checks are
   independent mandatory gates.
2. Run the existing BIOS/UEFI Intel Skylake and AMD EPYC matrix on the exact PR head.
3. Integrate PR #91 CPU profiles after its own review and exact-head CI; do not fork
   or duplicate its profile work.
4. Integrate PR #93 PCI/ACPI discovery after its own review and exact-head CI; retain
   separate gates for enumeration, BAR programming, IRQ and DMA.
5. Add multi-vCPU test execution only after AP startup is implemented; until then
   keep SMP qualification BLOCKED rather than manufacturing a pass.
6. Add Secure Boot/TPM test cases only after the native signature-verification and
   measurement path exists.
7. Start physical qualification with one Intel and one AMD reference machine, then
   expand to OEM laptops and workstations.

## 12. Baseline statement

This matrix is a qualification plan, not a completed certification. At authoring time,
the inspected main baseline is `4c62f28fe28fbd4de6053e99a4622406d1c33626`.
Physical Intel/AMD qualification, SMP AP startup, hardware IOMMU isolation,
Secure Boot and TPM measured boot remain unverified or blocked. No compatibility
claim beyond the exact evidence recorded by future test runs is authorized.
