# GlobusOS

> AI-native operating system and userspace platform built on the ShivaCore kernel.

**Project:** `globus-os`  
**Organization:** `A-TownChain-Okosystems`  
**Status:** `development`  
**License:** `Apache-2.0` (see repository license)

## Overview

GlobusOS is the operating-system layer of the A-TownChain ecosystem. It provides userspace services, system integration, drivers, filesystem/networking components, desktop functionality and platform editions on top of **ShivaCore**.

The security boundary is explicit:

```text
Hardware / Boot
      │
      ▼
ShivaCore
(kernel / TCB / capabilities)
      │
      ▼
GlobusOS
(userspace OS and services)
      │
      ├── Aurora AI integration
      ├── system services
      ├── drivers / filesystem / networking
      └── desktop / shell
```

GlobusOS is not the kernel and does not replace ShivaCore. AI functionality is integrated through defined OS interfaces; AI components do not become part of the ShivaCore TCB merely by integration.

## Status

`development` means active development and integration work. Historical milestone claims and audit results are not interpreted as current production readiness.

No Mainnet, production, or release-readiness claim is made by this README. `APPROVED`, `IMPLEMENTED`, `AUDITED`, and `PRODUCTION_READY` are independent states.

## Architecture

The repository contains the GlobusOS userspace/platform components, including:

- `atc-globus-os` — core userspace OS integration.
- `atc-globus-desktop` — desktop environment components.
- `atc-globus-fs` — filesystem services.
- `atc-globus-net` — networking services.
- `atc-globus-registry` — userspace registry services.
- `atc-globus-shell` — shell and command interface.
- `atc-drivers` — driver integration.
- `atc-bootloader` — boot integration components.
- `atc-linux-edition` — Linux-oriented platform edition.
- `atc-windows-edition` — Windows-oriented platform edition.

The canonical boot path where implemented is:

```text
UEFI → bootloader → ShivaCore → globus-init → GlobusOS userspace services
```

## Requirements

Requirements are component-specific. Rust tooling is required for Rust components; the exact supported toolchain is defined by the repository workspace and module manifests.

## Development

Development follows `ATC-STD-000` and applicable repository standards. Changes crossing the ShivaCore/userspace security boundary require the appropriate architecture and security review.

## Testing

Run the test suites defined by the individual workspace components. A successful component test does not by itself establish system-wide audit or production readiness.

## Security

Security-sensitive issues must not be disclosed through public GitHub Issues. Follow `SECURITY.md` and the organization's approved security-disclosure process.

## Governance

The repository is governed by the A-TownChain standards system. Canonical standards use the family-scoped form:

```text
ATC-STD-F{family}-{sequence}
```

Legacy standard IDs remain historical identifiers during migration and must not be silently renumbered, reused, or reclassified.

## Compliance terminology

- **APPROVED** — governance approval exists.
- **IMPLEMENTED** — the referenced implementation exists.
- **AUDITED** — the relevant audit has been performed and recorded.
- **PRODUCTION_READY** — all required release gates have passed.

These states are not interchangeable.

## Documentation

See the repository documentation, architecture specification, status and roadmap files where present. Ecosystem-wide governance and standards are maintained in the corresponding organization repositories.

## License

Apache License 2.0. See `LICENSE`.

## Repository Metadata

<!-- atc metadata block -->
<!--
atc:
  standard: ATC-STD-README-001
repository:
  name: globus-os
  type: operating-system
  status: development
ownership:
  organization: A-TownChain-Okosystems
architecture:
  kernel: atc-shivacore
  role: userspace-os
-->
