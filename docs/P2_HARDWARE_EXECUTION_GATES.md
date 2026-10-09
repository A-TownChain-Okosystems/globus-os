# P2 Hardware Execution Evidence Gates

Status: `NOT_READY`

This document defines the evidence required before GlobusOS may claim that the P2 hardware path is executable.

## Execution chain

```text
UEFI
  -> ExitBootServices
  -> ACPI RSDP / DMAR discovery
  -> PCI ECAM enumeration
  -> BAR decode + MMIO mapping
  -> IOMMU root/context domain
  -> MSI/MSI-X interrupt routing
  -> NVMe admin queue
  -> NVMe Identify / namespace I/O
  -> NIC-specific DMA driver
  -> persistent block I/O
  -> crash/recovery validation
```

## Gates

| Gate | Required evidence | Readiness |
|---|---|---|
| UEFI-01 | OVMF or physical UEFI boot reaches kernel entry | PASS/FAIL |
| ACPI-01 | Valid RSDP and parsed DMAR evidence | PASS/FAIL |
| PCI-01 | Enumerated BDFs, class codes and BARs | PASS/FAIL |
| IOMMU-01 | Translation enabled with requester-scoped mappings | PASS/FAIL |
| IRQ-01 | MSI/MSI-X capability discovered and vector delivery observed | PASS/FAIL |
| NVME-01 | Controller reset/configure/enable succeeds | PASS/FAIL |
| NVME-02 | Identify Controller and Identify Namespace completion observed | PASS/FAIL |
| NVME-03 | Namespace read/write completion observed through DMA | PASS/FAIL |
| NET-01 | Target NIC identified by PCI ID and correct driver selected | PASS/FAIL |
| NET-02 | RX/TX DMA completion observed | PASS/FAIL |
| FS-01 | Persistent block writes survive clean reboot | PASS/FAIL |
| FS-02 | Journal replay recovers an interrupted transaction | PASS/FAIL |
| SEC-01 | DMA is denied unless explicitly mapped by policy | PASS/FAIL |
| EVID-01 | Serial log + machine configuration + test result are retained | PASS/FAIL |

## Non-negotiable safety rules

1. No device may receive an unrestricted physical-memory DMA address.
2. MMIO access is valid only through a kernel-owned device-memory mapping.
3. Device identity must be established from PCI configuration space, not guessed from the host model.
4. A driver must refuse unsupported vendor/device IDs.
5. Failure of a hardware gate keeps the system `NOT_READY`.
6. QEMU evidence and physical-machine evidence are distinct; QEMU success does not imply physical-hardware readiness.

## Current implementation boundary

The P2 branch contains hardware-facing primitives, but the complete execution chain is not yet proven. In particular, PCI BAR enumeration, complete DMAR device-scope mapping, requester-scoped IOMMU contexts, MSI/MSI-X delivery, NVMe command execution, target-NIC selection, and block-device-backed filesystem recovery still require execution evidence.
