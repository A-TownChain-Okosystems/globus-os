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
- Implement a service-space adapter for `NetworkStack` and reconcile its behavior with
  ATS-1000; do not move transport implementation into the ABI crate.
- Replace or wrap the existing `security::TokenBucket` only after matching its actual
  semantics and call sites with compatibility tests.
- Implement a read-only `KernelBootStatusProvider` adapter that maps internal boot states
  without exposing `KernelState`.
- Run formatting, workspace checks, targeted tests, and all required PR gates against the
  exact final head. Until then, status remains **VERIFICATION PENDING**.
