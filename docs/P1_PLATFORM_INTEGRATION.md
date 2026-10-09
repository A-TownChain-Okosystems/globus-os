# GlobusOS P1 Platform Integration

Status: IMPLEMENTED (contracts and deterministic userspace orchestration)
Readiness: NOT_READY

## Scope

P1 establishes the execution contracts required to move from a system-foundation workspace toward a bootable, hardware-integrated GlobusOS userspace.

### Implemented in this increment

- deterministic `globus-init` boot/service ordering;
- explicit capability handoff model between ShivaCore and system services;
- PCIe and IOMMU/DMA policy data model;
- NVMe controller/namespace/queue contract;
- Ethernet/IP interface contract;
- persistent VFS block-device and filesystem contract;
- architecture documentation for UEFI/x86_64 integration.

### Not yet production implementations

The following remain hardware/driver work and must not be inferred from the contracts:

- real UEFI bootloader and ELF loader;
- x86_64 page-table/interrupt integration with ShivaCore;
- PCI config-space enumeration;
- IOMMU programming (VT-d/AMD IOMMU);
- NVMe MMIO/queue execution;
- real Ethernet DMA and packet driver;
- persistent filesystem implementation and crash-consistent journaling;
- cryptographic key provisioning and measured boot.

## Boot sequence

```text
UEFI firmware
  -> bootloader
  -> ShivaCore ELF
  -> kernel initializes memory/IPC/capabilities
  -> capability handoff
  -> globus-init
  -> storage + device discovery
  -> network service
  -> core system services
  -> user session / Aurora
```

## Trust boundary

Hardware drivers are isolated services. DMA requires an explicit IOMMU domain and policy. `globus-init` does not receive ambient hardware authority; it receives only the capabilities explicitly handed to it by the kernel/service broker.

## Acceptance gates

P1 can only progress to `PRODUCTION_READY` after hardware-backed evidence exists for each item above. A passing Rust unit test validates the contract only; it does not validate hardware behavior.
