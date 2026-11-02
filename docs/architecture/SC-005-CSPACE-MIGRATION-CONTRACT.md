# SC-005 CSpace Migration Contract

**Status:** Draft implementation contract — derived from frozen SC-005 v0.1.0; not a normative amendment  
**Canonical implementation:** `modules/atc-shivacore/kernel/src/capability.rs`  
**Architecture decision:** `a-townchain-ecosystem#241`, ADR-0002, D1–D6 accepted  
**Frozen source:** `atc-shivacore@3b56253b39bdc6cbf719ed9a2a21461d73b8fc34:docs/specs/SC-005-CAPABILITY_SYSTEM.md`  
**Frozen blob:** `2e19216b43237fcbd6e175a7abf15c1781cde4ca`

## 1. Purpose and scope

This document translates SC-005 REQ-SC005-01…11 and INV-01…09 into an implementation and migration contract for the existing GlobusOS kernel capability authority.

It does not amend SC-005, SC-003, SC-004, SC-001 or SC-007. It does not claim implementation or conformance. Any ambiguity against a frozen contract is a blocker and must be handled through the applicable versioned owner/governance process.

The migration replaces the existing flat `CapabilityTable` model in place. It MUST NOT introduce a second active capability manager, parallel CSpace, compatibility authority, or independently mutable subsystem capability table.

## 2. Normative data model mapping

| SC-005 requirement | Required implementation representation |
|---|---|
| REQ-SC005-01 | `CNode` with exactly 16,384 addressable slots by default; slot states are Empty or Capability. Full insertion returns CSPACE_FULL; never overwrite. |
| REQ-SC005-02 | `CSpace` is a tree of CNodes. The root is the Initial-Cap of its owner. A thread resolves a capability using a CNode path and slot. |
| REQ-SC005-03 | Capability entry stores object type, kernel object reference, 16-bit rights mask and immutable 64-bit badge. Raw pointers are never authority. |
| REQ-SC005-04 | Rights are validated against the object-type-specific allow-list. Unknown or foreign bits return INVALID_CAP; no truncating parser is permitted. |
| REQ-SC005-05…07a | mint attenuates rights and creates a new derivation edge; derive copies rights and badge unchanged; depth defaults to 16; overflow returns DERIVATION_LIMIT without mutation. |
| REQ-SC005-08…09a | revoke removes the complete derivation subtree atomically; Frame revocation additionally requires all AddressSpace unmaps and completed TLB invalidation before return. Running atomic invokes use the SC-004 snapshot boundary; blocked operations follow SC-003 I-E06/I-E07. |
| REQ-SC005-10 | Badge is immutable provenance, never a rights channel. Only Endpoint mint may assign a new badge to the child; parent badge is unchanged. |
| REQ-SC005-11 | Initial layout has no universal All-Rights capability and no ambient derive authority. Every initial capability is object-typed and rights-bounded. |

### 2.1 Core types

The implementation is expected to converge on these semantic types in the existing `capability.rs` module:

- `ObjectType`: Endpoint, Notification, Frame, AddressSpace, Thread, CNode, UntypedMemory, Timer, Device/IRQ as specified by SC-005 §3.
- `ObjectRef`: opaque, kernel-owned typed object identity; not a caller-provided pointer.
- `ObjectRights(u16)`: validated only against the exact rights set for its `ObjectType`.
- `CapabilityEntry`: `{ object_type, object_ref, rights, badge, derivation_node }`.
- `CNode`: fixed-capacity slot array with explicit Empty/Occupied state.
- `CSpace`: root CNode plus reachable CNode tree and owner binding.
- `CapPath`: bounded CNode path plus slot index; resolution checks the caller's own CSpace.
- `DerivationNode`: parent/children relation sufficient for complete recursive revocation.
- `KernelCapabilityAuthority`: sole kernel-owned authority that creates the Initial root, resolves paths, performs invoke/mint/derive/revoke, and coordinates revocation completion.

These are semantic requirements, not permission to add an independent second authority. Concrete Rust representation and allocation strategy must be validated against the no_std target, bounded allocation policy, and kernel synchronization model before implementation.

### 2.2 Object-specific rights

The implementation must not reuse one generic rights namespace across object classes.

| Object | Permitted rights |
|---|---|
| Endpoint | SEND, RECV, DELEGATE, NOTIFY |
| Notification | SIGNAL, WAIT |
| Frame | READ, WRITE, EXECUTE, GRANT |
| AddressSpace | MAP, UNMAP, SWITCH |
| Thread | SUSPEND, RESUME, CONFIG |
| CNode | DUPLICATE, MINT, MOVE |
| UntypedMemory | RETYPE |
| Timer | SET_DEADLINE, ACK |
| Device/IRQ | ACCESS, SUBSCRIBE, ACK |

Frame GRANT is not equivalent to generic DELEGATE. AddressSpace MAP is not generic WRITE. Mapping rights must additionally satisfy SC-007's subset and W^X constraints.

## 3. Authority and ownership boundaries

1. There is exactly one kernel-owned capability authority implementation and one authoritative Initial root for the Initial Task.
2. The Initial Task receives the Root-CNode capability and only the explicit initial capabilities listed by SC-001/SC-005.
3. Services and child execution contexts receive authority only through explicit, rights-reducing mint operations over concrete target capabilities.
4. ProcessManager, MemorySubsystem, VFS, IPC and syscall dispatch receive restricted references/operations. They do not own independent mutable capability tables and do not receive ambient root access.
5. A caller-supplied `&CapabilityTable` is not an authority boundary. Syscall dispatch must resolve the caller's CSpace from kernel-owned thread/process state.
6. The authority's root custodian and CSpace lifecycle are kernel-internal. A service cannot select an arbitrary root by passing a pointer, PID-indexed table, or resource identifier.

## 4. Operation contracts

### resolve / invoke
- Resolve `CapPath` only against the calling thread's bound CSpace.
- Empty, malformed, out-of-range or unreachable slots return INVALID_CAP.
- Missing object-specific rights return AUTH_DENIED with no state change.
- Resolution and rights validation form one atomic invoke-entry snapshot per SC-004/SC-005 REQ-SC005-09a.

### mint
- Requires the appropriate CNode MINT authority and concrete source capability.
- Child rights must be a subset of parent rights and valid for the same object type.
- Rights expansion returns AUTH_DENIED and emits the required diagnostic event.
- CNode capacity exhaustion returns CSPACE_FULL without mutation.
- Derivation depth above 16 returns DERIVATION_LIMIT without mutation.
- A new badge may be assigned only for Endpoint mint; the source badge is never changed.

### derive
- Copies rights and badge exactly; does not add rights or change badge.
- Registers the child in the source derivation tree.
- Enforces the same CNode capacity and derivation-depth failure atomicity.

### revoke
- Kernel-internal non-derivable originals cannot be revoked through a user capability (AUTH_DENIED).
- Revokes the entire derivation subtree atomically.
- For Frame objects, coordinate unmap across every AddressSpace and do not return until the SC-007 TLB completion condition is met.
- Blocked operations follow SC-003 I-E06/I-E07; in-flight non-blocking atomic invokes follow SC-005 snapshot semantics.
- No zombie slot, reachable descendant, or post-return access is permitted.

## 5. Existing implementation findings and required migration

Baseline GlobusOS source inspected at `main` (source commit baseline previously recorded as `f370b681e5c9ff49577f25c24f15b5fc880d5cba`):

| File | Current finding | Required destination |
|---|---|---|
| `capability.rs` | Flat `BTreeMap<CapId, Capability>`, PID index, generic 8-bit rights, generic ResourceType | Sole CSpace/CNode/typed-right/derivation authority implementation |
| `kernel_init.rs` | Creates separate MemorySubsystem, ProcessManager and VFS capability stores | Construct one authority at the Initial Task boundary and inject constrained interfaces |
| `process.rs` | ProcessManager owns a CapabilityTable and auto-creates generic Memory caps | Store process/thread CSpace binding; no local table and no implicit root/mint |
| `memory_manager.rs` | MemorySubsystem owns a CapabilityTable; allocation auto-mints generic Memory caps | Consume explicit typed authority operations; no local capability store |
| `vfs.rs` | VFS accepts `Arc<Mutex<CapabilityTable>>` and checks generic rights | Use object-bound VFS capability references through authority resolution |
| `syscall.rs` | Dispatch accepts caller-provided `&CapabilityTable`; HandleClose only checks handle presence | Resolve CapPath through the kernel-bound caller CSpace; close/revoke semantics must be real operations |

The recent removal of implicit EXEC grants in PR #112 is a least-authority hardening step only. It does not implement CSpace or establish SC-005 conformance.

## 6. Migration sequence and hard gates

### Phase A — authority replacement design
- Confirm no other capability manager or active capability table exists in the canonical kernel path.
- Define the bounded CNode storage strategy, path encoding, synchronization/locking, and kernel object reference registry.
- Define exact typed-right bit assignments from frozen SC-005/SC-003/SC-006 contracts; do not invent values or collapse rights across object types.
- Define Initial Task root construction only at the resolved SC-001 boot/Initial Task boundary.
- Gate: no code until the design has explicit type/bit-level traceability to frozen standards.

### Phase B — core CSpace primitive
- Replace the internals of the existing `capability.rs` authority; do not leave the flat table active beside a new CSpace.
- Implement fixed-capacity CNode slots, path resolution, typed rights validation, derivation tree, mint/derive and failure atomicity.
- Implement deterministic unit tests for empty/full/invalid slots, rights masks, attenuation, badge immutability, depth 16/17 and derivation registration.
- Gate: the existing callers must either be migrated in the same coherent change or remain explicitly blocked; no compatibility shim may retain ambient authority.

### Phase C — kernel wiring
- Construct the single authority at kernel initialization.
- Bind each thread/process to its own authorized CSpace path/root capability according to the Initial Task delegation model.
- Migrate ProcessManager, MemorySubsystem, VFS, IPC and syscall dispatch to restricted authority operations.
- Remove all independent CapabilityTable constructors and caller-supplied authority parameters from production paths.
- Gate: negative tests prove cross-CSpace reads and forged PID/CapPath attempts fail.

### Phase D — lifecycle and integration
- Integrate Frame GRANT + AddressSpace MAP with the existing VMM under SC-007.
- Complete cross-AddressSpace revocation, synchronous unmap/TLB completion and blocked-operation wake semantics.
- UntypedMemory/Retype remains blocked until the SC-001 Limine B0–B5 versus current bootloader_api discrepancy is resolved through the versioned owner/governance process.
- Gate: exact-head tests, integration tests, QEMU evidence and artifact hashes; no VERIFIED claim from CI green alone.

## 7. Required conformance tests

- CNode has 16,384 slots by default; Empty/Occupied state is deterministic; full insertion never overwrites.
- CSpace path resolution is limited to the caller's own authorized CSpace.
- Unknown rights bits are rejected as INVALID_CAP (never truncated).
- Every object type rejects rights from another object's rights namespace.
- mint rejects rights expansion, depth overflow and full CNode without changing source or existing descendants.
- derive preserves badge exactly; Endpoint mint changes only the child's badge.
- revoke removes the complete derivation subtree; no descendant resolves after return.
- Frame revoke does not return until all AddressSpace unmaps and required TLB acknowledgements complete.
- A blocked operation is woken with the prescribed error; an in-flight atomic invoke follows snapshot semantics.
- Initial layout has no all-rights cap and no ambient derive.
- All old independent CapabilityTable constructors and caller-supplied dispatch tables are absent from production wiring.

## 8. Explicit non-goals and blockers

- No change to frozen SC-005 or any other frozen standard.
- No second capability manager or temporary parallel CSpace.
- No generic rights adapter that silently maps READ/WRITE/EXEC/DELEGATE to every object type.
- No implementation of UntypedMemory/Retype before SC-001 boot-contract resolution.
- No claim that PR #111/#112 establish SC-005 or SC-007 conformance.
- No merge/release authorization in this document.

**Disposition:** Architecture direction D1–D2 is approved by ADR-0002. This migration contract is a technical implementation plan only. The CSpace implementation remains NOT IMPLEMENTED until the core authority replacement and all required caller migrations are committed and verified.
