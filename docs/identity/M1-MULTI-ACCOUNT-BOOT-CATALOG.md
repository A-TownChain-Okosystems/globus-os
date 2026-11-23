# M1 — Multi-account boot catalog

**PR:** #125  
**Branch:** `feature/multi-account-login-session`  
**Status:** catalog codec/loader IMPLEMENTED; production credential-source integration RESIDUAL/BLOCKED; no E2E claim.

## Canonical storage ownership

- `AccountStateRepository` is the intended single encrypted persistence boundary for complete multi-account state. It reuses `EncryptedBlobStore` and the existing `FileBlobStore`; it does not create a second account database.
- `AccountState` owns username, stable `UserId`, profile and the current identity credential record in one canonical state object.
- `AccountDirectoryStore` is now a read-only directory view. `AccountState` implements it. Mutations belong to the account-state transaction, not to a detached username index.
- `MutableAccountDirectoryStore` and `FileAccountDirectory` remain legacy standalone GUA1 writer compatibility only; they must not be the production account authority.
- `globus-identity-core` owns the shared no_std codecs and the existing fixed-size `ATID` verifier used by the kernel.

## Ramdisk contract: two reserved kernel-only entries

The kernel receives two independent entries in the existing `SRAM` Ramdisk. The pair is validated as one unit before any account is published.

### `sys.accounts` — GUA1

The existing GUA1 encoding is unchanged: magic `GUA1`, version byte, entry count, then username length/name and UserId length/UserId. The encoder/parser is shared through `globus-identity-core`; `system/identity` delegates to it.

This entry is an ephemeral directory projection exported from `AccountState`, not another persistent directory file.

### `sys.credentials` — GCR1

GCR1 is a versioned collection of the existing 64-byte ATID credential records, keyed by the full stable UserId.

Header: magic `GCR1`, version u16 BE = 1, count u16 BE. Each record: UserId length u16 BE, full UserId UTF-8 bytes, then exactly 64 bytes of the existing ATID record.

The GCR1 encoder sorts by UserId and rejects duplicate IDs or invalid ATID records. The kernel parser rejects unknown versions, invalid lengths, malformed records, duplicate or unsorted IDs, truncation and trailing bytes.

### Cross-entry invariant

The kernel parses both entries, requires equal non-zero account counts, and validates an exact one-to-one mapping from every GUA1 UserId to exactly one GCR1 ATID record. Missing credentials and orphaned credential records reject the complete set. No partial account set is published.

The namespace rule remains unchanged: only `Reader::Kernel` can read `sys.*`; Service and Program readers see neither entry.

## Implementation delivered

1. `system/identity-core/src/catalog.rs`
   - Shared no_std GUA1 parser/encoder.
   - GCR1 parser/encoder for existing ATID records.
   - Pair validator combining GUA1 + GCR1 into a read-only `BootAccountCatalog`.
   - Tests for account binding, cross-account credential mismatch, missing/orphaned records, duplicate IDs, truncation and trailing bytes.
2. `system/identity/src/account_registry.rs`
   - `AccountDirectoryStore` is a read-only view.
   - `AccountState` implements the view.
   - Legacy detached writes are separated into `MutableAccountDirectoryStore`; production account mutation must commit through `AccountStateRepository`.
3. `system/identity/src/boot_catalog.rs`
   - Builds two separate byte images from an `AccountDirectoryStore` view plus existing ATID records.
   - `AccountDirectorySnapshot` represents an ephemeral GUA1 export; it is not persistent storage.
4. `system/identity/examples/build-boot-catalog.rs`
   - Packages a GUA1 export and per-account ATID files into separate `sys.accounts` and `sys.credentials` files.
5. `modules/atc-shivacore/kernel/src/identity_catalog.rs`
   - Parses both entries with the same no_std codec and publishes only after full cross-validation.
6. `modules/atc-shivacore/kernel/src/main.rs`
   - Requires both `sys.accounts` and `sys.credentials`; rejects a partial pair and rejects coexistence with legacy `sys.identity`.
   - Retains the old singleton loader only as a temporary compatibility path when neither multi-account entry is present.
7. `scripts/qemu_boot.sh`
   - Accepts both files via `GLOBUS_ACCOUNT_DIRECTORY` and `GLOBUS_CREDENTIAL_CATALOG`.

## Host packaging interface

The GUA1 input must be an ephemeral export from the canonical encrypted AccountStateRepository, not a separately maintained production account database:

```bash
cargo run -p globus-identity --example build-boot-catalog -- \
  /run/identity-export/accounts.gua \
  /run/boot/sys.accounts \
  /run/boot/sys.credentials \
  user-alice=/run/credential-export/alice.atid \
  user-bob=/run/credential-export/bob.atid
```

Then:

```bash
GLOBUS_ACCOUNT_DIRECTORY=/run/boot/sys.accounts \
GLOBUS_CREDENTIAL_CATALOG=/run/boot/sys.credentials \
scripts/qemu_boot.sh
```

The packaging API accepts an `AccountState` directly through its `AccountDirectoryStore` implementation; the snapshot file is only for the standalone build example.

## Remaining M1 blockers

**M1 is not production-complete.** The following contract mismatch remains:

1. `system/identity::IdentityService` and `AccountState` currently persist an Argon2id PHC string in `IdentityRecord`.
2. The kernel's `globus-identity-core` verifier consumes the fixed 64-byte `ATID` record.
3. No approved conversion/migration contract currently maps PHC records to ATID, and no verified shared issuance path ensures host account creation emits the exact ATID format.
4. `IdentityKeyProvider` is an abstraction; a production ShivaCore/TPM/TEE key provider and complete account-state boot export path still need evidence.
5. The kernel loads the validated pair, but M2 must bind the Greeter username to the selected account and verify that account's ATID record.

Do not reinterpret PHC bytes as ATID. Do not create a parallel credential database. The next implementation gate is to make `globus-identity-core` the sole credential issue/verify contract and migrate existing PHC state explicitly before the boot exporter reads credentials from AccountState.

GCR1 is a transport envelope, not an authenticity signature. Secure-boot authenticity of the Ramdisk remains a separate security gate.

## Acceptance status

- [x] Shared GUA1 parser/encoder.
- [x] GCR1 wrapper for existing ATID records keyed by full UserId.
- [x] Pairwise validation of GUA1 + GCR1.
- [x] Kernel-only `sys.accounts` and `sys.credentials` loading.
- [x] Whole-catalog fail-closed publication.
- [x] AccountState read-only AccountDirectoryStore projection.
- [x] QEMU script accepts both Ramdisk entries.
- [ ] Production credential issue/migration to one ATID representation.
- [ ] Production IdentityKeyProvider and encrypted AccountState export integration.
- [ ] QEMU BIOS/UEFI tests with two real accounts.
- [ ] Exact-HEAD Actions logs and artifact hashes.

**Status:** IMPLEMENTED (shared formats, read-only account projection, kernel loader); RESIDUAL/BLOCKED (credential migration, production key/export wiring, QEMU evidence). Not E2E_VERIFIED.
