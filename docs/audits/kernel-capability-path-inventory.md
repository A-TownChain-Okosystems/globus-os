# Kernel capability-path inventory

Snapshot audited for the P0.1 service-space migration. This document distinguishes implemented checks from missing end-to-end wiring.

## Existing authoritative implementation

- `modules/atc-shivacore/kernel/src/capability.rs`
  - Kernel-owned `CapabilityTable`; monotonically assigned `CapId`; owner PID; resource type/id; rights bitfield; parent link for delegated capabilities.
  - `create` inserts a capability for a PID.
  - `delegate` checks source ownership, requires `DELEGATE`, and rejects rights expansion.
  - `check` validates resource type, resource id and rights for capabilities indexed to a PID.
  - `check_any` validates handle ownership and requested rights.
  - `revoke` removes the selected capability and descendants.
  - Risk found and corrected in this change: `Rights::from_bits_truncate` previously masked with `0x0F`, incorrectly discarding `INSPECT` (bit 4). It now preserves the five currently defined bits and has regression tests.

## Existing ABI and dispatch

- `system/shivacore-api/src/lib.rs` exposes ABI version 1.0, opaque `CapabilityHandle`, capability rights, and syscall IDs for IPC send/receive, capability query, handle close and monotonic time.
- `modules/atc-shivacore/kernel/src/syscall.rs` validates ABI version and payload bounds; IPC send/receive check channel capability ownership/resource/right; capability query requires `INSPECT`.
- The dispatcher currently accepts `&CapabilityTable`, so it cannot create, delegate or revoke capabilities through its current dispatch interface.
- `IpcSend` and `IpcReceive` currently return after authorization checks. They do not enqueue/dequeue a message or copy a payload in the inspected implementation.
- Search of the repository's indexed code found the dispatcher and its unit tests, but did not establish a production architecture trap/entry point invoking `SyscallDispatcher::dispatch`. This must be verified in the boot/interrupt path before claiming the ABI is wired end to end.

## IPC transport

- `system/ipc/src/channel.rs` implements bounded per-endpoint FIFO queues and payload/queue/endpoint checks.
- The channel registry has no capability table or caller PID parameter; its `send`/ `receive` methods are not, by themselves, an authorization boundary.
- A production bridge must validate the sender/receiver against the kernel-owned capability table and then invoke the actual queue operation; user-supplied IDs must never be treated as proof of authority.

## Knowledge Graph coupling

- `modules/atc-shivacore/service_space/src/knowledge_graph.rs` imports `CapId`, `CapabilityTable`, `Pid`, `ResourceType`, and `Rights` from the kernel crate.
- Graph operations directly create/check/delegate/revoke entries in the kernel table. This is useful for in-process tests but is not a stable userspace contract and bypasses any syscall boundary.
- `modules/atc-shivacore/service_space/Cargo.toml` therefore still depends on the kernel crate. Removing this dependency before a working capability client and syscall transport exist would replace an architectural coupling with a nonfunctional abstraction.

## Required completion gates

1. Define stable wire-safe process/resource identifiers and capability operation request/response types in `libshivacore`.
2. Add fail-closed create/delegate/check/revoke dispatch operations, with an explicit kernel-controlled authority model for capability creation.
3. Wire the architecture syscall entry to the dispatcher and connect IPC dispatch to the bounded channel transport.
4. Add a userspace client over the real syscall transport; do not ship a mock transport as production.
5. Migrate the Knowledge Graph to stable userspace types/client and remove the direct kernel dependency.
6. Add negative tests for foreign-owner use, missing/invalid handles, insufficient rights, rights expansion, unauthorized create/revoke, revoked descendants, malformed ABI, and IPC queue/payload errors.
7. Confirm formatting, build, unit/integration tests and security workflows all pass on the exact final head SHA.

## Current disposition

P0.1 remains incomplete. The rights-mask regression is fixed and tested in source, but no claim of end-to-end capability enforcement or successful CI is made by this inventory alone.
