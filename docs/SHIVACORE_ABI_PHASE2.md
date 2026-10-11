# ShivaCore Stable ABI — Phase 2 Contracts

Status: **IMPLEMENTED IN DRAFT / VERIFICATION PENDING**

Target crate: `system/shivacore-api` (`libshivacore`).
Implementation branch: `feat/shivacore-abi-contracts-phase2`.

## Design boundary

This phase defines service-facing contracts. It does not export kernel-owned
structures, pointers, the capability table, the network implementation, or
`KernelState`. Service-space code must not gain ambient authority from these
interfaces. Capability checks and rate limiting are separate controls.

## Contracts

### Capability authority

- `CapabilityCheckRequest` contains only an opaque `CapabilityHandle` and required rights.
- `CapabilityCheckResult` reports whether the request is granted and the effective
  rights. The implementation must fail closed on unknown handles, invalid bits,
  revoked capabilities, and internal errors.
- `CapabilityResourceType` assigns explicit numeric identifiers to resource classes.
- The ABI contract does not expose `CapabilityTable`, `Capability`, `CapId`, or
  kernel process identifiers.

### Network stack

- `NetworkStack` defines connect/send/receive operations at the boundary.
- Peer identity is a fixed 32-byte identifier; payloads are passed as borrowed slices.
- Implementations must enforce configured message bounds, validate connection handles,
  and return `AbiError` rather than panic on untrusted input.
- This trait is an interface only; it does not claim that ATS-1000 networking is
  implemented or production-ready.

### Token bucket

- `TokenBucketConfig` fixes capacity and refill rate in tokens per monotonic tick.
- `TokenBucket` implements deterministic integer refill and consumption with saturating
  arithmetic, a hard capacity ceiling, and no refill when the supplied clock moves
  backwards.
- Insufficient tokens return `AbiError::ResourceExhausted`.
- Rate limiting is not authorization; callers must independently validate capabilities.

### Kernel boot status

- `KernelBootStatusProvider` is read-only and exposes phase, state, and optional ABI error.
- Services can observe readiness but cannot call a boot function or access `KernelState`.
- The ABI status is an observation contract, not evidence that every boot phase is
  verified or that boot is production-ready.

## Compatibility and safety rules

1. Numeric identifiers and `repr` layouts are ABI. Do not reorder or reuse published values.
2. New optional behavior is additive within the same major ABI; incompatible changes
   require a major version increment and explicit migration.
3. Do not export aliases to kernel-private concrete types to make a build pass.
4. Treat every handle and request as untrusted. Reject unknown values and fail closed.
5. Never use `simple_hash` as a cryptographic digest, MAC, signature, or security boundary.

## Tests added

The contract module includes tests for capability request representation, deterministic
token consumption/refill, capacity enforcement, backwards-time behavior, and boot-status
observation. These tests are source-level evidence only until the exact commit's CI
results and test logs are inspected.

## Remaining integration work

- Implement a kernel-owned adapter from `CapabilityTable` to `CapabilityAuthority`
  with revoked/unknown-handle and rights-attenuation tests.
- Integrate the new ATS-1000-to-ABI `NetworkStack` adapter with the service-space transport owner; the adapter normalizes failures and enforces the ABI payload ceiling, but does not implement transport.
- The existing `security::TokenBucket` remains a separate legacy API. Its refill arithmetic is now overflow-safe and preserves fractional-second remainder; it has edge-case tests, but no ABI migration or full call-site parity claim is made.
- Implement a read-only `KernelBootStatusProvider` adapter that maps internal boot states
  without exposing `KernelState`.
- Run formatting, workspace checks, targeted tests, and all required PR gates against the
  exact final head. Until then, status remains **VERIFICATION PENDING**.

## Kernel adapter implemented in this phase

`modules/atc-shivacore/kernel/src/abi_adapter.rs` implements
`KernelCapabilityAuthority`, bound to the caller PID supplied by trusted syscall
dispatch. It checks ownership before evaluating rights, rejects unknown handles and
unknown rights bits, and returns only the public ABI result. Tests cover owned access,
foreign-owner denial, unknown handles, and invalid rights bits.

This adapter does not make caller identity a request field: caller identity is trusted
context, not data supplied by an untrusted service. Integration into each syscall path
must preserve that invariant.


## Network adapter and rate-limiter hardening update

The kernel adapter module now includes `KernelNetworkAdapter`, which translates the
legacy ATS-1000 `Option`/`bool` results into explicit `AbiError` values, applies
`MAX_SYSCALL_PAYLOAD` (or a stricter configured ceiling) before dispatch, and rejects
transport-reported receive lengths larger than the destination buffer. Tests cover
legacy failure mapping, successful connect/send/receive, pre-dispatch size rejection,
and an invalid transport length. This is an adapter over an injected stack, not proof
that a real network transport exists.

The existing security rate limiter was hardened separately: refill multiplication and
token addition saturate, backwards time does not refill, and elapsed whole seconds are
accounted without discarding a fractional-second remainder. New tests cover overflow,
fractional refill, and backwards-clock behavior. It is deliberately not described as
migrated to the stable ABI.

Remaining integration blockers are still real: the adapter must be wired to the actual
service-space transport owner, the read-only boot-status provider is not yet backed by
kernel lifecycle state, capability checks are not yet routed through all syscall entry
points, and final-head CI logs have not been verified.
