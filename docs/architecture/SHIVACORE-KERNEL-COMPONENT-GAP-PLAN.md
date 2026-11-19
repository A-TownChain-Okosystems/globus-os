# ShivaCore Kernel Component Gap Plan

- Status: ARCHITECTURE PLAN — REVIEW REQUIRED
- Baseline repository: `A-TownChain-Okosystems/globus-os`
- Baseline commit: `f370b681e5c9ff49577f25c24f15b5fc880d5cba`
- Normative baseline: frozen SC-001, SC-003, SC-004, SC-005 and SC-007
- Related rights implementation: PR #114 (separate branch; not assumed merged)
- Scope: inventory, dependency order and acceptance gates; this document does not implement or amend normative standards.

## 1. Executive finding

The kernel contains useful partial implementations for capability bookkeeping, process management, physical-frame allocation, paging, syscalls, IPC and filesystem access. The inspected baseline does not establish conformance to the frozen capability and memory contracts.

The primary issue is not a lack of isolated modules. It is the absence of one enforced authority and complete resource lifecycles across existing modules. New components must be integrated into the canonical kernel implementation and must not create parallel managers or authorities.

## 2. Component inventory

Status meanings:
- PARTIAL: a related implementation exists, but the normative contract is not fully satisfied.
- BLOCKED: a critical prerequisite or contract mismatch prevents a conformant implementation.
- NOT ESTABLISHED: no sufficient implementation evidence was identified in the inspected baseline.
- DEPENDENT: implementation depends on an upstream component.

| ID | Component | Baseline status | Required outcome | Priority |
|---|---|---|---|---|
| K-01 | Single Capability Authority | BLOCKED | One kernel-owned authority root; no subsystem-owned mutable capability tables | P0 |
| K-02 | CSpace / CNode hierarchy | NOT ESTABLISHED | Typed CNode tree, bounded slots, lookup and authority-controlled slot operations | P0 |
| K-03 | Typed capability objects and rights | PARTIAL | ObjectType-bound RightsMask<u16>, strict rejection of foreign/reserved bits | P0 |
| K-04 | Capability derivation and attenuation | PARTIAL | Typed derivation, provenance, bounded authority and monotonic attenuation | P0 |
| K-05 | Capability revocation coordinator | PARTIAL | Coordinated descendant revocation and resource-lifecycle barriers | P0 |
| K-06 | Boot memory contract | BLOCKED | Resolve active boot path against SC-001 before claiming boot conformance | P0 |
| K-07 | UntypedMemory manager | NOT ESTABLISHED | Ownership-aware untyped regions and deterministic Retype accounting | P0 |
| K-08 | Typed object Retype engine | NOT ESTABLISHED | Validated object creation, failure atomicity, destruction and reclaim | P0 |
| K-09 | Frame capability manager | BLOCKED | Frame GRANT enforced by the central authority | P0 |
| K-10 | AddressSpace capability manager | BLOCKED | AddressSpace MAP/UNMAP enforced by the central authority | P0 |
| K-11 | Hardware VMM integration | PARTIAL | Preserve existing vmm.rs as sole hardware VMM; gate mutations by typed capabilities | P0 |
| K-12 | Production frame backend | NOT ESTABLISHED | Real-boot HHDM/frame backend with explicit ownership and evidence | P0 |
| K-13 | Global frame/mapping registry | NOT ESTABLISHED | Cross-AddressSpace mapping, reference and lease tracking | P0 |
| K-14 | TLB shootdown coordinator | NOT ESTABLISHED | Synchronous invalidation acknowledgements before frame reuse | P0 |
| K-15 | Page-fault lifecycle | PARTIAL | Suspend, authorized endpoint notification, validated resume or termination | P0 |
| K-16 | Typed syscall capability resolver | PARTIAL | Resolve handles through caller CSpace and validate type, object and operation | P1 |
| K-17 | Endpoint and notification authority | PARTIAL | SC-003 endpoint/badge and typed notification rights | P1 |
| K-18 | Process capability binding | PARTIAL | Process/thread ownership linked to authority-managed CSpace roots | P1 |
| K-19 | VFS/ATCFS capability binding | PARTIAL | Bind capability type, object identity and operation to the target resource | P1 |
| K-20 | Object destruction and sanitization | NOT ESTABLISHED | Revoke, unmap, drain references, sanitize and reclaim in defined order | P1 |
| K-21 | Device/IRQ capability managers | DEPENDENT | Typed ACCESS/SUBSCRIBE/ACK authority with hardware-specific validation | P2 |
| K-22 | Timer capability manager | DEPENDENT | Typed SET_DEADLINE/ACK operations | P2 |
| K-23 | Kernel audit/evidence integration | PARTIAL | Auditable authority transitions and SHA-bound verification evidence | P1 |

This is a gap inventory, not a claim that every named component must become a separate Rust module. Components may be implemented as cohesive submodules within the canonical authority or existing subsystem where that yields a smaller trusted computing base.

## 3. Canonical ownership and integration boundaries

- Canonical kernel implementation: `globus-os/modules/atc-shivacore/kernel/`.
- `capability.rs` is the current legacy capability implementation and is the migration target, not a second authority.
- `vmm.rs` remains the sole hardware VMM. Do not create a parallel page-table manager.
- `memory.rs` and the existing frame allocator are transitional until the SC-001 UntypedMemory/Retype contract is implemented.
- ProcessManager, MemorySubsystem, syscall, IPC, VFS and ATCFS are consumers. They must not own independent mutable capability roots.
- The `atc-shivacore` standards/governance repository is not the kernel implementation location.

## 4. Dependency-ordered implementation plan

### Phase 0 — Normative and source-baseline gate
1. Pin the exact SC-001/003/004/005/007 source commits and blobs.
2. Resolve the active `bootloader_api` path versus SC-001's frozen Limine B0–B5 contract through the required owner/governance process.
3. Keep the rights encoding decision in PR #113 and its implementation primitive in PR #114; do not treat either as merged unless GitHub confirms it.
4. Capture exact source commit, paths, symbols, blob hashes and test baseline.

Exit criteria: no unresolved normative contradiction is silently encoded in implementation.

### Phase 1 — Typed rights and central authority
1. Complete and verify PR #114's typed rights primitive.
2. Integrate the typed pair `(ObjectType, RightsMask<u16>)` into the existing canonical capability authority.
3. Replace or quarantine legacy `Rights(u8)` and generic `ResourceType` at the authority boundary; do not allow two production authority models.
4. Ensure all capability creation, lookup, delegation and invocation paths validate object type and allowed rights.
5. Add fail-closed tests for unknown types, reserved bits, cross-type masks, rights expansion and invalid operation/type combinations.

Exit criteria: one authority owns all mutable capability state; all consumers use typed validation.

### Phase 2 — CSpace and CNode
1. Implement authority-owned CSpace roots and CNode hierarchy.
2. Add bounded slot allocation, lookup and slot lifecycle.
3. Keep CNode DUPLICATE, MINT and MOVE as CNode-operation rights, never target-object rights.
4. Bind caller identity to its CSpace; prohibit ambient root access.
5. Test slot exhaustion, stale handles, invalid paths, unauthorized lookup and concurrent mutation.

Exit criteria: handles resolve only through the caller's authorized CSpace and authority-owned slot state.

### Phase 3 — Boot memory, UntypedMemory and Retype
1. Implement SC-001 memory classification only after the boot contract discrepancy is resolved.
2. Track Kernel-Control versus Untyped ownership explicitly.
3. Add UntypedMemory capabilities and deterministic Retype accounting.
4. Define atomic failure, destruction, sanitization and controlled reclaim semantics.
5. Add exhaustive range, alignment, overlap, exhaustion and ownership tests.

Exit criteria: no frame or typed object is allocated from an unowned or untyped-accounting-free physical range.

### Phase 4 — Frame and AddressSpace capabilities
1. Add Frame GRANT and AddressSpace MAP/UNMAP validation.
2. Integrate authorization into existing `vmm.rs` mutation entry points.
3. Preserve kernel-owned page tables and W^X.
4. Add a production frame backend and distinguish it from test-only backends.
5. Test mapping rights as a subset of frame rights and transactional rollback.

Exit criteria: no hardware mapping mutation is possible through an untyped raw-frame/raw-flags path.

### Phase 5 — Global revocation, TLB and reclaim
1. Track mappings and references globally across AddressSpaces.
2. Revoke relevant capabilities and prevent new use.
3. Unmap all affected mappings.
4. Wait for all required TLB shootdown acknowledgements.
5. Drain references/leases, sanitize and only then reclaim the frame.

Exit criteria: no frame is reused while a stale translation, capability or active reference can still reach it.

### Phase 6 — Fault, IPC and consumer migration
1. Implement page-fault context capture and thread suspension.
2. Notify only an authorized SC-003 fault endpoint.
3. Validate recovery authority before resume; otherwise terminate safely.
4. Migrate ProcessManager, MemorySubsystem, syscall, IPC, VFS and ATCFS to the central authority.
5. Remove consumer-owned capability tables after all call sites are migrated.

Exit criteria: all kernel resource access is mediated by the same authority and typed contracts.

### Phase 7 — Hardware and security verification
1. Unit and property tests for rights, CSpace, derivation, revocation and object lifecycle.
2. Integration tests for Retype → Frame → MAP → UNMAP → TLB → reclaim.
3. Fault/IPC tests including unauthorized resume and endpoint failure.
4. Real-boot tests for the resolved SC-001 contract.
5. Exact-head CI, job/step/log evidence, artifact SHA-256 and E2E evidence.

Exit criteria: each claim is assigned IMPLEMENTED, TESTED, CI_VERIFIED, E2E_VERIFIED, VERIFIED, RESIDUAL or BLOCKED with supporting evidence. Unknown remains BLOCKED.

## 5. Mandatory security invariants

- Exactly one kernel-owned mutable capability authority.
- No ambient authority or subsystem-owned mutable capability tables.
- Rights are object-type-local and validated together with ObjectType.
- Unknown object types and reserved/foreign rights bits fail closed.
- Delegation never expands rights.
- Mapping rights never exceed Frame capability rights.
- W^X is enforced at every mapping path.
- Page tables remain kernel-owned.
- Revocation prevents new use before teardown.
- Frame reclaim waits for unmaps, TLB acknowledgements and reference/lease completion.
- Page-fault resume requires explicit authorization.
- No frozen normative specification is modified by implementation PRs.

## 6. Explicit non-goals

This plan does not authorize:
- a second CapabilityAuthority, allocator or hardware VMM;
- changes to frozen SC-001, SC-003, SC-004, SC-005 or SC-007;
- G7 binding, external rights serialization, persistent rights encoding, syscall ABI changes or ABI freeze;
- weakening CI, security or governance gates;
- merge, release or VERIFIED claims without exact evidence.

## 7. Current implementation boundary

PR #114 is the typed-rights primitive and remains a separate, draft implementation PR. It does not by itself migrate `CapabilityTable`, implement CSpace/CNode, resolve the boot contract discrepancy, or establish kernel conformance.

The next code implementation must be a dependent, narrowly scoped change that integrates the approved typed rights into the existing capability authority. CSpace implementation follows only after that authority boundary is established.
