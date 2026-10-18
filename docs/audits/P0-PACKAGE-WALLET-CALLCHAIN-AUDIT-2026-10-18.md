# GlobusOS P0 Security Call-Chain Audit

- Date: 2026-10-18
- Scope: package install/update/recovery verification and wallet IPC/capability enforcement
- Status: PARTIAL / BLOCKED FOR RELEASE
- Audit base: stacked on `fix/p0-package-cryptographic-verification` at `723b78aeaac92bf92824b62186c5f1bf827df895`

## Executive findings

1. The package verifier now hashes the supplied artifact bytes, verifies Ed25519 metadata against the configured keyring, rejects revoked keys, and returns a `VerifiedArtifact` carrying the exact bytes passed to verification. Tests cover exact-byte preservation, digest mismatch, untrusted/revoked keys, malformed metadata, metadata tampering, and invalid signatures.
2. `system/update/src/staging.rs` now defines a fail-closed staging gate that accepts only `VerifiedArtifact`, writes its byte slice to the inactive slot, reads the slot back, compares every byte, and activates only after an exact match. Negative tests cover write failure, readback tampering, and attempts to overwrite the active slot. However, the audited tree still has no concrete production implementation of `UpdateSlotStorage` bound to real disk/partition and boot-selection operations. End-to-end installation/activation is therefore NOT proven.
3. The visible filesystem recovery code at `system/vfs/src/recovery.rs` replays journaled block writes. It does not parse package artifacts or call the package verifier. No package recovery/rollback consumer joining these paths was identified in the audited tree.
4. Wallet signing previously accepted caller-created `Grant` values and authorized only by the declared right. The correction removes that trust model: wallet methods accept a capability identifier and require it to resolve in a trusted `CapabilityRegistry`, bound to the wallet-signing resource, with the requested right and no revocation.
5. ShivaCore's generic IPC/syscall path has kernel-side capability checks in `modules/atc-shivacore/kernel/src/ipc.rs` and `modules/atc-shivacore/kernel/src/syscall.rs`. The audit did not find a concrete adapter connecting the typed `WalletSignMessage` service entry point to that kernel path. Do not treat generic IPC enforcement as proof of end-to-end wallet IPC enforcement until that adapter is traced and tested.

## Call-chain inventory

### Package / update / recovery

- Package metadata and byte verification: `system/package/src/lib.rs`
- Update verification wrapper and state model: `system/update/src/lib.rs`
- Verified-byte staging contract and in-memory negative tests: `system/update/src/staging.rs`
- Update boot-state/health/transition modules: `system/update/src/boot_state.rs`, `system/update/src/health.rs`, `system/update/src/transition.rs`
- Filesystem journal recovery: `system/vfs/src/recovery.rs`
- Concrete installer/stager and package rollback consumer: NOT FOUND in the audited Rust workspace paths.

Required release gate: implement a concrete `UpdateSlotStorage` adapter over the real inactive-slot write/readback and atomic boot-selection mechanisms. Keep raw bytes out of that API; accept only `VerifiedArtifact`; write only `artifact.bytes()`; do not reopen/re-fetch the source after verification; compare readback byte-for-byte before activation. Repeat verification after any transformation/delta application. Add fault-injection tests against the real adapter and show recovery/rollback cannot activate an unverified slot.

### Wallet / capability / IPC

- Typed service boundary: `system/wallet-service/src/lib.rs` (`sign_ipc` → `sign` → keystore)
- Capability registry: `system/security/src/capability_registry.rs`
- Shared authorization API: `system/security/src/lib.rs`
- Generic kernel IPC capability checks: `modules/atc-shivacore/kernel/src/ipc.rs`
- Kernel syscall capability checks: `modules/atc-shivacore/kernel/src/syscall.rs`

The service now resolves the supplied identifier in its injected trusted registry and checks the wallet-signing resource and right. Unknown, wrong-right, wrong-object, and revoked capabilities must be denied. The registry must be provisioned only by a privileged security/kernel integration; it must never be accepted from the untrusted IPC payload.

## Required negative tests

- Artifact modified after signing or between verification and staging: reject / staged bytes unchanged.
- Invalid Ed25519 signature, malformed signature encoding, invalid digest and digest mismatch: reject.
- Untrusted and revoked signing keys: reject, including attempted re-trust after revocation.
- Unknown capability ID, caller-invented capability ID, wrong right, wrong resource binding and revoked capability: reject.
- Wallet IPC identity mismatch: reject.
- Update staging and rollback/recovery must never activate bytes other than the exact verified artifact.

## CI and readiness

This audit report is not a CI result. Fresh workflow results must be read from the exact head commit of the draft PR. Release remains blocked until the package installer/update/recovery consumers and the wallet-to-kernel IPC adapter are demonstrated and covered by tests.
