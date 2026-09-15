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
