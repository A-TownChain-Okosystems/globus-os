![ATC COMPLIANCE](https://img.shields.io/badge/ATC%20COMPLIANCE-R3%20%C2%B7%20ATC--STD--201%2F202%2F203-brightgreen)

# ATC GlobusOS

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

See [`docs/IDENTITY_WALLET_ARCHITECTURE.md`](docs/IDENTITY_WALLET_ARCHITECTURE.md), [`docs/IDENTITY_WALLET_FLOW.md`](docs/IDENTITY_WALLET_FLOW.md), and the normative [`docs/ACCOUNT_WALLET_AURORA_NODE_BINDING.md`](docs/ACCOUNT_WALLET_AURORA_NODE_BINDING.md) contract for account ↔ wallet ↔ Aurora node binding, challenge signing, rotation, privacy, and evidence gates.

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
- `system/identity/` — identity, authentication, wallet-binding and recovery integration contract.

These are the canonical foundations. Hardware-specific implementations and production cryptography remain separate, evidence-driven milestones.

## Standard OS component coverage

GlobusOS tracks the standard operating-system domains as an explicit implementation matrix:

- Boot, UEFI, Secure Boot and TPM
- CPU/SMP, interrupts, timers and memory management
- PCI/PCIe, MMIO, DMA and IOMMU
- NVMe/storage, VFS and filesystems
- Ethernet/network stack
- capabilities, authorization, audit and key management
- identity, authentication and sessions
- init/service management and power management
- graphics, GPU and audio
- packages, updates and recovery
- shell/userland and developer SDK
- observability and diagnostics
- Aurora AI integration outside the kernel TCB
- A-TownChain/ATC-VM/ATCLang integration outside the kernel TCB

See [`docs/OS_STANDARD_COMPONENTS.md`](docs/OS_STANDARD_COMPONENTS.md) for ownership, priority, trust boundaries, hardware dependency chains and readiness criteria.

## Boot path

```text
UEFI / firmware
 → bootloader
 → ShivaCore
 → capability + IPC initialization
 → GlobusOS init/service manager
 → identity / security / devices / storage / network
 → wallet integration boundary
 → runtime / graphics / audio
 → Aurora / desktop / applications
```

## Architecture and governance

See [`docs/SYSTEM_ARCHITECTURE.md`](docs/SYSTEM_ARCHITECTURE.md) for the normative system decomposition and trust boundaries and [`docs/OS_STANDARD_COMPONENTS.md`](docs/OS_STANDARD_COMPONENTS.md) for complete OS component coverage. Development follows `ATC-STD-000` and the applicable organization standards. `APPROVED`, `IMPLEMENTED`, `AUDITED` and `PRODUCTION_READY` remain independent states.

## Ecosystem boundaries

```text
ATCLang → ATC-VM → A-TownChain

Aurora → GlobusOS IPC/API → ShivaCore

GlobusOS Identity → atc-wallet integration → A-TownChain identity
```

The VM remains the boundary between chain execution and the Rust system/network track. GlobusOS does not embed chain semantics into the kernel.

## Security

Security-sensitive issues must not be disclosed through public GitHub Issues. Follow `SECURITY.md` and the organization disclosure process.

## License

Apache License 2.0. See [`LICENSE`](LICENSE).









## Overview
GlobusOS is the operating-system userspace layer built on ShivaCore.

## Purpose
Provides the user-facing OS services, identity, wallet, applications and system integration required by the A-TownChain ecosystem.

## Status
**Status:** `development`  
**Version:** `0.1.0`

## Architecture
Components include the system services, ShivaCore integration modules and SDK. Data flow and dependencies are defined by the workspace manifests and canonical architecture documentation.

## Features
- Identity and wallet integration
- System services and media
- ShivaCore integration
- Application SDK

## Repository Structure
```text
├── system
├── modules
├── sdk
├── scripts
├── docs
└── tests
```

## Requirements
Rust stable and the repository's workspace toolchain are required.

## Installation
```bash
git clone https://github.com/A-TownChain-Okosystems/globus-os.git
cd globus-os
cargo check -p shivacore -p shivacore-service-space
```
`cargo check --workspace` is not usable in this repository for the same reason the
workspace-wide test command is not: the bare-metal crate's `bootloader` dependency has
no host build.

## Configuration
Configuration is defined by the workspace manifests and system-specific configuration files.

## Usage
Build and test the workspace with Cargo commands documented by the repository workflows.

## Development
```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy -p shivacore -p shivacore-service-space --all-targets -- -D warnings
```

The push and pull request workflows run the same gates. They currently cannot start
because the GitHub account's e-mail address is unverified; the commands above are the
local equivalent.

## Testing
```bash
cargo test --workspace --all-targets      # 1687 tests, alle Crates
cargo test -p shivacore --lib             # 698 tests
cargo test -p shivacore-service-space --lib  # 552 tests
cargo test -p libshivacore                # 14 Tests (Syscall-ABI 2.0.0)
```
Expected result as of 2026-11-07: 1687 passed across 30 suites (targets), zero failures, run on
the stable toolchain.

Der Boot-Image-Builder (`modules/atc-shivacore/boot`) ist **kein Workspace-Mitglied**.
Er haengt am `bootloader`-Crate, dessen Build-Skript Nightly verlangt; als Mitglied zog
er frueher den kompletten Workspace in einen Nightly-Build und liess
`cargo test --workspace` auf Stable abbrechen, bevor ein Test lief. Er wird deshalb in
seinem Verzeichnis gebaut und erzeugt daraus die BIOS- und UEFI-Images:

```bash
RUSTFLAGS='-C target-cpu=x86-64 --cfg curve25519_dalek_backend="serial" \
  cargo +nightly build -p shivacore --release --target x86_64-unknown-none --features x86-boot

(cd modules/atc-shivacore/boot && cargo +nightly build --release)
modules/atc-shivacore/boot/target/release/boot \
  target/x86_64-unknown-none/release/shivacore images
```
Expected result: an `x86_64-unknown-none` kernel ELF plus `images/shivacore-bios.img` and
`images/shivacore-uefi.img`. `scripts/qemu_boot.sh` fuehrt dieselben Schritte aus und
bootet beide Images.

## Security

Security-sensitive issues must not be disclosed through public GitHub Issues. Follow `SECURITY.md` and the organization disclosure process.

## License

Apache License 2.0. See [`LICENSE`](LICENSE).









## Overview
GlobusOS is the operating-system userspace layer built on ShivaCore.

## Purpose
Provides the user-facing OS services, identity, wallet, applications and system integration required by the A-TownChain ecosystem.

## Status
**Status:** `development`  
**Version:** `0.1.0`

## Architecture
Components include the system services, ShivaCore integration modules and SDK. Data flow and dependencies are defined by the workspace manifests and canonical architecture documentation.

## Features
- Identity and wallet integration
- System services and media
- ShivaCore integration
- Application SDK

## Repository Structure
```text
├── system
├── modules
├── sdk
├── scripts
├── docs
└── tests
```

## Requirements
Rust stable and the repository's workspace toolchain are required.

## Installation
```bash
git clone https://github.com/A-TownChain-Okosystems/globus-os.git
cd globus-os
cargo check -p shivacore -p shivacore-service-space
```
`cargo check --workspace` is not usable in this repository for the same reason the
workspace-wide test command is not: the bare-metal crate's `bootloader` dependency has
no host build.

## Configuration
Configuration is defined by the workspace manifests and system-specific configuration files.

## Usage
Build and test the workspace with Cargo commands documented by the repository workflows.

## Development
```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy -p shivacore -p shivacore-service-space --all-targets -- -D warnings
```

The push and pull request workflows run the same gates. They currently cannot start
because the GitHub account's e-mail address is unverified; the commands above are the
local equivalent.

## Testing
```bash
cargo test -p shivacore --lib               # 574 tests
cargo test -p shivacore-service-space --lib # 552 tests
```
Expected result: 574 passed in `shivacore` and 552 passed in `shivacore-service-space`,
zero failures, as of 2026-11-06.

`cargo test --workspace --all-targets` is not a usable gate here: the bare-metal crate
pulls in `bootloader`, whose build script does not run for a host test build, so the
command aborts before a single test executes. The kernel image is built separately:

```bash
RUSTFLAGS='-C target-cpu=x86-64 --cfg curve25519_dalek_backend="serial"' \
  cargo +nightly build -p shivacore --release --target x86_64-unknown-none --features x86-boot
```
Expected result: an `x86_64-unknown-none` kernel ELF, about 145 KB. A QEMU boot run is
not part of the current evidence.

## Security
Security issues must not be disclosed publicly; use the repository's official security reporting process and ATC-STD-203.

## Documentation
Canonical documentation is maintained in `docs/` and the repository's governance files.

## Governance
Changes follow ATC governance, evidence and review requirements.

## Standards & Compliance
Applicable standards include ATC-STD-000, ATC-STD-201, ATC-STD-202 and ATC-STD-203.

## Roadmap
See `ROADMAP.md` for the canonical development roadmap.

## Contributing
Contributions must pass the applicable CI and governance gates.

## License
Apache-2.0.

## Maintainers
A-TownChain-Okosystems / ShivaCoreDev.

## Repository Metadata
Canonical repository: `A-TownChain-Okosystems/globus-os`.

## Canonical status and inventories

Use these documents together; they have different purposes:

- [Implementation status and release gates](docs/IMPLEMENTATION_STATUS.md) — current declared state, blockers, acceptance checkpoints, and exact-SHA evidence requirements.
- [Component and function catalog](docs/COMPONENT_FUNCTION_CATALOG.md) — subsystem ownership, responsibility, declared state, and verification boundary.
- [Standard OS component coverage](docs/OS_STANDARD_COMPONENTS.md) — breadth/coverage baseline, not a claim of completed implementation.
- [System architecture](docs/SYSTEM_ARCHITECTURE.md) — normative decomposition and trust boundaries.
- [Roadmap](ROADMAP.md) — milestone ordering and planned work.
- [CI root-cause register PR #52](https://github.com/A-TownChain-Okosystems/globus-os/pull/52) — exact-head failure findings for selected open PRs; the register remains a draft proposal until merged.

**Evidence rule:** implementation, verification, and residual risk are separate states. Never mark a subsystem verified solely because its interface exists or a different commit passed CI.
