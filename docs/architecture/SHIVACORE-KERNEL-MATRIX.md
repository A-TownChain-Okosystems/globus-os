---
document_id: ATC-SHIVACORE-KERNEL-MATRIX-001
title: ShivaCore Kernel Function Matrix
version: 1.0.0
status: ARCHITECTURE CONTRACT
standard: ATC-STD-MD-001
canonical_source: modules/atc-shivacore/kernel/
---

# ShivaCore Kernel Function Matrix

## 1. Scope and authority

This matrix defines the canonical **ShivaCore microkernel / TCB contract** implemented and verified by the canonical GlobusOS source tree.

Canonical implementation:
`modules/atc-shivacore/kernel/`

The matrix is a contract and traceability surface. It does **not** convert source presence into implementation or verification evidence.

The normative TCB is limited to:
- capability validation and object authority;
- address-space and memory protection;
- scheduling and context switching;
- IPC endpoint authorization and message transfer;
- interrupt/trap handling;
- timer primitives;
- architecture-specific mechanisms required by those functions.

OS services, network stacks, filesystems, blockchain execution, AI services and application policy remain outside the normative TCB unless a separate kernel contract explicitly assigns a mechanism to the TCB.

## 2. Status model

`UNANALYZED → ANALYZED → FIXED → RERUNNING → VERIFIED → RESIDUAL`

Current matrix status is intentionally **UNANALYZED** until exact-SHA CI evidence exists.

## 3. Canonical kernel functions

| ID | Domain | Function | Contract / responsibility | Canonical source | Test / evidence | Verification | Residual |
|---|---|---|---|---|---|---|---|
| SHK-001 | Boot | Kernel initialization | Establish deterministic kernel entry, initialization order and safe transition into scheduler/userspace | `kernel_init.rs`, `main.rs`, `lib.rs` | GlobusOS kernel CI; boot smoke | UNANALYZED | Exact-SHA boot evidence required |
| SHK-002 | HAL | Architecture abstraction | Isolate CPU, memory, interrupt and timer primitives behind the HAL boundary | `hal/mod.rs`, `hal/cpu.rs` | HAL target tests | UNANALYZED | Target coverage required |
| SHK-003 | Memory | Physical memory management | Manage frames/pages without violating kernel/userspace isolation | `memory.rs`, `memory_manager.rs`, `mempool.rs` | Kernel memory tests | UNANALYZED | Exact-SHA evidence required |
| SHK-004 | Memory | Address spaces / VM | Establish and enforce virtual address-space mappings and protection | `vm.rs`, `vmm.rs` | VM/page-table tests | UNANALYZED | Target-specific mapping evidence required |
| SHK-005 | Memory | Allocation | Provide deterministic kernel allocation primitives within defined safety limits | `allocator.rs` | Allocator tests | UNANALYZED | Verify no isolation bypass |
| SHK-006 | Capability | Capability validation | Validate explicit authority for kernel object access; no ambient authority | `capability.rs` | Capability tests | UNANALYZED | Revocation/negative-path evidence required |
| SHK-007 | Capability | Object / CSpace control | Bind capabilities to kernel objects and controlled namespaces | `capability.rs`, kernel object code | Capability/CSpace tests | UNANALYZED | Canonical object model evidence required |
| SHK-008 | Scheduling | Scheduler | Select runnable execution contexts while preserving isolation and defined starvation guarantees | `scheduler.rs`, `user_sched.rs` | Scheduler tests | UNANALYZED | Fairness/preemption evidence required |
| SHK-009 | Threads | Context switching | Create/manage kernel execution contexts and safe context transitions | `threads.rs`, `process.rs` | Context/thread tests | UNANALYZED | Target register-state evidence required |
| SHK-010 | IPC | Endpoint authorization | Authorize sender/receiver access before IPC transfer | `ipc.rs` | IPC/negative authorization tests | UNANALYZED | Capability-to-endpoint evidence required |
| SHK-011 | IPC | Message transfer | Provide bounded kernel-mediated message transfer without implicit authority escalation | `ipc.rs` | IPC integration tests | UNANALYZED | Exact-SHA IPC evidence required |
| SHK-012 | Interrupts | Trap / interrupt handling | Enter privileged handlers safely and return to the correct execution context | `interrupts.rs`, `page_fault.rs` | Interrupt/page-fault tests | UNANALYZED | Target smoke evidence required |
| SHK-013 | Timers | Timer primitives | Provide kernel timer mechanisms required for scheduling and time-based kernel operations | `timer.rs` | Timer tests | UNANALYZED | Hardware-backed evidence required |
| SHK-014 | Syscalls | Stable kernel ABI | Expose only documented kernel mechanisms through the syscall boundary | `syscall.rs` | ABI/syscall tests | UNANALYZED | ABI compatibility evidence required |
| SHK-015 | Userspace | Process / userspace boundary | Establish controlled transition from kernel to userspace and enforce privilege separation | `userspace.rs`, `process.rs`, `elf_loader.rs` | Userspace/ELF smoke tests | UNANALYZED | Loader security evidence required |
| SHK-016 | SMP | Multi-core coordination | Initialize and coordinate multiple CPUs without violating scheduler, memory or capability invariants | `smp.rs` | SMP/QEMU tests | UNANALYZED | Target-specific SMP evidence required |
| SHK-017 | Security | Kernel isolation | Enforce privileged boundary, fail-closed behavior and protection invariants across kernel mechanisms | `security.rs`, `module_security.rs` | Security/audit CI | UNANALYZED | Privileged unsafe-code audit required |
| SHK-018 | Diagnostics | Kernel diagnostics | Expose controlled diagnostics without leaking secrets or bypassing authority boundaries | `diagnostics.rs`, `tracing.rs` | Diagnostics/security tests | UNANALYZED | Log-separation evidence required |

## 4. Explicitly non-canonical / boundary-controlled modules

The current source tree contains additional modules including networking, filesystems, blockchain-oriented components, AI-related components, containers and loadable-kernel-module support.

Their presence in `modules/atc-shivacore/kernel/` does **not** automatically make them part of the normative minimal TCB.

Examples requiring separate contract classification:
- `net.rs`, `tcpip.rs`, `p2p.rs`, `p2p_secure.rs`, `sockets.rs`
- `vfs.rs`, `atcfs.rs`, `devfs.rs`, `fs_journal.rs`
- `ai.rs`, `contract.rs`, `block.rs`
- `container.rs`, `container_net.rs`
- `lkm.rs`, `module_security.rs`

The known `lkm.rs` dependency-resolution blocker remains a residual implementation issue and must not be represented as verified kernel functionality.

## 5. Authority model

```text
Firmware / Bootloader
        ↓
Architecture HAL
        ↓
ShivaCore TCB
  Capability → Memory → Scheduler → IPC → Traps/Timers
        ↓
Kernel-facing OS services
        ↓
GlobusOS userspace
        ↓
Applications / Aurora
```

Aurora, UI, AI models and OS services cannot independently grant kernel authority. Kernel authority is established only through the kernel's capability and privilege mechanisms.

## 6. Verification contract

A matrix row becomes VERIFIED only when current exact-SHA evidence establishes:

`Run → Job → Ergebnis → Step → Exit code → Log → Source → Root Cause → Minimal Fix → Commit → Rerun`

Required evidence families:
1. target kernel compilation;
2. target HAL coverage;
3. boot/image generation where claimed;
4. kernel smoke/integration tests;
5. IPC/ABI tests;
6. privileged unsafe-code/security audit.

Historical CI runs do not verify a newer SHA.

## 7. Residuals

- Kernel source is canonical in GlobusOS, not the historical `atc-shivacore` repository.
- Production status remains NOT_READY until the release gate is satisfied.
- `lkm.rs` dependency-resolution correctness remains an active blocker.
- Matrix rows are architecture traceability, not implementation claims.

## 8. Machine-readable identity

- Matrix ID: `ATC-SHIVACORE-KERNEL-MATRIX-001`
- Canonical source: `modules/atc-shivacore/kernel/`
- Function count: 18
- Status: `ARCHITECTURE CONTRACT`


Agent-ID: ATC-AI-ARCH-001
Task-ID: ATC-TASK-0930
AI-Role: software-development
Validation: PENDING
