# GlobusOS — Capability-Microkernel-Betriebssystem

> **Produkt-Repo des A-TownChain-Ökosystems** · [Monorepo](https://github.com/A-TownChain-Okosystems/a-townchain-os) · [Docs-Hub](https://github.com/A-TownChain-Okosystems/a-townchain-os-docs) · Mainnet: **15.09.2026**

Betriebssystem des Ökosystems auf Basis des ShivaCore-Kernels (AD-012/AD-013 verbindlich): Kernel nur Primitive (Scheduler, Memory, IPC, Capability Security, HAL, Syscall ABI); Filesystem, Network, GPU, AI, Blockchain, ATCLang im Service Space. Boot Chain: UEFI→Limine→ShivaCore→globus-init→Globus OS.

## Module (aus Monorepo `src/modules/` überführt)

| Modul | Dateien | Zeilen |
|---|---|---|
| `atc-shivacore` | 74 | 53,392 |
| `atc-kernel` | 85 | 20,127 |
| `atc-globus-shell` | 21 | 6,575 |
| `atc-globus-desktop` | 15 | 543 |
| `atc-globus-fs` | 14 | 490 |
| `atc-globus-net` | 14 | 480 |
| `atc-globus-registry` | 14 | 475 |
| `atc-globus-os` | 28 | 971 |
| `atc-bootloader` | 20 | 515 |
| `atc-drivers` | 27 | 4,492 |
| `atc-linux-edition` | 16 | 542 |
| `atc-windows-edition` | 16 | 536 |
| `atc-mobile` | 19 | 1,188 |
| `atc-shivacore-tools` | 15 | 640 |
| **Total** | **378** | **90,966** |

## Richtlinien

- Architektur-Vorgaben: AD-012/AD-013 (ShivaCore Microkernel, Gate v1.1) — siehe Docs-Hub `docs/architecture/`
- Neue Produkt-Entwicklung läuft hier; Integration & Deployment über das Monorepo
- Standards: ATC-01…35 · ATS-1000…1007 · Lizenz: All Rights Reserved (Michael Wroblewski / ShivaCore / A-TownChain-Okosystems)

*Eingerichtet am 06.09.2026 durch Agent Aurora (Base44) im Auftrag des Owners.*
