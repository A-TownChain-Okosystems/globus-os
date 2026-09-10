# globus-os [L4]

Globus OS — Userspace-OS auf ShivaCore (AD-013 Bootchain: UEFI -> Limine -> ShivaCore -> globus-init).

**Vault-Restauration (07.09.2026, AD-020/026/027):** Inhalt aus dem Wiki-Vault
(docs/archive/monorepo-full/) restauriert — vor der Repo-Leerung byte-identisch gesichert. Keine — Vault-Stand konsistent.

**Module:** atc-globus-os, atc-globus-desktop, atc-globus-fs, atc-globus-net, atc-globus-registry, atc-globus-shell, atc-drivers, atc-bootloader, atc-linux-edition, atc-windows-edition

**Meile (AD-027):** M5 CLAIMED — Evidence incomplete (Bootchain/globus-init reproduzierbar nachweisen, SCR-0073): globus-init bootet Userspace-Services auf ShivaCore mit Initial-Caps

**Hinweis:** Basis fuer den Rebuild; Gate-Kriterien laut LAUFFAEHIGKEITS_ROADMAP
(a-townchain-os-docs/docs/roadmap/).

---

## ATC Compliance & Governance (ATC-STD-201 / 202 / 203)

**ATC COMPLIANCE: R3** — auditiert am 2026-09-07 (atc-repo-audit; R-Level aus `.atc/repository.yaml`).
Architekturentscheidungen: zentral im [DECISIONS_REGISTER](https://github.com/A-TownChain-Okosystems/a-townchain-os-docs/blob/main/docs/DECISIONS_REGISTER.md) (AD-Nummern verbindlich; lokale Entscheidungen in `docs/decisions/`).

- **Purpose:** Globus OS — das Userspace-OS auf ShivaCore (L4).
- **Scope:** Layer L4, Domain os — globus-os als OS in der 23-Repo-Landschaft (AD-024/026).
- **Architecture:** Boot: UEFI→Limine→ShivaCore→globus-init→Globus OS; Service-Space statt Monolith (AD-012/028).
- **Features:** 10 Module (globus-*: shell, fs, net, registry, desktop, bootloader, drivers, editions).
- **Installation:** Modul-Build je Sprache (rust); Integration via Monorepo-Workspace (a-townchain-os, sync_modules.py).
- **Development:** Conventional Commits; Governance-Regeln aus atc-standards; Naming gemaess ATC-STD-000 §7.
- **Testing:** cargo-Tests je Modul; Boot-Chain M5.
- **Security:** SECURITY.md; S-Klasse S4; ATC-STD-203 Release-Gates; Emergency-Prozess ATC-STD-000 §32.
- **Roadmap:** Einordnung in die Lauffaehigkeits-Roadmap M1-M8 (AD-027) und Bauhierarchie L0-L7 (AD-026).
- **Version:** CHANGELOG.md; SemVer; Releases als ATC-REL-X.Y.Z.
- **License:** Apache-2.0 — Apache-2.0, Michael Wroblewski / ShivaCore / A-TownChain-Okosystems (ATC-LIC/ATS-LIC).
