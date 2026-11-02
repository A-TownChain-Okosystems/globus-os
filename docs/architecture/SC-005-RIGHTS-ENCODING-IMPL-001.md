# SC-005 Rights Encoding — Implementation Decision IMPL-001

**Identifier:** SC-005-RIGHTS-ENCODING-IMPL-001  
**Version:** 1.0.0  
**Status:** Proposed implementation decision — review required before implementation constants or binary/G7 binding  
**Scope:** GlobusOS kernel implementation of frozen SC-005 v0.1.0  
**Related contract:** [SC-005 CSpace Migration Contract](./SC-005-CSPACE-MIGRATION-CONTRACT.md)  
**Frozen normative source:** `atc-shivacore@3b56253b39bdc6cbf719ed9a2a21461d73b8fc34:docs/specs/SC-005-CAPABILITY_SYSTEM.md`  
**Frozen source blob:** `2e19216b43237fcbd6e175a7abf15c1781cde4ca`

## 1. Decision summary

This document proposes a deterministic, reversible, object-type-local encoding for the 16-bit rights mask required by SC-005 REQ-SC005-03/04.

The encoded value is interpreted only together with its exact `ObjectType`. The canonical semantic identity is:

`(ObjectType, RightsMask<u16>)`

Bit positions are intentionally reusable between different object types. They are not globally meaningful. This avoids inventing a global rights hierarchy that SC-005 does not define and preserves object-specific validation.

**This is an implementation-level proposal, not a normative amendment to SC-005, SC-003, SC-004, or SC-007.** It does not authorize a G7 binding, external serialization contract, or ABI freeze. Those require explicit review and approval before the first binary binding.

## 2. Normative constraints inherited from frozen SC-005

SC-005 v0.1.0 is frozen. This proposal preserves, and does not redefine:

- 16-bit rights-mask width (REQ-SC005-03).
- Exact rights sets per object type (REQ-SC005-04).
- Any unknown or foreign bit is invalid and returns `INVALID_CAP` (REQ-SC005-04, INV-06, C-E07).
- Derivation may only attenuate rights; it may never add rights (REQ-SC005-05/06, INV-03).
- CNode `DUPLICATE`, `MINT`, and `MOVE` remain distinct CNode-operation rights and are never target-object rights.
- Endpoint badge semantics remain independent of rights encoding.
- SC-004 slot resolution, invoke snapshot, and sysret semantics remain unchanged.

The source specification defines named rights and mask width but assigns no numeric bit positions. The mapping below is therefore an explicit, reviewable implementation decision, not an inference from the order of the normative table.

## 3. Proposed v1 encoding map

For each object type, the first listed right is bit 0, the next is bit 1, and so on. The table is the authoritative mapping; implementations MUST NOT derive positions from source declaration order, enum discriminants, hash order, or generated documentation order.

| ObjectType | Right | Bit | Mask |
|---|---|---:|---:|
| Endpoint | SEND | 0 | 0x0001 |
| Endpoint | RECV | 1 | 0x0002 |
| Endpoint | DELEGATE | 2 | 0x0004 |
| Endpoint | NOTIFY | 3 | 0x0008 |
| Notification | SIGNAL | 0 | 0x0001 |
| Notification | WAIT | 1 | 0x0002 |
| Frame | READ | 0 | 0x0001 |
| Frame | WRITE | 1 | 0x0002 |
| Frame | EXECUTE | 2 | 0x0004 |
| Frame | GRANT | 3 | 0x0008 |
| AddressSpace | MAP | 0 | 0x0001 |
| AddressSpace | UNMAP | 1 | 0x0002 |
| AddressSpace | SWITCH | 2 | 0x0004 |
| Thread | SUSPEND | 0 | 0x0001 |
| Thread | RESUME | 1 | 0x0002 |
| Thread | CONFIG | 2 | 0x0004 |
| CNode | DUPLICATE | 0 | 0x0001 |
| CNode | MINT | 1 | 0x0002 |
| CNode | MOVE | 2 | 0x0004 |
| UntypedMemory | RETYPE | 0 | 0x0001 |
| Timer | SET_DEADLINE | 0 | 0x0001 |
| Timer | ACK | 1 | 0x0002 |
| Device/IRQ | ACCESS | 0 | 0x0001 |
| Device/IRQ | SUBSCRIBE | 1 | 0x0002 |
| Device/IRQ | ACK | 2 | 0x0004 |

All unassigned bits are reserved and MUST be zero in v1. In particular, the current defined sets use no bit above bit 3.

### 3.1 Why bit positions are object-type-local

SC-005 defines independent rights sets for each object type, not one global union. For example, `0x0001` means Endpoint SEND, Notification SIGNAL, Frame READ, AddressSpace MAP, Thread SUSPEND, CNode DUPLICATE, UntypedMemory RETYPE, Timer SET_DEADLINE, or Device/IRQ ACCESS depending on the validated object type.

A mask MUST NEVER be interpreted without its object type. Code MUST NOT compare, union, intersect, attenuate, serialize, or authorize rights from different object types as if they belonged to one namespace.

## 4. Validation and decoding contract

Validation MUST be fail-closed and MUST occur before object dispatch:

1. Validate that the object type is a recognized SC-005 type.
2. Obtain that type's explicit v1 allowed mask.
3. Reject if `raw_mask & !allowed_mask != 0`; return `INVALID_CAP`.
4. Only after validation, construct the typed rights value.
5. For an operation, verify that every requested right is present in the validated mask; otherwise return `AUTH_DENIED` with no state change.

The implementation MUST NOT use truncating conversion such as `from_bits_truncate()`, masking away unknown bits, fallback-to-empty behavior, or permissive deserialization.

The encode/decode functions MUST be total over valid typed values and round-trip exactly:

- `decode(ObjectType, encode(ObjectType, rights)) == rights`
- `encode(ObjectType, decode(ObjectType, mask)) == mask` for every valid mask.
- A mask containing any reserved or foreign bit MUST fail decoding; it MUST NOT be normalized.

A suitable semantic API is a typed wrapper such as `ObjectRights { object_type, bits: u16 }` or an equivalent type-safe representation. A bare public `Rights(u16)` value MUST NOT cross an authority boundary without an associated object type and validation.

## 5. Attenuation and operation separation

- Rights attenuation is permitted only within the same object type.
- A child mask MUST be a subset of its validated parent mask: `child & !parent == 0`.
- A requested right from another object type is invalid, even if its numeric bit happens to match a permitted bit in the parent mask.
- CNode `MINT`, `DUPLICATE`, and `MOVE` authorize CNode operations only. They are not target-object rights.
- Endpoint `DELEGATE` remains the Endpoint right defined by SC-003; it MUST NOT substitute for CNode `MINT` or `DUPLICATE`.
- Endpoint mint badge assignment and immutable badge propagation remain governed by SC-005 §6 and SC-003.

## 6. Compatibility and versioning

- Decision identifier: `SC-005-RIGHTS-ENCODING-IMPL-001`.
- Encoding revision: `v1`.
- The pair `(ObjectType, u16 mask)` is the only canonical typed representation.
- No standalone numeric mask is a globally portable capability-rights value.
- This document does not define a wire format, G7 register layout, syscall opcode, persistent storage format, or cross-process/network token.
- Before G7 or any binary/external binding, the owner must explicitly approve this mapping and the exact type-tag representation. Once bound, changes require a versioned compatibility decision; silent bit reassignment is forbidden.
- A future rights addition must identify the object type, allocate an unused bit within that type, update the allowed mask, and add positive and negative conformance tests. It must not reinterpret any previously assigned bit.
- If a type exhausts the 16-bit namespace, implementation MUST stop and request a versioned standards/architecture decision; it MUST NOT widen or alias the mask locally.

## 7. Required tests

### Positive tests
- Every named right maps to exactly the bit shown in §3.
- Every allowed singleton and valid combination round-trips for its object type.
- Each object type accepts all and only its listed rights.
- Attenuation within one object type preserves the subset invariant.
- Same numeric bit values decode to the correct, different semantic right when paired with different object types.

### Negative tests
- Every unassigned bit is rejected for every object type.
- A bit valid for another type but invalid for the supplied type is rejected.
- Unknown object types are rejected.
- Cross-object-type attenuation and rights comparison are rejected at the API boundary.
- CNode MINT/DUPLICATE/MOVE are not accepted as Endpoint, Frame, or other target-object rights.
- No parser or constructor silently truncates, masks, or normalizes an invalid value.
- Rejected values leave source capabilities, destination slots, derivation metadata, and object state unchanged.

Tests MUST assert exact error classes and failure atomicity, not merely that an operation returns an error.

## 8. Implementation and evidence gates

This proposal is not implementation evidence. Before kernel code adopts the mapping:

1. Review this mapping against the exact frozen SC-005 blob cited above.
2. Record owner approval of the mapping and the typed representation.
3. Implement strict typed validation in the existing canonical capability authority; do not add a parallel rights authority.
4. Add the positive and negative tests in §7.
5. Verify the exact PR head and retain Actions Run → Job → Step/log evidence.
6. Keep G7, external serialization, and ABI binding blocked until their separate explicit approval.

No frozen standard is changed by this document. No claim of CSpace implementation, CI verification, E2E verification, or release readiness is made.

## 9. Decision status

**PROPOSED — REVIEW REQUIRED.**

The authoring work is authorized for PR #113. The numeric mapping is recorded here as a concrete candidate for review. It MUST NOT be treated as owner-approved ABI or consumed by production kernel constants until the explicit review gate in §8, item 2, is satisfied.
