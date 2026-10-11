# ShivaCore Service-Space Drift Audit

- **Repository:** `globus-os`
- **Audited commit:** `fef3af85ff9ecab90c25bc179e2a3284fa17f633`
- **Date:** 2026-10-11
- **Scope:** active kernel exports, unregistered kernel source files, Service-Space placement, reverse dependencies, local test evidence
- **Verdict:** `PARTIAL_SEPARATION_CONFIRMED`

## Evidence summary

| Check | Result |
|---|---|
| `shivacore` workspace member | PASS |
| `shivacore-service-space` workspace member | PASS |
| Kernel -> Service-Space source references | 0 found |
| Service-Space contains extracted domain modules | PASS |
| ShivaCore kernel tests | PASS, 546/546 |
| Service-Space tests | PASS, 548/548 |
| Service-Space depends on kernel crate | PRESENT |
| Service-Space uses narrow stable ABI only | NOT YET |
| Unregistered kernel source files | PRESENT, classified below |

Commands used:

```text
cargo test -p shivacore --lib
cargo test -p shivacore-service-space --lib
```

Both commands passed on the audited commit. This is local evidence, not a replacement for GitHub CI evidence bound to the same SHA.

## Confirmed extraction

The following domain modules are active in `modules/atc-shivacore/service_space/src/` and are exported by its `lib.rs`:

`blockchain`, `consensus`, `genesis`, `genesis_bridge`, `gossip_bridge`, `security_audit`, `mempool`, `vm`, `atcnet`, `net`, `tcpip`, `p2p`, `p2p_secure`, `sockets`, `contract`, `ai`, `did`, `knowledge_graph`, `remote_caps`, `service_init`, `identity_key_hal`, and `identity_key_service`.

They are not exported from `modules/atc-shivacore/kernel/src/lib.rs`. The reverse direction is therefore preserved: the kernel does not import the Service-Space crate.

## Remaining coupling

`service_space/Cargo.toml` still contains:

```toml
shivacore = { path = "../kernel" }
```

The Service-Space currently uses these kernel-internal paths:

- `shivacore::security::{simple_hash, TokenBucket}`
- `shivacore::capability::{Rights, ...}`
- `shivacore::ats1000::NetworkStack`
- `shivacore::kernel_init::KernelState::boot`

Therefore the extraction is active, but the final boundary is not yet a narrow, stable ABI or capability-only interface.

## Classification of unregistered kernel files

These files are physically present under `kernel/src/` but are not exported by the kernel `lib.rs`. The audit found no evidence that they are Service-Space leftovers. They are retained as planned kernel or hardware-chain work unless a later architecture decision changes their ownership.

| File | Decision | Reason |
|---|---|---|
| `block.rs` | retain / planned | K-Sprint 11 block-device layer; kernel/hardware-chain work |
| `container.rs` | retain / planned | K-Sprint 41 container isolation and sandboxing |
| `cow.rs` | retain / planned | K-Sprint 45 copy-on-write fork engine |
| `devfs.rs` | retain / planned | K-Sprint 38 device filesystem and kernel logging |
| `framebuffer.rs` | retain / planned | x86 boot framebuffer output |
| `fs_journal.rs` | retain / planned | filesystem journal implementation |
| `gdt.rs` | retain / planned | x86 descriptor table and TSS |
| `hw_drivers.rs` | retain / planned | explicitly marked hardware-driver planning marker |
| `interrupts.rs` | retain / planned | IDT, PIC and fault handling |
| `memory.rs` | retain / planned | x86 physical-memory and paging boot path |
| `module_security.rs` | retain / planned | kernel module security work |
| `page_fault.rs` | retain / planned | page-fault and demand-paging path |
| `power.rs` | retain / planned | K-Sprint 40 power and ACPI |
| `serial.rs` | retain / planned | early kernel/QEMU debug console |
| `signals.rs` | retain / planned | K-Sprint 42 kernel signal handling |
| `smp.rs` | retain / planned | K-Sprint 43 multi-core support |
| `threads.rs` | retain / planned | K-Sprint 39 threading and futex support |
| `tracing.rs` | retain / planned | K-Sprint 46 kernel observability |
| `user_io.rs` | retain / planned | K-Sprint 34 file descriptors and user I/O |

`lib.rs` and `main.rs` are crate entry points, not orphaned subsystem files.

## Conclusion

The evidence supports the following precise claim:

> The active non-TCB domain implementation has been moved into Service-Space and the kernel has zero reverse references to Service-Space. The remaining unregistered kernel files are planned kernel/hardware work, not confirmed duplicate Service-Space implementations. Full separation is still incomplete because Service-Space depends directly on the kernel crate and imports internal kernel modules.

## Follow-up

1. Define a small public kernel contract crate or explicit capability/syscall boundary.
2. Replace direct Service-Space imports of `security`, `capability`, `ats1000`, and `kernel_init` with that contract.
3. Keep the classified planned kernel files until their owner and acceptance tests are explicit.
4. Re-run this audit and bind the result to the exact commit after the boundary change.
