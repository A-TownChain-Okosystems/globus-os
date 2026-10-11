# GlobusOS Roadmap

## Foundation — in implementation

- [x] Rust system workspace
- [x] IPC boundary
- [x] capability authorization boundary
- [x] process/thread model
- [x] memory/address-space policy
- [x] VFS contract
- [x] network policy boundary
- [x] device service registry
- [x] service lifecycle/dependency model
- [x] graphics/audio boundaries
- [x] package verification model
- [x] A/B update and rollback state machine
- [x] integrated runtime status
- [x] CI compile/test/format/Clippy gates

## P1 — hardware and platform implementation

- [ ] ShivaCore ABI adapter and capability handoff
- [ ] `globus-init` executable and service supervisor
- [ ] x86_64 UEFI boot integration
- [ ] aarch64 boot integration
- [ ] PCI/PCIe and DMA/IOMMU services
- [ ] NVMe/storage driver service
- [ ] network driver + IPv4/IPv6/TCP/UDP implementation
- [ ] input/USB service
- [ ] display/GPU service
- [ ] persistent VFS backend
- [ ] cryptographic package/update verification

## P2 — desktop and AI platform

- [ ] compositor and window manager
- [ ] desktop shell
- [ ] audio backend
- [ ] Aurora IPC bridge
- [ ] AI sandbox/tool permission broker
- [ ] model/runtime resource broker
- [ ] system settings and identity UI

## P3 — ecosystem integration

- [ ] ATC runtime service boundary
- [ ] ATCLang/ATC-VM host integration
- [ ] SDK and developer APIs
- [ ] signed release images
- [ ] recovery environment
- [ ] reproducible system images
- [ ] formal security audit evidence

## Release gates

`IMPLEMENTED` does not imply `AUDITED` or `PRODUCTION_READY`. A release requires passing the applicable architecture, security, integration, reproducibility and recovery gates with recorded evidence.

## CI failure register — 2026-10-11

This register records observed failures from the exact pull-request heads below. A failed run is not evidence that the current default branch fails, and a log diagnosis is not a verified fix. Status remains **BLOCKED / NOT VERIFIED** until a corrective commit is tested on its exact head SHA.

| PR | Head SHA | Failed workflow/run | Observed root-cause evidence | Status |
|---|---|---|---|---|
| [#33 — diagnostics/taskbar](https://github.com/A-TownChain-Okosystems/globus-os/pull/33) | `1fa45a6a56b9a03b5ad0f4df82ca510b79bfbf78` | Rust CI `37595404883`; System CI `37595404946`; ATC Test Suite `37595405056` | rustfmt diffs in `system/diagnostics/src/lib.rs` and `system/graphics/src/taskbar.rs`; compile errors include unresolved `x86_64`, unresolved `globus_diagnostics::CrashEvent`, and missing `vec!`, `format!`, and `Vec` in no_std consumers | BLOCKED |
| [#34 — diagnostics/taskbar foundation](https://github.com/A-TownChain-Okosystems/globus-os/pull/34) | `3a8c704c0384027f2da97f1b133da8e24ec98fc7` | Rust CI `37595410666`; System CI `37595410668`; ATC Test Suite `37595410641`; RustSec `37595410642` | `globus-diagnostics` compile failure includes incorrect `crate::executor::RepairResult` paths; RustSec could not fetch advisory database because of an IO/network error (not a confirmed vulnerability and not a passing audit) | BLOCKED |
| [#36 — boot blockers](https://github.com/A-TownChain-Okosystems/globus-os/pull/36) | `1c969dc61133cdcbc0ec79bd116c42d232b13848` | Rust CI `37595417377`; System CI `37595417322`; Kernel Boot `37595417280`; ATC Test Suite `37595417286` | kernel build and workspace tests fail; unresolved `x86_64` dependency/import and additional ShivaCore compile errors | BLOCKED |
| [#37 — IPCBus/EventBus](https://github.com/A-TownChain-Okosystems/globus-os/pull/37) | `f5f30b53ff46063f50b9b7ae6b70a3cb43a9838c` | Rust CI `37595421112`; System CI `37595421044`; ATC Test Suite `37595421034`; Kernel Boot `37595421024` | kernel compile break includes unresolved `x86_64`, missing no_std collection macros/types, invalid inline-asm `rbx` operand, missing `PageSize` trait scope, and move-out-of-borrowed `UserContext` error at `user_sched.rs:723` | BLOCKED |

### Required corrective sequence

1. Reconcile diagnostics public API and dependency/feature boundaries across kernel and service-space consumers.
2. Correct no_std imports/macros and architecture-specific kernel code without weakening safety or boot gates.
3. Resolve PR #33/#34 overlap additively against current `main`; retain compatible functionality from both proposals.
4. Treat the RustSec network failure as **infrastructure failure / audit result unknown**. Retry the unchanged audit gate and record its resulting run/job/step/exit evidence.
5. Rerun formatting, workspace build/tests, System CI, and applicable x86_64 boot checks on each new exact head SHA. Record `IMPLEMENTED`, `VERIFIED`, and `RESIDUAL` separately.
6. Do not merge or close these PRs until the merge conflict is resolved and all required gates have fresh successful evidence.

The failed job logs were fetched and the root-cause summaries were posted to the corresponding PR discussions on 2026-10-11. Documentation of a finding does not mean the source defect has been fixed. The RustSec job retry was requested; its completion/result is not yet verified in this register.

