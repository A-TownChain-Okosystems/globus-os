# GlobusOS OS Function Catalog

**Status:** AUDIT BASELINE  
**Scope:** Canonical GlobusOS userspace/platform functions plus ShivaCore-owned prerequisites  
**Date:** 2026-09-27

This catalog defines the functions that must be independently evidenced. It is **not** an implementation claim.

## Status model

Each function must be tracked independently:

`SPECIFIED -> DOCUMENTED -> IMPLEMENTED -> TESTED -> CI-VERIFIED -> E2E-VERIFIED -> AUDITED -> RELEASE-READY`

A source file, type, module, or directory alone does not prove implementation or verification.

## Evidence contract

Every function record must eventually bind:

- canonical function ID
- owning layer/component
- requirement/specification reference
- authoritative source symbol/path
- implementation commit SHA
- test(s)
- exact-source-SHA CI evidence
- E2E evidence where required
- security/audit evidence where required
- release evidence where required
- current status and traceability state

## Function catalog

| ID | Domain | Function | Owner boundary | Priority | Required evidence |
|---|---|---|---|---|---|
| ATC-FUNC-OS-BOOT-001 | Boot | Firmware/UEFI handoff | ShivaCore/boot | P0 | source + boot test |
| ATC-FUNC-OS-BOOT-002 | Boot | Bootloader image validation | ShivaCore/boot | P0 | source + negative tests |
| ATC-FUNC-OS-BOOT-003 | Boot | Secure/measured boot state | ShivaCore/boot/security | P0 | implementation + hardware/emulator evidence |
| ATC-FUNC-OS-BOOT-004 | Boot | Boot failure/recovery state | GlobusOS recovery | P0 | state tests + recovery E2E |
| ATC-FUNC-OS-CPU-001 | CPU | CPU/SMP initialization | ShivaCore/HAL | P0 | source + target/emulator tests |
| ATC-FUNC-OS-CPU-002 | CPU | Interrupt/exception dispatch | ShivaCore | P0 | source + kernel tests |
| ATC-FUNC-OS-CPU-003 | CPU | Timer/clock source | ShivaCore/HAL | P0 | source + deterministic tests |
| ATC-FUNC-OS-MEM-001 | Memory | Physical memory management | ShivaCore/memory | P0 | source + allocator tests |
| ATC-FUNC-OS-MEM-002 | Memory | Virtual address-space management | ShivaCore/globus-memory | P0 | source + isolation tests |
| ATC-FUNC-OS-MEM-003 | Memory | Page-table/protection policy | ShivaCore | P0 | source + negative tests |
| ATC-FUNC-OS-MEM-004 | Memory | Memory exhaustion handling | Kernel/system service | P0 | failure-path tests |
| ATC-FUNC-OS-PROC-001 | Process | Process creation/termination | ShivaCore/globus-process | P0 | source + lifecycle tests |
| ATC-FUNC-OS-PROC-002 | Process | Thread creation/termination | ShivaCore/globus-process | P0 | source + lifecycle tests |
| ATC-FUNC-OS-PROC-003 | Process | Scheduling | ShivaCore | P0 | kernel tests + target evidence |
| ATC-FUNC-OS-PROC-004 | Process | Synchronization primitives | ShivaCore | P0 | concurrency tests |
| ATC-FUNC-OS-IPC-001 | IPC | Endpoint creation/destruction | ShivaCore/globus-ipc | P0 | source + tests |
| ATC-FUNC-OS-IPC-002 | IPC | Message send/receive | ShivaCore/globus-ipc | P0 | source + positive/negative tests |
| ATC-FUNC-OS-IPC-003 | IPC | Shared-memory transfer | ShivaCore/globus-ipc | P0 | source + isolation tests |
| ATC-FUNC-OS-IPC-004 | IPC | Capability transfer | ShivaCore/globus-ipc | P0 | security tests |
| ATC-FUNC-OS-CAP-001 | Security | Capability issuance | ShivaCore/globus-security | P0 | source + policy tests |
| ATC-FUNC-OS-CAP-002 | Security | Capability enforcement | ShivaCore/globus-security | P0 | deny-by-default tests |
| ATC-FUNC-OS-CAP-003 | Security | Authorization decision | globus-security | P0 | source + negative vectors |
| ATC-FUNC-OS-IDENT-001 | Identity | User identity creation | globus-identity | P0 | source + tests |
| ATC-FUNC-OS-IDENT-002 | Identity | Authentication | globus-identity | P0 | source + negative tests |
| ATC-FUNC-OS-IDENT-003 | Identity | Session lifecycle | globus-identity | P0 | source + lifecycle tests |
| ATC-FUNC-OS-IDENT-004 | Identity | Credential/recovery boundary | globus-identity | P0 | security tests + audit |
| ATC-FUNC-OS-DEVICE-001 | Devices | PCI/PCIe discovery | HAL/device services | P0 | hardware/emulator evidence |
| ATC-FUNC-OS-DEVICE-002 | Devices | MMIO mapping | HAL/device services | P0 | isolation tests |
| ATC-FUNC-OS-DEVICE-003 | Devices | DMA/IOMMU domain setup | HAL/device services | P0 | hardware/emulator evidence |
| ATC-FUNC-OS-DEVICE-004 | Devices | Driver/service isolation | globus-devices | P0 | capability + failure tests |
| ATC-FUNC-OS-STORAGE-001 | Storage | VFS namespace | globus-vfs | P0 | source + tests |
| ATC-FUNC-OS-STORAGE-002 | Storage | Mount/unmount lifecycle | globus-vfs | P0 | lifecycle tests |
| ATC-FUNC-OS-STORAGE-003 | Storage | Block-device boundary | storage/device services | P0 | source + integration tests |
| ATC-FUNC-OS-STORAGE-004 | Storage | Filesystem boundary | storage/vfs | P0 | implementation + filesystem tests |
| ATC-FUNC-OS-STORAGE-005 | Storage | NVMe queue/DMA path | device/storage | P0 | hardware/emulator E2E |
| ATC-FUNC-OS-NET-001 | Network | NIC discovery/configuration | globus-net/devices | P0 | hardware/emulator evidence |
| ATC-FUNC-OS-NET-002 | Network | Ethernet RX/TX | globus-net/device service | P0 | integration/E2E |
| ATC-FUNC-OS-NET-003 | Network | IPv4/IPv6 | globus-net | P0 | protocol tests |
| ATC-FUNC-OS-NET-004 | Network | TCP/UDP sockets | globus-net | P0 | socket tests |
| ATC-FUNC-OS-NET-005 | Network | DNS/routing | globus-net | P0 | integration tests |
| ATC-FUNC-OS-SVC-001 | Services | Init/service startup | globus-services | P0 | lifecycle tests |
| ATC-FUNC-OS-SVC-002 | Services | Dependency ordering | globus-services | P0 | deterministic ordering tests |
| ATC-FUNC-OS-SVC-003 | Services | Health/failure supervision | globus-services | P0 | failure/restart tests |
| ATC-FUNC-OS-PKG-001 | Package | Package metadata parsing | globus-package | P1 | parser tests |
| ATC-FUNC-OS-PKG-002 | Package | Package signature/integrity verification | globus-package | P1 | positive/negative security vectors |
| ATC-FUNC-OS-PKG-003 | Package | Dependency resolution | globus-package | P1 | resolver tests |
| ATC-FUNC-OS-UPD-001 | Update | A/B update state machine | globus-update | P1 | state/property tests |
| ATC-FUNC-OS-UPD-002 | Update | Verified activation | globus-update | P1 | negative verification tests |
| ATC-FUNC-OS-UPD-003 | Update | Rollback | globus-update | P1 | failure/recovery E2E |
| ATC-FUNC-OS-REC-001 | Recovery | Recovery mode | GlobusOS recovery | P1 | recovery E2E |
| ATC-FUNC-OS-REC-002 | Recovery | Diagnostic evidence collection | diagnostics | P1 | artifact tests |
| ATC-FUNC-OS-POWER-001 | Power | Shutdown/reboot | system services | P1 | target/emulator tests |
| ATC-FUNC-OS-POWER-002 | Power | Suspend/resume | system services | P1 | target/emulator evidence |
| ATC-FUNC-OS-POWER-003 | Power | Thermal/power policy | system services | P1 | target evidence |
| ATC-FUNC-OS-GFX-001 | Graphics | Display/device discovery | globus-graphics | P1 | hardware/emulator evidence |
| ATC-FUNC-OS-GFX-002 | Graphics | Compositor/window surface | globus-graphics | P1 | integration tests |
| ATC-FUNC-OS-GFX-003 | Graphics | GPU boundary | globus-graphics | P1 | hardware backend evidence |
| ATC-FUNC-OS-AUDIO-001 | Audio | Audio service lifecycle | globus-audio | P1 | service tests |
| ATC-FUNC-OS-AUDIO-002 | Audio | Playback/record boundary | globus-audio | P1 | integration/hardware evidence |
| ATC-FUNC-OS-USER-001 | Userland | Shell/CLI | userspace | P1 | command tests |
| ATC-FUNC-OS-USER-002 | Userland | System administration API | userspace/services | P1 | authorization + integration tests |
| ATC-FUNC-OS-OBS-001 | Observability | Structured logging | diagnostics | P1 | schema + redaction tests |
| ATC-FUNC-OS-OBS-002 | Observability | Metrics/tracing | diagnostics | P1 | integration tests |
| ATC-FUNC-OS-OBS-003 | Observability | Crash evidence | diagnostics | P1 | failure-path tests |
| ATC-FUNC-OS-SDK-001 | SDK | Public API/ABI contract | SDK | P1 | compatibility tests |
| ATC-FUNC-OS-SDK-002 | SDK | Developer toolchain | SDK/tooling | P1 | build/test evidence |
| ATC-FUNC-OS-SDK-003 | SDK | Debug/test interface | SDK/tooling | P1 | integration tests |
| ATC-FUNC-OS-AI-001 | AI Platform | Aurora IPC/API boundary | GlobusOS/Aurora | P1 | capability + integration tests |
| ATC-FUNC-OS-AI-002 | AI Platform | AI capability/policy boundary | GlobusOS/Aurora | P1 | deny-by-default tests |
| ATC-FUNC-OS-AI-003 | AI Platform | AI audit/provenance boundary | GlobusOS/Aurora | P1 | audit/evidence tests |
| ATC-FUNC-OS-WALLET-001 | Wallet | Wallet service boundary | globus-wallet-service | P1 | interface + security tests |
| ATC-FUNC-OS-WALLET-002 | Wallet | Identity-wallet binding | identity/wallet | P1 | cryptographic integration tests |
| ATC-FUNC-OS-WALLET-003 | Wallet | Recovery flow | identity/wallet | P1 | negative/security E2E |
| ATC-FUNC-OS-CHAIN-001 | Chain Boundary | ATC tooling boundary | GlobusOS platform | P1 | interface tests |
| ATC-FUNC-OS-CHAIN-002 | Chain Boundary | ATC-VM isolation boundary | GlobusOS platform | P1 | isolation/integration tests |
| ATC-FUNC-OS-CHAIN-003 | Chain Boundary | ATCLang tooling boundary | GlobusOS platform | P1 | interface tests |
| ATC-FUNC-OS-VIRT-001 | Virtualization | VM/container isolation | optional platform | P2 | isolation tests |
| ATC-FUNC-OS-VIRT-002 | Virtualization | Virtual device boundary | optional platform | P2 | integration tests |
| ATC-FUNC-OS-ADMIN-001 | Administration | OS configuration | system services | P1 | authorization + persistence tests |
| ATC-FUNC-OS-ADMIN-002 | Administration | User/session administration | identity/services | P1 | authorization + lifecycle tests |

## Audit rule

For every ID above, the audit must resolve:

`FUNCTION ID -> SPEC -> DOC -> SOURCE SYMBOL -> IMPLEMENTATION SHA -> TEST -> EXACT-SHA CI -> E2E/AUDIT/RELEASE`

If any required edge is missing, the function is **not proven complete**.

### Explicit non-claims

- This catalog does not declare any function IMPLEMENTED merely because a corresponding crate/path exists.
- Hardware-facing contracts do not prove working hardware drivers.
- Passing unit/component tests does not prove system-wide readiness.
- Historical CI does not prove the current default branch.
- Integration into GlobusOS does not move Aurora or blockchain semantics into the ShivaCore TCB.
