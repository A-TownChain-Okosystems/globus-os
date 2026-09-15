# GlobusOS System Architecture v1

## 1. Authority model

GlobusOS is userspace above ShivaCore. ShivaCore remains the kernel/TCB and owns capabilities, address spaces, scheduling, IPC primitives, interrupts and low-level resource control. GlobusOS does not place AI, blockchain or desktop policy into the kernel.

```text
Applications
    |
Aurora AI / Desktop / Shell
    |
GlobusOS service layer
    |-- service manager
    |-- security / identity
    |-- process / runtime
    |-- VFS / storage
    |-- network
    |-- device services
    |-- graphics / audio
    |-- package / update / recovery
    |
IPC + capabilities
    |
ShivaCore
    |
HAL / firmware
    |
Hardware
```

## 2. Repository implementation

The `system/` workspace is the canonical GlobusOS userspace foundation. Each crate is intentionally narrow so services can be isolated behind IPC boundaries.

| Crate | Responsibility |
|---|---|
| `globus-system-core` | System lifecycle and kernel-facing identity |
| `globus-ipc` | Message/endpoint model |
| `globus-security` | Explicit capability authorization |
| `globus-process` | Process/thread lifecycle |
| `globus-memory` | Address-space and page policy |
| `globus-vfs` | VFS namespace and mounts |
| `globus-net` | Network policy/socket boundary |
| `globus-devices` | Device/driver service registry |
| `globus-services` | Service lifecycle/dependency order |
| `globus-graphics` | Display/compositor/GPU boundary |
| `globus-audio` | Audio service boundary |
| `globus-package` | Signed package metadata verification |
| `globus-update` | Atomic A/B update and rollback state |
| `globus-runtime` | Integrated userspace runtime |

## 3. Boot contract

```text
UEFI/firmware
  -> bootloader
  -> ShivaCore
  -> capability/IPC initialization
  -> GlobusOS init/service manager
  -> security + devices + storage + network
  -> runtime + graphics + audio
  -> Aurora / desktop / applications
```

A service is not trusted merely because it starts successfully. Its authority must be represented by explicit capabilities and policy.

## 4. Security invariants

1. No ambient authority.
2. Deny by default.
3. AI services are outside the ShivaCore TCB.
4. Blockchain services are outside the ShivaCore TCB.
5. Drivers are isolated services where the platform permits it.
6. Package installation requires metadata/integrity/signature evidence.
7. Updates must support verification and rollback.
8. A component test does not imply system-wide production readiness.

## 5. Ecosystem boundaries

```text
ATCLang -> ATC-VM -> A-TownChain
                 ^
                 |
        explicit GlobusOS API

Aurora -> GlobusOS IPC/API -> ShivaCore
```

The VM is the boundary between chain execution and the Rust system/network track. GlobusOS may host ATC tooling but does not make chain semantics a kernel responsibility.
