# GlobusOS System Areas & Settings Catalog

**Status:** AUDIT BASELINE  
**Date:** 2026-09-27

This catalog defines the user-facing and administrative system areas/settings that GlobusOS must account for. It does not claim that these settings are implemented.

## System areas

1. System Overview — OS version, build, kernel/runtime state, uptime, health.
2. Display & Graphics — resolution, scaling, refresh rate, compositor, GPU selection, multi-display.
3. Sound — output/input devices, volume, mute, balance, audio services.
4. Network — Ethernet, Wi-Fi, IPv4/IPv6, DNS, routing, proxy, VPN boundary.
5. Bluetooth & Peripherals — pairing, devices, input peripherals.
6. Storage — disks, partitions, mounts, filesystems, encryption boundary, removable media.
7. Applications & Services — installed packages, service state, startup, permissions.
8. Users & Accounts — users, groups, sessions, authentication methods, recovery.
9. Security & Privacy — capabilities, authorization, secure boot state, credentials, audit policy, telemetry/privacy controls.
10. Updates & Recovery — update channel, verification, A/B slot state, rollback, recovery mode.
11. Power — battery/AC state, suspend, shutdown, reboot, thermal/power policy.
12. Time & Region — clock, timezone, NTP/time source, locale, date/time formats.
13. Accessibility — display assistance, input assistance, audio assistance, interaction settings.
14. Appearance — theme, UI scale, fonts, compositor/UI preferences.
15. Input & Keyboard — keyboard layout, pointer, touch, shortcuts, input methods.
16. Privacy & Data — diagnostics, logs, crash reporting, data retention.
17. Developer Options — debug interfaces, developer mode, SDK/tooling access, test endpoints.
18. AI / Aurora — model/provider policy, AI capabilities, tool permissions, approvals, provenance/audit.
19. ATC / Blockchain — network/chain endpoint boundary, wallet integration, ATC-VM/ATCLang tooling boundaries.
20. Backup & Restore — backup policy, restore points, configuration recovery.
21. System Administration — policies, service management, configuration profiles, administrative roles.
22. Diagnostics — health checks, logs, traces, crash reports, evidence bundles.

## Settings contract

Each setting should have:

- stable setting ID: ATC-SETTING-OS-*
- owning subsystem
- data type/schema
- default value
- allowed values/range
- persistence semantics
- read capability
- write capability
- authorization policy
- validation rules
- audit/provenance requirements
- migration/versioning rule
- test vectors
- exact-SHA CI evidence
- E2E evidence where applicable

## Initial setting families

| Family | Examples | Priority |
|---|---|---|
| System | hostname, OS profile, service policy | P0 |
| Security | capability policy, secure-boot policy, audit policy | P0 |
| Identity | users, sessions, authentication policy | P0 |
| Network | interfaces, DNS, routes, proxy | P0 |
| Storage | mounts, filesystem policy, encryption boundary | P0 |
| Power | suspend, shutdown, thermal policy | P1 |
| Display | resolution, scaling, refresh, multi-display | P1 |
| Audio | default input/output, volume, mute | P1 |
| Updates | channel, verification, A/B slot, rollback policy | P0 |
| Privacy | telemetry, diagnostics, retention | P1 |
| Accessibility | UI/input/audio assistance | P1 |
| Appearance | theme, fonts, UI scale | P2 |
| Developer | debug/test access, developer mode | P1 |
| AI | model policy, capability grants, approval policy, audit | P1 |
| ATC | endpoint/network/tooling policy | P1 |
| Backup | backup schedule, retention, restore policy | P1 |
| Diagnostics | logging level, trace collection, evidence bundle policy | P1 |

## Non-negotiable security rule

Settings are not merely key/value storage. Security-sensitive settings are policy-controlled state.

A write path must follow:

request -> identity -> capability -> policy -> schema validation -> state transition -> audit record -> persistence

No administrative UI may bypass the authoritative policy/API boundary.

## Traceability

Every setting must eventually resolve:

SETTING ID -> SPEC -> DOC -> OWNER -> SCHEMA -> READ/WRITE API -> AUTHORIZATION -> IMPLEMENTATION SHA -> TEST -> EXACT-SHA CI -> E2E/AUDIT

Until that chain is proven, the setting remains UNASSESSED.

This document is an audit contract, not an implementation claim.
