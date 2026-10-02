# ShivaCore Boot Blocker

## Purpose

The Boot Blocker is a mandatory CI release gate for the real x86_64 boot path.

A green unit-test suite, a successfully built kernel ELF, or the existence of a boot image is **not** sufficient to establish boot correctness.

The gate blocks verification unless the QEMU serial log proves the complete required boot evidence chain.

## Required evidence

For each CPU model:

1. Kernel entry point reached.
2. CPU vendor detection matches the QEMU model.
3. CPU HAL compatibility check passes.
4. USER-001 maps PID 1000.
5. SCHED-001 arms PID 1000 and PID 1001.
6. Real Ring-3 entry for PID 1000 is attempted.
7. Timer-driven preemption switches to PID 1001.
8. Timer-driven preemption returns to PID 1000.
9. Register checkpoints for both processes are preserved.
10. Canonical `KernelState::boot()` reaches `Done`.
11. No kernel panic or triple fault is present.

## Blocking semantics

The gate returns non-zero when:

- the QEMU log is missing or empty;
- any required evidence marker is absent;
- `KernelState::boot()` reports failure;
- a panic or triple fault is detected;
- the evidence is otherwise incomplete.

A QEMU timeout is **not** independently accepted as proof of success. It is acceptable only when the complete evidence chain has already been emitted and the Boot Blocker passes.

## Evidence policy

The Boot Blocker does not weaken existing tests and does not replace source-level or subsystem-specific gates. It adds a hard integration gate at the actual BIOS → kernel → Ring-3 → scheduler → kernel-init path.

A repository state must not be described as boot-verified without a successful Boot Blocker result for the exact source SHA.
