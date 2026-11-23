# M1 — Multi-account boot catalog

**PR:** #125  
**Implementation branch:** `feature/multi-account-login-session`  
**Scope:** project the existing persistent account directory and existing ATID credential records into a kernel-readable Ramdisk entry. This does not create a second identity database.

## Canonical ownership

- `FileAccountDirectory` / `AccountDirectoryStore` remains the persistent authority for canonical username → stable `UserId`.
- The existing `GUA1` byte format remains unchanged. Its encoder/parser now lives in `globus-identity-core` as a shared `no_std` codec; `system/identity` delegates to that codec rather than maintaining another interpretation.
- `globus-identity-core::CredentialRecord` remains the credential authority: fixed 64-byte `ATID` Argon2id records.
- `GAC1` is a boot-image envelope only. It embeds the GUA1 directory snapshot and binds exactly one ATID record to each directory `UserId`. It is not persistent account storage.
- The kernel reads the envelope only from the protected Ramdisk namespace entry `sys.accounts`. User programs and services cannot read `sys.*`.

## GAC1 v1 binary layout

All GAC1 integer fields are big-endian. The embedded GUA1 format retains its existing layout and byte order.

| Offset | Field | Size |
|---:|---|---:|
| 0 | Magic `GAC1` | 4 |
| 4 | Version = 1 | u16 |
| 6 | Account count | u16 |
| 8 | Embedded GUA1 byte length | u32 |
| 12 | Existing GUA1 directory snapshot | variable |
| next | Repeated account records in canonical GUA1 username order | variable |

Each repeated account record contains: `UserId length (u16 BE)`, `UserId UTF-8 bytes`, then exactly 64 bytes of the existing `ATID` credential record. The repeated UserId must equal the corresponding GUA1 entry. The credential record is validated by the existing identity-core parser. Missing, duplicate, orphaned, mismatched, malformed, truncated, or trailing data is rejected as a whole.

The directory is sorted by canonical lowercase username. The parser rejects duplicate usernames, duplicate UserIds, non-canonical names, invalid UTF-8, invalid lengths, unsupported versions, and partial records.

## Implementation delivered

1. `system/identity-core/src/catalog.rs`
   - Shared no_std GUA1 directory parser/encoder.
   - GAC1 parser/encoder using existing ATID credential records.
   - Tests for multi-account binding, wrong-account credential use, duplicate entries, truncation, trailing bytes, and ID mismatch.
2. `system/identity/src/account_registry.rs`
   - GUA1 encode/decode delegated to identity-core; no second format implementation.
3. `system/identity/src/boot_catalog.rs`
   - `build_boot_catalog(AccountDirectoryStore, AccountCredential[])` reads the canonical directory and validates one ATID record per UserId.
4. `system/identity/examples/build-boot-catalog.rs`
   - Host-side packaging command for an existing GUA1 directory and per-account ATID files.
5. `modules/atc-shivacore/kernel/src/identity_catalog.rs`
   - Kernel-side atomic parse-and-publish of the complete catalog; no filesystem account database.
6. `modules/atc-shivacore/kernel/src/main.rs`
   - Loads `sys.accounts`; rejects simultaneous `sys.accounts` and legacy `sys.identity` as ambiguous. The old singleton path remains a compatibility fallback only when no multi-account catalog is present.
7. `scripts/qemu_boot.sh`
   - Accepts an explicit prebuilt catalog via `GLOBUS_ACCOUNT_CATALOG` and places it in the protected Ramdisk namespace. Without it, the script states that multi-account login remains unavailable.

## Host packaging

Create the GUA1 index using the existing `FileAccountDirectory` implementation. Supply one existing 64-byte ATID file per UserId:

```bash
cargo run -p globus-identity --example build-boot-catalog -- \
  /trusted/identity/accounts.gua \
  /trusted/boot/sys.accounts \
  user-alice=/trusted/credentials/alice.atid \
  user-bob=/trusted/credentials/bob.atid
```

Then boot with:

```bash
GLOBUS_ACCOUNT_CATALOG=/trusted/boot/sys.accounts scripts/qemu_boot.sh
```

The packer rejects any mismatch between the directory and supplied credential records. It does not generate passwords, derive wallet keys, or mutate the persistent directory.

## Remaining M1 integration blockers

**M1 is not yet production-complete.** The catalog codec and kernel loader are implemented, but two existing host/kernel credential contracts are not yet unified:

1. `system/identity::IdentityService` currently stores an Argon2id PHC string in `IdentityRecord`.
2. The kernel's existing `globus-identity-core` verifier consumes the fixed 64-byte `ATID` record.

There is no approved conversion/migration contract between these representations, and the current `IdentityStore` API is a trait with an in-memory implementation rather than a durable per-account backend. The boot packer therefore currently consumes existing ATID files explicitly; it must not silently reinterpret PHC strings or invent a second credential authority.

Additional boundaries:
- The kernel loads and validates the catalog, but M2 must bind the Greeter username to the selected account and verify that account's ATID record.
- GAC1 does not add authenticity protection. The boot image must be treated as trusted input until secure-boot signature verification is separately proven.
- The legacy `sys.identity` path must be removed after the multi-account path is implemented and migration is complete.

## M1 acceptance status

- [x] Canonical GUA1 format shared between host and no_std kernel code.
- [x] GAC1 envelope binds each credential record to a stable UserId.
- [x] Kernel-only Ramdisk entry `sys.accounts`.
- [x] Fail-closed whole-catalog validation.
- [x] Explicit QEMU boot-script input.
- [ ] Durable credential export from the existing production IdentityStore.
- [ ] Resolve PHC versus ATID as one normative credential representation.
- [ ] QEMU BIOS/UEFI test with two real catalog accounts.
- [ ] Workflow evidence bound to the resulting exact commit.

**Status:** IMPLEMENTED (catalog codec and loader); RESIDUAL/BLOCKED (production credential-source integration and QEMU evidence). Not E2E_VERIFIED.
