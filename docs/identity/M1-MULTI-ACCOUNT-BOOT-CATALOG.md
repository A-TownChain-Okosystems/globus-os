# M1 — Multi-account boot catalogue

**PR:** #125  
**Branch:** `feature/multi-account-login-session`  
**Scope:** export the canonical identity directory and the existing ATID verifier records as two kernel-protected Ramdisk entries. This is a boot-time projection, not a second identity database.

## Canonical ownership

- `AccountStateRepository` is the target single encrypted persistence authority for usernames, stable UserIds, profiles, and credential records. It uses the existing `EncryptedBlobStore` + `FileBlobStore` boundary.
- `AccountDirectoryStore` is a read-only projection trait. `AccountState` implements it directly. `FileAccountDirectory` and `MutableAccountDirectoryStore` remain a legacy import/migration path only; they must not be a second production source of truth.
- `GUA1` remains the canonical username → UserId directory encoding. Its shared parser/encoder lives in `globus-identity-core` (`no_std`).
- `GCR1` is a boot-only credential collection containing existing 64-byte `ATID` records keyed by full UserId. It is not persistent account storage.
- The kernel requires both `sys.accounts` (GUA1) and `sys.credentials` (GCR1). Both names fit the existing 16-byte SRAM entry-name field. `sys.*` remains kernel-only.
- The legacy singleton `sys.identity` is accepted only as a compatibility case when neither multi-account entry is present. Any partial or mixed multi-account/legacy set fails closed.

## GUA1 directory format

The existing GUA1 format is unchanged: magic `GUA1`, version 1, big-endian u32 account count, then canonical lowercase username and stable UserId length/value pairs. Entries are sorted by username. Duplicate usernames, duplicate UserIds, invalid UTF-8, non-canonical usernames, invalid lengths, truncation, and trailing bytes are rejected.

## GCR1 credential format

GCR1 uses big-endian fields:

| Field | Encoding |
|---|---|
| Magic | `GCR1` (4 bytes) |
| Version | u16 BE, currently 1 |
| Record count | u16 BE |
| Each entry | UserId length u16 BE, UserId UTF-8 bytes, exactly 64 ATID bytes |

Entries are sorted by full UserId. Duplicate IDs, malformed ATID records, unsupported versions, truncation, and trailing bytes are rejected.

## Cross-catalogue validation

The shared `globus-identity-core::catalog::parse_boot_catalog(gua1, gcr1)` parser validates both entries as one set:
- each GUA1 UserId has exactly one GCR1 credential;
- no GCR1 credential is orphaned;
- no duplicate username or UserId exists;
- each ATID record passes the canonical identity-core parser;
- the complete validated snapshot is published atomically by the kernel.

The kernel does not publish a partial account set. A malformed entry clears the previously published catalogue and leaves multi-account authentication unavailable.

## Implementation delivered

1. `system/identity-core/src/catalog.rs`
   - Shared no_std GUA1 parser/encoder.
   - GCR1 parser/encoder and strict one-to-one cross-validation.
2. `system/identity/src/account_registry.rs`
   - `AccountDirectoryStore` is read-only.
   - `AccountState` exposes the username/UserId mapping as a projection.
   - The standalone GUA1 writer is separated as `MutableAccountDirectoryStore` for legacy migration only.
3. `system/identity/src/account_state.rs`
   - Versioned `GAS1` account-state codec and encrypted repository over the existing encrypted blob boundary.
   - Username mapping and IdentityRecord are committed in one encrypted state object.
   - Exposes an ephemeral GUA1 snapshot directly from the encrypted state; no plaintext sidecar is persisted.
4. `system/identity/src/file_store.rs` and `secure_store.rs`
   - File writers use a fail-closed lock.
   - Generation advancement is serialized and encrypted commits can be bound to an expected generation.
5. `system/identity/src/boot_catalog.rs`
   - Builds the two protected Ramdisk projections and validates their cross-reference before packaging.
6. `system/identity/examples/build-boot-catalog.rs`
   - Packages an ephemeral GUA1 snapshot plus per-account ATID files into `sys.accounts` and `sys.credentials`.
7. `modules/atc-shivacore/kernel/src/identity_catalog.rs`
   - Parses both protected entries through the shared no_std codec and publishes atomically.
8. `modules/atc-shivacore/kernel/src/main.rs`
   - Requires both entries together; rejects partial or mixed legacy/multi-account images.
9. `scripts/qemu_boot.sh`
   - Accepts explicit `GLOBUS_ACCOUNT_DIRECTORY` and `GLOBUS_CREDENTIAL_CATALOG` inputs.

## Host packaging

The GUA1 input must be an ephemeral export from `AccountStateRepository::export_directory_snapshot()`. The packer also requires one existing ATID record per UserId:

```bash
cargo run -p globus-identity --example build-boot-catalog -- \
  /run/identity/accounts.gua \
  /run/boot/sys.accounts \
  /run/boot/sys.credentials \
  user-alice=/trusted/credentials/alice.atid \
  user-bob=/trusted/credentials/bob.atid
```

Boot with:

```bash
GLOBUS_ACCOUNT_DIRECTORY=/run/boot/sys.accounts \
GLOBUS_CREDENTIAL_CATALOG=/run/boot/sys.credentials \
scripts/qemu_boot.sh
```

The packaging tool does not create credentials, derive wallet keys, or mutate persistent identity state.

## Remaining M1 blockers

**M1 is not production-complete.** The storage primitive and the boot projection are implemented, but the production lifecycle is not connected end-to-end.

1. `IdentityService` currently creates and stores Argon2id PHC strings in `IdentityRecord`.
2. The kernel verifier consumes fixed 64-byte `ATID` records through `globus-identity-core`.
3. `AccountStateRepository` currently persists the PHC representation. It can export GUA1, but cannot export valid ATID credentials without a reviewed migration or credential re-entry ceremony.
4. Registration, password changes, account deletion, and recovery are not yet wired to atomic `AccountStateRepository` commits.
5. A production ShivaCore key-provider binding and target-filesystem reopen/crash tests are not yet evidenced.
6. The boot packer still receives ATID files as external inputs. It is transitional packaging, not proof that production account state is the source of all boot credentials.
7. GUA1/GCR1 are not signed. The Ramdisk must be treated as trusted input until secure-boot signature verification is separately demonstrated.

Never reinterpret PHC bytes as ATID, derive ATID from a PHC string, or introduce a second verifier. The PHC/ATID migration must be explicit, reviewed, and tested.

## Acceptance status

- [x] Shared no_std GUA1 format.
- [x] Shared no_std GCR1 format for existing ATID records.
- [x] Exact one-to-one GUA1/GCR1 cross-validation.
- [x] Kernel-only `sys.accounts` and `sys.credentials` entries.
- [x] Atomic kernel publication and fail-closed partial/mixed-image handling.
- [x] Versioned encrypted account-state repository primitive.
- [x] Read-only AccountDirectoryStore projection over AccountState.
- [x] Ephemeral GUA1 export from AccountStateRepository.
- [ ] Credential migration / one normative PHC-or-ATID representation.
- [ ] Production account lifecycle wired to AccountStateRepository.
- [ ] Production key-provider integration and filesystem crash/reopen tests.
- [ ] Direct ATID/GCR1 export from the canonical encrypted repository.
- [ ] Two-account BIOS and UEFI QEMU test reaching account-specific login verification.
- [ ] Exact-HEAD Actions and artifact SHA-256 evidence.

**Status:** IMPLEMENTED (repository primitive, GUA1/GCR1 codecs, kernel loader, packaging boundary); RESIDUAL/BLOCKED (credential migration, lifecycle integration, production key custody, QEMU). Not TESTED, CI_VERIFIED, or E2E_VERIFIED until current-head workflow evidence is available.
