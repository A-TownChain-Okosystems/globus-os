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
   - Exposes ephemeral GUA1 and GCR1 snapshots directly from encrypted state; no plaintext sidecar is persisted.
   - Provides generation-bound `insert_account`, `replace_identity`, and `remove_account` operations. Credential rotation cannot change the stable UserId or profile binding.
   - `MultiAccountIdentityService` now orchestrates repository-backed registration, restore, username login, credential rotation, and account removal. It updates its in-memory coordinator only after successful durable commits.
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

For canonical repository exports, the GCR1 file can be passed as one already-validated catalog instead of individual ATID files:

```bash
cargo run -p globus-identity --example build-boot-catalog -- \\
  /run/identity/accounts.gua \\
  /run/boot/sys.accounts \\
  /run/boot/sys.credentials \\
  --gcr1=/run/identity/credentials.gcr
```

The `AccountStateRepository::export_credential_snapshot()` API produces that ephemeral GCR1 projection. The packer's `--gcr1` mode validates the GUA1/GCR1 one-to-one binding before publishing either output file. This is implemented at the packaging boundary; production orchestration from the live repository to the image build remains a separate integration step.

## Remaining M1 blockers

**M1 is not production-complete.** The storage primitive and the boot projection are implemented, but the production lifecycle is not connected end-to-end.

1. `IdentityService` currently creates and stores Argon2id PHC strings in `IdentityRecord`.
2. The kernel verifier consumes fixed 64-byte `ATID` records through `globus-identity-core`.
3. `AccountStateRepository` persists the existing PHC representation and now exposes an ephemeral GCR1 export. Conversion is limited to PHC Argon2id v19 records with supported ATID parameters, a 16-byte salt, and a 32-byte output; salt and hash are copied without re-hashing. Unsupported records fail closed. CI verifier-equivalence tests are still required before calling this migration verified.
4. `AccountStateRepository` exposes generation-bound atomic account insertion, identity replacement (profile/UserId binding preserved), and account removal. `MultiAccountIdentityService` orchestrates registration, restore, username-bound login, credential rotation, and removal through these commits. The returned session remains an in-process userspace object, not the kernel-owned session required by M3.
5. A production ShivaCore key-provider binding and target-filesystem reopen/crash tests are not yet evidenced.
6. The example packer accepts both per-account ATID files and a validated GCR1 snapshot via `--gcr1`. The repository export API and packaging path exist; automated production orchestration and signed boot-catalogue provenance remain open.
7. GUA1/GCR1 are not signed. The Ramdisk must be treated as trusted input until secure-boot signature verification is separately demonstrated.

Never reinterpret PHC bytes as ATID or introduce a second verifier. The converter parses the PHC structure, requires Argon2id v19 with the canonical owner KDF profile, and copies only validated salt/hash fields into ATID without re-hashing. `IdentityService` and the kernel both verify through `globus-identity-core`. Conversion is not considered verified until the generated ATID record passes same-password and wrong-password tests on the current CI head.

## M2 username-bound authentication

The existing `IdentityCheck = 0x0070` syscall number is unchanged; ABI minor is 2.1. Multi-account images require the versioned `LGN1` payload containing both canonical username and password. The kernel resolves the username through the published GUA1/GCR1 catalog and verifies only the corresponding ATID record through `globus-identity-core`. The legacy password-only payload is accepted only when the boot path loaded the explicit singleton `sys.identity` compatibility record.

Unknown users are processed with a dummy Argon2id operation and return the same credential-failure result as a wrong password. The Greeter displays one generic failure message. Password and copied payload buffers are cleared after verification. This source implementation is not yet CI_VERIFIED; malformed-pointer, malformed-payload, and cross-account tests must pass on the exact current HEAD.

Successful credential verification still does not create a kernel-owned session or launch a shell. Those are M3 and M4 gates, respectively.

## Acceptance status

- [x] Shared no_std GUA1 format.
- [x] Shared no_std GCR1 format for existing ATID records.
- [x] Exact one-to-one GUA1/GCR1 cross-validation.
- [x] Kernel-only `sys.accounts` and `sys.credentials` entries.
- [x] Atomic kernel publication and fail-closed partial/mixed-image handling.
- [x] Versioned encrypted account-state repository primitive.
- [x] Read-only AccountDirectoryStore projection over AccountState.
- [x] Ephemeral GUA1 export from AccountStateRepository.
- [x] Strict PHC Argon2id v19 → ATID conversion and GCR1 snapshot API (implementation present; CI verification pending).
- [ ] Production key-provider integration and filesystem crash/reopen tests.
- [x] Ephemeral GCR1 export from `AccountStateRepository` (CI verification pending).
- [ ] Two-account BIOS and UEFI QEMU test reaching account-specific login verification.
- [ ] Exact-HEAD Actions and artifact SHA-256 evidence.

**Status:** IMPLEMENTED (encrypted repository primitive, generation-bound account mutation operations, repository-backed multi-account lifecycle orchestration, shared credential verifier adapter, PHC→ATID export, GUA1/GCR1 codecs, kernel loader, packaging boundary, username-bound LGN1 authentication source path); RESIDUAL/BLOCKED (production key custody, signed catalogue provenance, kernel-owned session/process transition, rate limiting, QEMU). No TESTED / CI_VERIFIED / E2E_VERIFIED claim until exact-HEAD workflow and BIOS/UEFI evidence is available.
