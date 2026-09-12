---
document_id: ATC-DOC-ARC-GLOB-001
title: Repository Architecture Specification
version: 1.0.0
status: active
owner: A-TownChain-Okosystems
created: 2026-09-13
updated: 2026-09-13
standard: ATC-STD-MD-001
---

# Architecture Specification — globus-os

## Übersicht

`globus-os` (Layer L4) ist das Userspace-OS auf ShivaCore. Bootchain (AD-013): UEFI → Limine → ShivaCore → globus-init. Vault-restauriert (07.09.2026, AD-020/026/027). Meile M5 (CLAIMED, Evidence incomplete — SCR-0073).

## Subsysteme

1. **atc-globus-os / globus-init:** Userspace-Init auf ShivaCore mit Initial-Caps.
2. **atc-globus-desktop / atc-globus-shell:** Desktop- und Shell-Schicht.
3. **atc-globus-fs / atc-globus-net:** Dateisystem- und Netzwerk-Services.
4. **atc-globus-registry:** Paket-/Dienst-Registry.
5. **atc-drivers / atc-bootloader:** Treiber- und Bootchain-Komponenten.
6. **atc-linux-edition / atc-windows-edition:** Editions-Varianten.

## Verantwortungsgrenzen

- `atc-shivacore` (L1): Kernel — globus-os ist rein Userspace, keine Kernel-Semantik.
- ShivaCore definiert Caps/Scheduling — globus-os konsumiert sie.

## Registry-Einordnung

| Property | Value |
|---|---|
| Layer | L4 |
| Criticality | C1 |
| Security-Klasse | S4 |
| Maturity | R-Level laut `.atc/repository.yaml` · Statusleiter in `.atc/evidence/evidence.yaml` (SCR-0080) |
| Canonical | globus-os (Userspace-OS) |
| Domäne | domaene laut registry/repositories.yaml |

> Ehrlichkeitsregel: CLAIMED ≠ PASS · IMPLEMENTED ≠ VERIFIED — der verbindliche Implementierungsstand
> liegt ausschließlich in `.atc/evidence/evidence.yaml`, nicht in dieser Spezifikation.
