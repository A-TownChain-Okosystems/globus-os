# GlobusOS

> AI-native operating system and userspace platform built on the ShivaCore kernel.

**Project:** `globus-os`  
**Organization:** `A-TownChain-Okosystems`  
**Status:** `development`  
**Version:** `0.1.0`  
**License:** `Apache-2.0`

## System role

GlobusOS is the complete operating-system userspace/platform layer above **ShivaCore**. ShivaCore remains the reusable kernel/TCB; GlobusOS provides the system services, device integration, storage, networking, graphics, audio, package lifecycle and recovery required to turn the kernel into an operating system.

```text
Applications
    ↓
Aurora AI / Desktop / Shell
    ↓
GlobusOS system services
    ↓
IPC + capabilities
    ↓
ShivaCore kernel / TCB
    ↓
HAL / firmware / hardware
```

AI and blockchain components are not promoted into the ShivaCore TCB by integration.

## Implemented system foundation

The repository now contains a Rust workspace under `system/` with explicit subsystem boundaries:

- `globus-system-core` — lifecycle and kernel-facing identity.
- `globus-ipc` — endpoints and IPC messages.
- `globus-security` — deny-by-default capability authorization.
- `globus-process` — process/thread lifecycle.
- `globus-memory` — address-space and page policy.
- `globus-vfs` — filesystem namespace and mount contract.
- `globus-net` — network policy/socket boundary.
- `globus-devices` — isolated device/driver registry.
- `globus-services` — service lifecycle and dependency ordering.
- `globus-graphics` — display/compositor/GPU boundary.
- `globus-audio` — audio service boundary.
- `globus-package` — signed package metadata verification.
- `globus-update` — atomic A/B update and rollback state machine.
- `globus-runtime` — integrated userspace runtime.

These are the canonical foundations. Hardware-specific implementations and production cryptography remain separate, evidence-driven milestones.

## Boot path

```text
UEFI / firmware
 → bootloader
 → ShivaCore
 → capability + IPC initialization
 → GlobusOS init/service manager
 → security / devices / storage / network
 → runtime / graphics / audio
 → Aurora / desktop / applications
```

## Development

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

CI runs the same validation gates on pushes and pull requests.

## Architecture and governance

See [`docs/SYSTEM_ARCHITECTURE.md`](docs/SYSTEM_ARCHITECTURE.md) for the normative system decomposition and trust boundaries. Development follows `ATC-STD-000` and the applicable organization standards. `APPROVED`, `IMPLEMENTED`, `AUDITED` and `PRODUCTION_READY` remain independent states.

## Ecosystem boundaries

```text
ATCLang → ATC-VM → A-TownChain

Aurora → GlobusOS IPC/API → ShivaCore
```

The VM remains the boundary between chain execution and the Rust system/network track. GlobusOS does not embed chain semantics into the kernel.

## Security

Security-sensitive issues must not be disclosed through public GitHub Issues. Follow `SECURITY.md` and the organization disclosure process.

## License

Apache License 2.0. See [`LICENSE`](LICENSE).
