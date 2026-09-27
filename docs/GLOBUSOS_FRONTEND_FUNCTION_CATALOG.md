# GlobusOS Frontend Function Catalog (GLOBUSOS-FRONTEND-001)

**Status:** AUDIT BASELINE  
**Date:** 2026-09-27  
**Scope:** Desktop, Control Center and Trust Center user-interface functions

This catalog defines the user-facing function surface that must be independently evidenced. It is **not** an implementation claim.

## Architectural surfaces

- **DESKTOP** — applications, files, windows, launcher and session surface.
- **CONTROL CENTER** — system, personal, security, management, AI/Aurora, A-TownChain, developer, creator and administration controls.
- **TRUST CENTER** — integrity, provenance, audit and verification evidence.

## Security and authority boundary

`UI -> GlobusOS API -> Identity -> Capability -> Policy -> Validation -> Authoritative State -> Audit`

The UI must not directly mutate kernel/TCB state. Security-sensitive settings and actions require explicit authorization and auditable state transitions. Wallet keys, TPM/TEE material and other privileged credentials remain behind their authoritative trust boundaries.

## Evidence rule

Every UI function eventually requires:

`FUNCTION ID -> SPEC -> DOC -> SOURCE SYMBOL -> IMPLEMENTATION SHA -> TEST -> EXACT-SHA CI -> E2E/AUDIT/RELEASE`

A screen, route, mock, placeholder, design, or frontend type does **not** prove the underlying OS capability exists.

## Function catalog

| ID | Surface | Group | Area / Function | Status | Traceability |
|---|---|---|---|---|---|
| ATC-FUNC-OS-UI-SYSTEM-001 | CONTROL_CENTER | SYSTEM | Overview — Open/manage Overview | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SYSTEM-002 | CONTROL_CENTER | SYSTEM | Display & Graphics — Open/manage Display & Graphics | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SYSTEM-003 | CONTROL_CENTER | SYSTEM | Sound — Open/manage Sound | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SYSTEM-004 | CONTROL_CENTER | SYSTEM | Network — Open/manage Network | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SYSTEM-005 | CONTROL_CENTER | SYSTEM | Bluetooth — Open/manage Bluetooth | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SYSTEM-006 | CONTROL_CENTER | SYSTEM | Devices — Open/manage Devices | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SYSTEM-007 | CONTROL_CENTER | SYSTEM | Storage — Open/manage Storage | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SYSTEM-008 | CONTROL_CENTER | SYSTEM | Power — Open/manage Power | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SYSTEM-009 | CONTROL_CENTER | SYSTEM | Time & Region — Open/manage Time & Region | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SYSTEM-010 | CONTROL_CENTER | SYSTEM | Sensors — Open/manage Sensors | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-PERSONAL-011 | CONTROL_CENTER | PERSONAL | Accounts — Open/manage Accounts | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-PERSONAL-012 | CONTROL_CENTER | PERSONAL | Appearance — Open/manage Appearance | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-PERSONAL-013 | CONTROL_CENTER | PERSONAL | Accessibility — Open/manage Accessibility | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-PERSONAL-014 | CONTROL_CENTER | PERSONAL | Input & Keyboard — Open/manage Input & Keyboard | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-PERSONAL-015 | CONTROL_CENTER | PERSONAL | Notifications — Open/manage Notifications | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-PERSONAL-016 | CONTROL_CENTER | PERSONAL | Privacy — Open/manage Privacy | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-PERSONAL-017 | CONTROL_CENTER | PERSONAL | Language & Region — Open/manage Language & Region | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SEC-018 | CONTROL_CENTER | SECURITY | Security Center — Open/manage Security Center | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SEC-019 | CONTROL_CENTER | SECURITY | Identity — Open/manage Identity | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SEC-020 | CONTROL_CENTER | SECURITY | Authentication — Open/manage Authentication | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SEC-021 | CONTROL_CENTER | SECURITY | Permissions — Open/manage Permissions | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SEC-022 | CONTROL_CENTER | SECURITY | Capabilities — Open/manage Capabilities | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SEC-023 | CONTROL_CENTER | SECURITY | Credentials / Keys — Open/manage Credentials / Keys | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SEC-024 | CONTROL_CENTER | SECURITY | Secure Boot — Open/manage Secure Boot | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SEC-025 | CONTROL_CENTER | SECURITY | TPM / TEE — Open/manage TPM / TEE | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SEC-026 | CONTROL_CENTER | SECURITY | Firewall — Open/manage Firewall | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SEC-027 | CONTROL_CENTER | SECURITY | Encryption — Open/manage Encryption | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SEC-028 | CONTROL_CENTER | SECURITY | Audit — Open/manage Audit | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-SEC-029 | CONTROL_CENTER | SECURITY | Security Events — Open/manage Security Events | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-MGMT-030 | CONTROL_CENTER | SYSTEM-MANAGEMENT | Applications — Open/manage Applications | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-MGMT-031 | CONTROL_CENTER | SYSTEM-MANAGEMENT | Services — Open/manage Services | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-MGMT-032 | CONTROL_CENTER | SYSTEM-MANAGEMENT | Packages — Open/manage Packages | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-MGMT-033 | CONTROL_CENTER | SYSTEM-MANAGEMENT | Drivers — Open/manage Drivers | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-MGMT-034 | CONTROL_CENTER | SYSTEM-MANAGEMENT | Updates — Open/manage Updates | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-MGMT-035 | CONTROL_CENTER | SYSTEM-MANAGEMENT | Recovery — Open/manage Recovery | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-MGMT-036 | CONTROL_CENTER | SYSTEM-MANAGEMENT | Backup — Open/manage Backup | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-MGMT-037 | CONTROL_CENTER | SYSTEM-MANAGEMENT | Restore — Open/manage Restore | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-MGMT-038 | CONTROL_CENTER | SYSTEM-MANAGEMENT | Diagnostics — Open/manage Diagnostics | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-AI-039 | CONTROL_CENTER | AI-AURORA | Aurora Overview — Open/manage Aurora Overview | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-AI-040 | CONTROL_CENTER | AI-AURORA | Models — Open/manage Models | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-AI-041 | CONTROL_CENTER | AI-AURORA | Providers — Open/manage Providers | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-AI-042 | CONTROL_CENTER | AI-AURORA | Conversations — Open/manage Conversations | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-AI-043 | CONTROL_CENTER | AI-AURORA | Memory — Open/manage Memory | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-AI-044 | CONTROL_CENTER | AI-AURORA | RAG / Knowledge — Open/manage RAG / Knowledge | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-AI-045 | CONTROL_CENTER | AI-AURORA | Agents — Open/manage Agents | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-AI-046 | CONTROL_CENTER | AI-AURORA | Tools — Open/manage Tools | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-AI-047 | CONTROL_CENTER | AI-AURORA | Capabilities — Open/manage Capabilities | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-AI-048 | CONTROL_CENTER | AI-AURORA | Permissions — Open/manage Permissions | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-AI-049 | CONTROL_CENTER | AI-AURORA | Approvals — Open/manage Approvals | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-AI-050 | CONTROL_CENTER | AI-AURORA | AI Policies — Open/manage AI Policies | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-AI-051 | CONTROL_CENTER | AI-AURORA | AI Security — Open/manage AI Security | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-AI-052 | CONTROL_CENTER | AI-AURORA | Provenance — Open/manage Provenance | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-AI-053 | CONTROL_CENTER | AI-AURORA | AI Audit — Open/manage AI Audit | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-054 | CONTROL_CENTER | A-TOWNCHAIN | Blockchain Overview — Open/manage Blockchain Overview | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-055 | CONTROL_CENTER | A-TOWNCHAIN | Network — Open/manage Network | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-056 | CONTROL_CENTER | A-TOWNCHAIN | Wallet — Open/manage Wallet | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-057 | CONTROL_CENTER | A-TOWNCHAIN | Accounts — Open/manage Accounts | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-058 | CONTROL_CENTER | A-TOWNCHAIN | Transactions — Open/manage Transactions | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-059 | CONTROL_CENTER | A-TOWNCHAIN | Node — Open/manage Node | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-060 | CONTROL_CENTER | A-TOWNCHAIN | Mempool — Open/manage Mempool | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-061 | CONTROL_CENTER | A-TOWNCHAIN | Consensus — Open/manage Consensus | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-062 | CONTROL_CENTER | A-TOWNCHAIN | Staking — Open/manage Staking | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-063 | CONTROL_CENTER | A-TOWNCHAIN | Mining — Open/manage Mining | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-064 | CONTROL_CENTER | A-TOWNCHAIN | Smart Contracts — Open/manage Smart Contracts | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-065 | CONTROL_CENTER | A-TOWNCHAIN | ATC-VM — Open/manage ATC-VM | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-066 | CONTROL_CENTER | A-TOWNCHAIN | ATCLang — Open/manage ATCLang | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-067 | CONTROL_CENTER | A-TOWNCHAIN | Explorer — Open/manage Explorer | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-068 | CONTROL_CENTER | A-TOWNCHAIN | Economics — Open/manage Economics | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ATC-069 | CONTROL_CENTER | A-TOWNCHAIN | Blockchain Security — Open/manage Blockchain Security | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-DEV-070 | CONTROL_CENTER | DEVELOPER | Developer Mode — Open/manage Developer Mode | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-DEV-071 | CONTROL_CENTER | DEVELOPER | SDK — Open/manage SDK | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-DEV-072 | CONTROL_CENTER | DEVELOPER | IDE — Open/manage IDE | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-DEV-073 | CONTROL_CENTER | DEVELOPER | Terminal — Open/manage Terminal | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-DEV-074 | CONTROL_CENTER | DEVELOPER | Debugging — Open/manage Debugging | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-DEV-075 | CONTROL_CENTER | DEVELOPER | Profiling — Open/manage Profiling | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-DEV-076 | CONTROL_CENTER | DEVELOPER | Logs — Open/manage Logs | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-DEV-077 | CONTROL_CENTER | DEVELOPER | APIs — Open/manage APIs | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-DEV-078 | CONTROL_CENTER | DEVELOPER | AI Development — Open/manage AI Development | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-DEV-079 | CONTROL_CENTER | DEVELOPER | Blockchain Development — Open/manage Blockchain Development | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-GENESIS-080 | CONTROL_CENTER | CREATOR-GENESIS | Genesis Engine — Open/manage Genesis Engine | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-GENESIS-081 | CONTROL_CENTER | CREATOR-GENESIS | Projects — Open/manage Projects | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-GENESIS-082 | CONTROL_CENTER | CREATOR-GENESIS | Worlds — Open/manage Worlds | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-GENESIS-083 | CONTROL_CENTER | CREATOR-GENESIS | Characters — Open/manage Characters | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-GENESIS-084 | CONTROL_CENTER | CREATOR-GENESIS | Creatures — Open/manage Creatures | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-GENESIS-085 | CONTROL_CENTER | CREATOR-GENESIS | Quests — Open/manage Quests | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-GENESIS-086 | CONTROL_CENTER | CREATOR-GENESIS | Dialogue — Open/manage Dialogue | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-GENESIS-087 | CONTROL_CENTER | CREATOR-GENESIS | Mods — Open/manage Mods | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-GENESIS-088 | CONTROL_CENTER | CREATOR-GENESIS | Assets — Open/manage Assets | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-GENESIS-089 | CONTROL_CENTER | CREATOR-GENESIS | Scripts — Open/manage Scripts | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-GENESIS-090 | CONTROL_CENTER | CREATOR-GENESIS | Publishing — Open/manage Publishing | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ADMIN-091 | CONTROL_CENTER | ADMINISTRATION | System Policies — Open/manage System Policies | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ADMIN-092 | CONTROL_CENTER | ADMINISTRATION | Users & Roles — Open/manage Users & Roles | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ADMIN-093 | CONTROL_CENTER | ADMINISTRATION | Services — Open/manage Services | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ADMIN-094 | CONTROL_CENTER | ADMINISTRATION | Configuration — Open/manage Configuration | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ADMIN-095 | CONTROL_CENTER | ADMINISTRATION | Resource Management — Open/manage Resource Management | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ADMIN-096 | CONTROL_CENTER | ADMINISTRATION | Monitoring — Open/manage Monitoring | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ADMIN-097 | CONTROL_CENTER | ADMINISTRATION | Compliance — Open/manage Compliance | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ADMIN-098 | CONTROL_CENTER | ADMINISTRATION | Evidence — Open/manage Evidence | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-ADMIN-099 | CONTROL_CENTER | ADMINISTRATION | System Audit — Open/manage System Audit | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-DESKTOP-001 | DESKTOP | DESKTOP | Desktop Shell — Desktop shell/session surface | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-DESKTOP-002 | DESKTOP | DESKTOP | Windows — Window management surface | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-DESKTOP-003 | DESKTOP | DESKTOP | Launcher — Application launcher surface | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-DESKTOP-004 | DESKTOP | DESKTOP | Files — File management surface | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-TRUST-001 | TRUST_CENTER | TRUST_CENTER | System Integrity — System integrity evidence surface | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-TRUST-002 | TRUST_CENTER | TRUST_CENTER | Secure Boot — Secure boot evidence surface | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-TRUST-003 | TRUST_CENTER | TRUST_CENTER | Kernel Status — Kernel/TCB status evidence surface | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-TRUST-004 | TRUST_CENTER | TRUST_CENTER | Capability Audit — Capability audit evidence surface | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-TRUST-005 | TRUST_CENTER | TRUST_CENTER | AI Audit — AI audit evidence surface | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-TRUST-006 | TRUST_CENTER | TRUST_CENTER | Blockchain Audit — Blockchain audit evidence surface | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-TRUST-007 | TRUST_CENTER | TRUST_CENTER | Software Provenance — Software provenance evidence surface | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-TRUST-008 | TRUST_CENTER | TRUST_CENTER | Update Provenance — Update provenance evidence surface | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-TRUST-009 | TRUST_CENTER | TRUST_CENTER | Security Events — Security event evidence surface | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-TRUST-010 | TRUST_CENTER | TRUST_CENTER | Evidence Bundles — Evidence bundle inspection surface | UNASSESSED | INCOMPLETE |
| ATC-FUNC-OS-UI-TRUST-011 | TRUST_CENTER | TRUST_CENTER | Verification Status — Verification status surface | UNASSESSED | INCOMPLETE |

## Required function record

Each function should eventually bind:
- stable function ID
- surface and navigation group
- requirement/specification and documentation
- owning service/component and authoritative API/interface
- source symbol/path and implementation SHA
- tests and exact-source-SHA CI evidence
- E2E/security/audit/provenance evidence where required
- release evidence where required
- current status and traceability state

## Non-claims

- This catalog does not claim that the listed UI exists.
- Existing settings, graphics, or service foundations do not prove a complete frontend.
- A backend service does not automatically prove that a secure UI write path exists.
- Historical CI does not prove the current source state.
- AI and A-TownChain controls remain subordinate to their respective capability/policy boundaries.
