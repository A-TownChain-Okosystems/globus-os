# GlobusOS System Control Center — Implementation Map

**Catalog status:** `PROPOSED / NOT VERIFIED`  
**Canonical UI catalog:** `docs/GLOBUSOS_FRONTEND_FUNCTION_CATALOG.json` (114 existing UI function IDs; do not renumber)  
**Cross-cutting contract registry:** [a-townchain-ecosystem Issue #191](https://github.com/A-TownChain-Okosystems/a-townchain-ecosystem/issues/191)  
**Machine-readable map:** `docs/SYSTEM_CONTROL_CENTER_IMPLEMENTATION_MAP.json`

## Purpose

This document maps the Windows-like GlobusOS System Control Center to existing frontend function IDs and to the proposed SYSCTL backend contract families. It is an implementation-planning map, not evidence that any UI route, API adapter, backend function, or end-to-end flow is implemented.

The Windows analogy is limited to discoverability and user workflows. GlobusOS retains its own authority model and canonical service boundaries.

## Layers

1. **Frontend / Control Center** — navigation, search, read-only status views, forms, confirmation and error presentation.
2. **Middleware / OS API adapter** — typed request/response contracts, schema validation, correlation IDs, capability and policy pre-checks, approval flow, timeouts and error normalization.
3. **Canonical OS services** — settings, service lifecycle, diagnostics, devices, network, update, VFS/storage and session management in `globus-os`.
4. **Security authority** — existing identity, capability registry, policy and audit interfaces; never reimplement these in the UI.
5. **Domain adapters** — Aurora AI and A-TownChain / ATC-VM status through their canonical APIs. Do not duplicate model runtime, wallet, node, consensus or VM behavior.
6. **ShivaCore boundary** — privileged kernel operations remain behind canonical capability-checked interfaces; no UI direct kernel access.

Required request path:

`UI → GlobusOS API → Identity → Capability → Policy → Validation → Authoritative service/state → Audit`

## Component and function mapping

The complete machine-readable table in `SYSTEM_CONTROL_CENTER_IMPLEMENTATION_MAP.json` records all 20 SYSCTL families and every function name explicitly enumerated in Issue #191. The issue states 68 candidates, but its 20 rows enumerate 63 names; the five-name discrepancy is recorded as an unresolved inventory gap. Each listed candidate is linked to relevant existing UI IDs where a plausible correspondence exists. The SYSCTL IDs remain owned by Issue #191; this file does not create replacement IDs.

| SYSCTL | Control Center area | Existing UI IDs (candidate mapping) | Canonical owner / boundary |
|---|---|---|---|
| SYSCTL-001 | Overview / system health | `ATC-FUNC-OS-UI-SYSTEM-001`, `ATC-FUNC-OS-UI-ADMIN-096` | GlobusOS runtime/status APIs |
| SYSCTL-002 | Configuration | `ATC-FUNC-OS-UI-ADMIN-094`, `ATC-FUNC-OS-UI-PERSONAL-012`, `ATC-FUNC-OS-UI-SYSTEM-009` | GlobusOS settings service; scoped, validated changes |
| SYSCTL-003 | Users and sessions | `ATC-FUNC-OS-UI-ADMIN-092`, `ATC-FUNC-OS-UI-PERSONAL-011` | Canonical identity/session service |
| SYSCTL-004 | Identity, access and capabilities | `ATC-FUNC-OS-UI-SEC-019`, `ATC-FUNC-OS-UI-SEC-021`, `ATC-FUNC-OS-UI-SEC-022` | Canonical identity/security/capability services |
| SYSCTL-005 | Security posture | `ATC-FUNC-OS-UI-SEC-018`, `ATC-FUNC-OS-UI-SEC-024`, `ATC-FUNC-OS-UI-TRUST-002` | GlobusOS security + verified boot evidence |
| SYSCTL-006 | Services | `ATC-FUNC-OS-UI-MGMT-031`, `ATC-FUNC-OS-UI-ADMIN-093` | GlobusOS service manager; no assumed supervisor completeness |
| SYSCTL-007 | Resources and monitoring | `ATC-FUNC-OS-UI-ADMIN-095`, `ATC-FUNC-OS-UI-ADMIN-096` | Runtime/resource telemetry; distinguish unavailable from zero |
| SYSCTL-008 | Devices and drivers | `ATC-FUNC-OS-UI-SYSTEM-005`, `ATC-FUNC-OS-UI-SYSTEM-006`, `ATC-FUNC-OS-UI-MGMT-033` | GlobusOS device/driver services |
| SYSCTL-009 | Network | `ATC-FUNC-OS-UI-SYSTEM-004`, `ATC-FUNC-OS-UI-ATC-055` | OS network service; chain peer state via A-TownChain APIs |
| SYSCTL-010 | Updates and rollback | `ATC-FUNC-OS-UI-MGMT-034`, `ATC-FUNC-OS-UI-MGMT-035`, `ATC-FUNC-OS-UI-TRUST-008` | OS update service; signature and recovery evidence required |
| SYSCTL-011 | Diagnostics and logs | `ATC-FUNC-OS-UI-MGMT-038`, `ATC-FUNC-OS-UI-DEV-076` | OS diagnostics/logging; secrets must be redacted |
| SYSCTL-012 | Events and audit | `ATC-FUNC-OS-UI-SEC-028`, `ATC-FUNC-OS-UI-SEC-029`, `ATC-FUNC-OS-UI-ADMIN-099`, `ATC-FUNC-OS-UI-TRUST-009` | Canonical append-only/tamper-evident audit source |
| SYSCTL-013 | Storage and mounts | `ATC-FUNC-OS-UI-SYSTEM-007` | GlobusOS VFS/storage APIs |
| SYSCTL-014 | Backup and restore | `ATC-FUNC-OS-UI-MGMT-036`, `ATC-FUNC-OS-UI-MGMT-037` | Canonical backup/restore service; integrity and restore validation required |
| SYSCTL-015 | Installation and platform | `ATC-FUNC-OS-UI-TRUST-001`, `ATC-FUNC-OS-UI-TRUST-003` | Platform/install subsystem; **dedicated install-mode UI coverage is a catalog gap** |
| SYSCTL-016 | Aurora AI | `ATC-FUNC-OS-UI-AI-039`–`ATC-FUNC-OS-UI-AI-053` | Aurora owns model/agent runtime; GlobusOS enforces OS-side capabilities and approvals |
| SYSCTL-017 | A-TownChain | `ATC-FUNC-OS-UI-ATC-054`–`ATC-FUNC-OS-UI-ATC-069` | Canonical A-TownChain node/wallet/protocol APIs |
| SYSCTL-018 | ATC-VM | `ATC-FUNC-OS-UI-ATC-065` | Canonical `a-townchain/components/vm`; status/limits only, no second VM |
| SYSCTL-019 | Notifications | `ATC-FUNC-OS-UI-PERSONAL-015` | GlobusOS notification service |
| SYSCTL-020 | Policy administration | `ATC-FUNC-OS-UI-ADMIN-091`, `ATC-FUNC-OS-UI-AI-050` | Canonical policy authority; AI policy is not OS policy authority |

The mappings above are candidates, not proof of complete semantic equivalence. All enumerated backend function candidates remain `UNRESOLVED`; five candidates claimed by Issue #191 are not individually named and must be identified before the inventory can be considered complete until their input/output/error semantics, caller/adapter, tests and exact-SHA evidence are linked.

## UI and API function responsibilities

### Read paths

- Load overview, health and uptime snapshots with observation timestamp, source and freshness.
- List services, devices, mounts, alerts, audit events and installed/update state.
- Query resource metrics with units, sampling interval and explicit `UNAVAILABLE` / `PERMISSION_DENIED` / `STALE` states.
- Present chain, VM and Aurora status only through their canonical APIs; display unknown status as unknown, never healthy by default.

### Write paths

- Read current version → validate typed change → show diff/impact → check identity/capability/policy → obtain approval if required → apply via authoritative service → verify observed state → write audit event.
- Configuration changes require version/concurrency control and explicit partial-failure behavior. Claim atomicity only where the backend guarantees it.
- Service start/stop, update/rollback, restore, policy changes and destructive actions require explicit authorization and confirmation.
- Wallet signing/submission, consensus changes, VM state changes, kernel/TCB operations and TPM/TEE operations must not be exposed as generic settings mutations.

### Common error contract

Every API adapter must distinguish at least: `INVALID_INPUT`, `UNAUTHENTICATED`, `PERMISSION_DENIED`, `POLICY_DENIED`, `APPROVAL_REQUIRED`, `CONFLICT`, `UNAVAILABLE`, `TIMEOUT`, `INTEGRITY_FAILURE`, `PARTIAL_FAILURE` and `INTERNAL_ERROR`. Errors must be structured and safe for display; secrets, private keys, tokens and raw sensitive payloads must not be logged.

## Implementation sequence

1. Preserve all existing `ATC-FUNC-OS-UI-*` IDs; use this mapping to reconcile rather than create duplicate UI entries.
2. Define typed API schemas and error semantics for read-only overview, settings read, service status, diagnostics and notifications first.
3. Bind each adapter to an existing canonical service symbol; if no source symbol exists, record the backend gap instead of adding a placeholder implementation.
4. Add capability/policy and audit tests for every mutation path, including denied and stale-context cases.
5. Add UI tests for loading, stale data, empty state, denied access, service unavailable, timeout and partial failure.
6. Add integration/E2E tests only after source/API ownership is clear; bind evidence to exact full source SHA and Run → Job → Step → exit code/log/artifact.
7. Keep the PR Draft and release blocked until the complete symbol census, bidirectional reconciliation, schema validation and required CI evidence pass.

## Security and acceptance invariants

- UI controls express intent; they do not grant authority.
- Fail closed if identity, capability, policy, approval or integrity context is absent or ambiguous.
- No secret material in settings views, logs, diagnostics exports or audit metadata.
- Do not report a requested state as an observed state; show both separately.
- Do not report zero resource usage when telemetry is missing.
- Never infer implementation from a route, mock, data model, test name or green workspace CI alone.
- Keep canonical owners: GlobusOS for OS/control center, ShivaCore for kernel boundary, Aurora for AI runtime, A-TownChain for chain/wallet/node and canonical ATC-VM, ecosystem for cross-repo architecture/catalog/evidence.
- Catalog status remains `PROPOSED / NOT VERIFIED` until all function-level gates are satisfied.

## Desktop context menu — GLOBUSOS-CONTEXT-MENU-001

The native context menu is part of the canonical desktop shell tracked by [Issue #78](https://github.com/A-TownChain-Okosystems/globus-os/issues/78), not a new system service or a separate shell implementation. The canonical source audit must begin at `system/graphics/{desktop,input,shell,wm}`.

Eight proposed UI function IDs were added to the existing frontend catalog; all remain `UNASSESSED / INCOMPLETE`:

| Function ID | Responsibility |
|---|---|
| `ATC-FUNC-OS-UI-DESKTOP-005` | Open menu for a validated target |
| `ATC-FUNC-OS-UI-DESKTOP-006` | Resolve target and available actions |
| `ATC-FUNC-OS-UI-DESKTOP-007` | Keyboard/pointer navigation and selection |
| `ATC-FUNC-OS-UI-DESKTOP-008` | Dispatch allowlisted, non-privileged UI actions |
| `ATC-FUNC-OS-UI-DESKTOP-009` | Request a capability-checked system action |
| `ATC-FUNC-OS-UI-DESKTOP-010` | Show authoritative result/error/approval state |
| `ATC-FUNC-OS-UI-DESKTOP-011` | Dismiss menu and restore focus |
| `ATC-FUNC-OS-UI-DESKTOP-012` | Accessible labels, shortcuts and focus order |

Initial targets: desktop background, application icon, file/folder, window title bar, taskbar item, notification/tray item. Support context-menu key / Shift+F10 where available, arrow navigation, Enter/Space, Escape, touch long-press, outside-click dismissal and predictable focus restoration.

The context menu is not an authority boundary. Revalidate the target when dispatching an action. Mutations must use `UI → GlobusOS API → Identity → Capability → Policy → Validation → Authoritative State → Audit`. Do not expose wallet signing/submission, consensus changes, direct kernel/TCB or TPM/TEE operations, or direct ATC-VM state mutation as generic context-menu actions. Show success only after authoritative confirmation.

Acceptance tests must cover stale targets, unavailable actions, denied capabilities, approval-required operations, timeout/partial failure, keyboard/touch access, focus restoration and exact-SHA CI evidence. The new IDs are catalog proposals, not proof of implemented behavior.
