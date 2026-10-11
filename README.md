![ATC COMPLIANCE](https://img.shields.io/badge/ATC%20COMPLIANCE-R3%20%C2%B7%20ATC--STD--201%2F202%2F203-brightgreen)

# GlobusOS

> AI-native operating system and userspace platform built on the ShivaCore kernel.

**Project:** `globus-os`  
**Organization:** `A-TownChain-Okosystems`  
**Status:** `development`  
**Version:** `0.1.0`  
**License:** `Apache-2.0`

## System role

GlobusOS is the complete operating-system userspace/platform layer above **ShivaCore**. ShivaCore remains the reusable kernel/TCB; GlobusOS provides the system services, device integration, storage, networking, graphics, audio, package lifecycle, identity, wallet integration and recovery required to turn the kernel into an operating system.

The complete standard OS component coverage and implementation priorities are defined in [`docs/OS_STANDARD_COMPONENTS.md`](docs/OS_STANDARD_COMPONENTS.md). That document is a coverage baseline, not a claim that every listed component is already production-ready.

The formal Aurora AI 1.0 architecture, domain boundaries, agent contracts, Fabric/offline-first rules, risk/approval model, integration boundaries and staged acceptance criteria are defined in [`docs/AURORA_AI_ARCHITECTURE_1.0.md`](docs/AURORA_AI_ARCHITECTURE_1.0.md).

```text
Applications
    ↓
Aurora AI / Desktop / Shell
    ↓
GlobusOS system services
    ├─ Identity / Authentication
    ├─ Wallet Service boundary
    ├─ Storage / VFS
    ├─ Network / Devices
    └─ Package / Update / Recovery
    ↓
IPC + capabilities
    ↓
ShivaCore kernel / TCB
    ↓
HAL / firmware / hardware
```

AI and blockchain components are not promoted into the ShivaCore TCB by integration.

## Identity & wallet

GlobusOS uses a native **Boot → Login/Register → Identity → Wallet → Desktop** flow. Creating a new user profile provisions an A-TownChain-compatible wallet identity through the dedicated wallet integration. The public wallet address is bound to the GlobusOS User ID through a cryptographically verifiable identity binding.

The wallet's 24-word recovery phrase is **recovery material, not the daily login credential**. Private keys and recovery material remain behind protected local boundaries and are never exposed to ordinary applications, Aurora AI, telemetry, logs, GitHub, or remote services.

See [`docs/IDENTITY_WALLET_ARCHITECTURE.md`](docs/IDENTITY_WALLET_ARCHITECTURE.md) and [`docs/IDENTITY_WALLET_FLOW.md`](docs/IDENTITY_WALLET_FLOW.md).

## Implemented system foundation

The repository now contains a Rust workspace under `system/` with explicit subsystem boundaries:

- `globus-system-core` — lifecycle and kernel-facing identity.
- `globus-ipc` — endpoints and IPC messages.
- `globus-security` — deny-by-default capability authorization.
- `globus-process` — process/thread lifecycle.
- `globus-memory` — address-space and page policy.
- `globus-vfs` — filesystem interfaces.
- `globus-net` — networking abstractions.
- `globus-runtime` — integrated userspace runtime.
- `aurora-core` — dependency-free Aurora control-plane contracts and state model.

See the documentation directory for subsystem specifications, implementation status and verification evidence.
