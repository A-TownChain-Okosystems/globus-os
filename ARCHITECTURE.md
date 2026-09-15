# GlobusOS Architecture

The normative architecture is documented in [`docs/SYSTEM_ARCHITECTURE.md`](docs/SYSTEM_ARCHITECTURE.md).

GlobusOS is the userspace operating-system layer above ShivaCore. The current implementation foundation is the Rust workspace under `system/`.

## Security boundary

```text
Hardware -> HAL -> ShivaCore TCB -> IPC/capabilities -> GlobusOS -> Aurora/apps
```

ShivaCore remains reusable and OS-neutral. GlobusOS owns system services, device integration, storage, networking, graphics, audio, package lifecycle and recovery. Aurora remains an AI platform outside the kernel TCB.

## Current implementation

- Process/thread model
- Address-space and memory policy types
- Capability authorization boundary
- IPC endpoint/message model
- VFS mount contract
- Network policy boundary
- Device registry
- Service lifecycle/dependency order
- Graphics and audio API boundaries
- Package verification model
- Atomic update/rollback state machine
- Integrated runtime status
- CI validation for formatting, compilation, tests and Clippy

These components establish the API and trust boundaries; hardware-specific drivers, protocol implementations, boot integration and production-grade cryptography remain explicit implementation milestones and are not claimed complete by this document.
