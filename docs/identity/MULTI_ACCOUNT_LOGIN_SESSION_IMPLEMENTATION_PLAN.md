# Multi-account Login and Session Integration Plan

**Status:** DESIGN BASELINE — implementation not yet verified  
**Base:** `main` at `4c62f28fe28fbd4de6053e99a4622406d1c33626`  
**Scope:** native GlobusOS identity path from account selection through authenticated session and process transition.

## 1. Decision

Use the existing `system/identity` architecture as the only account and identity authority. Extend and integrate its existing `AccountUsername`, `UserId`, `AccountDirectoryStore`, `IdentityStore`, `IdentityService`, `MultiAccountIdentityService`, and transaction journal. Do not introduce a parallel account database, credential verifier, username registry, or session authority.

The current kernel `IdentityGate` remains the security enforcement boundary for boot-time authentication requests. It must stop treating a single synthetic `sys.identity` record as the account model. The gate must resolve a canonical account identity and verify the matching credential record; it must never accept a password without a matching account identifier.

## 2. Existing implementation facts and constraints

- `MultiAccountIdentityService` already isolates per-account credentials and sessions, but is explicitly in-memory.
- `FileAccountDirectory` already provides durable username-to-`UserId` uniqueness. It stores no credentials or wallet secrets.
- `IdentityService` already verifies credentials with Argon2id, creates `IdentitySession`, and supports durable identity snapshots through `IdentityStore`.
- `globus-identity-core` provides the `no_std` credential-record verification boundary used by the kernel.
- The current kernel boot path publishes one credential record from the ramdisk into a singleton `IdentityGate`.
- The Greeter collects both username and password, but the current `IdentityCheck` call transmits only the password. The username is not part of the authentication decision.
- The current successful response only changes the Greeter status text. It does not create a durable identity session, issue a session handle, or transition into a user process.
- `SpawnProcess` is not currently dispatched by the kernel syscall ABI. Existing address-space construction helpers are not a substitute for an authorized process lifecycle API.

## 3. Target ownership boundaries

| Concern | Sole authority | Required behavior |
|---|---|---|
| Username normalization and uniqueness | `system/identity` account directory | Canonicalize once; reject duplicates and ambiguous aliases |
| Stable account identity | `UserId` in `system/identity` | Username resolves to exactly one stable ID |
| Credential persistence | Existing `IdentityStore` / record contract | One record per account; atomic, integrity-checked persistence |
| Password verification | `globus-identity-core` through kernel gate | Verify the record selected by canonical account ID; fail closed |
| Authenticated session | Existing `IdentitySession` model in `system/identity` | Bind session to UserId, expiry, state, and revocation |
| Kernel authorization | ShivaCore `CapabilityAuthority` | Mint only the minimum session-scoped capabilities after successful authentication |
| Process creation | ShivaCore process/address-space owner | Validate executable, construct isolated address space, register PID and owner |
| Login UI | Greeter | Collect and submit credentials; display result; no local verifier or authority |
| User environment | Existing GlobusOS shell/runtime | Start only after session and process authorization succeed |

## 4. Required end-to-end flow

1. Boot loads and validates the account directory and per-account credential records from the trusted system store. Missing, malformed, duplicate, or mismatched records stop authentication (fail closed).
2. Greeter submits both username and password through a versioned ABI request. Password bytes are copied from validated userspace memory into bounded kernel-owned memory and zeroized after verification.
3. Kernel resolves the username to a canonical `UserId`; it does not accept a caller-supplied UserId as authoritative.
4. The matching account credential is verified by the existing identity-core verifier. Unknown user and invalid password must not disclose account existence through distinct UI responses.
5. Successful verification creates a session bound to that UserId. Session IDs must be unguessable, scoped, expiring, revocable, and never derived from username or PID.
6. Kernel creates a session principal and grants only explicitly defined session capabilities. No broad EXEC/DELEGATE rights.
7. The authorized process-start operation validates the shell executable against the trusted program catalogue, builds its address space, registers it with the process manager/scheduler, and binds it to the session principal.
8. Greeter exits or is demoted only after the shell process is confirmed runnable. Any failure revokes provisional session capabilities and leaves the system at the login screen.
9. Logout, lock, timeout, process crash, and account revocation invalidate the session and revoke all session-derived capabilities.

## 5. ABI and security requirements

- Define a versioned login request containing a bounded username byte span and a bounded password byte span; reject overflow, invalid UTF-8, embedded NUL where prohibited, overlapping or unmapped ranges, and unsupported ABI versions.
- Do not pass a raw userspace pointer to a verifier without validating and copying it under the active process address space.
- Do not log usernames, passwords, credential hashes, session tokens, or recovery material.
- Use constant-time credential comparison as provided by the vetted verifier; apply rate limiting and lockout policy without account-enumeration leaks.
- Session issuance and process creation must be one fail-closed transaction from the user's perspective. Partial success must be rolled back.
- Session state must not be represented by a boolean returned to userspace. Return an opaque session handle with kernel-side ownership and lifecycle checks.
- Process creation must not be exposed as unrestricted caller-controlled PID, CR3, entry point, or arbitrary executable bytes.
- Keep wallet key custody and derivation outside the kernel login path. Login authenticates an account; it does not unlock or derive wallet keys implicitly.

## 6. Implementation sequence

### M1 — Multi-account boot credential catalogue
- Implement one encrypted account-state repository by reusing the existing encrypted-blob and file-blob boundaries.
- Make the existing AccountDirectoryStore behavior a view over that repository; do not create a second persistent account store.
- Reconcile PHC and ATID credential formats; use globus-identity-core as the sole credential creation/verification authority.
- Export the validated account directory and per-account ATID records into reserved kernel-only ramdisk catalogue entries.
- Validate the complete catalogue and all cross-references before publishing any account.

### M1 implementation gate — storage and format reconciliation

The first M1 code slice is gated by a concrete mismatch in the current tree. Do not wire the existing files together by copying records or creating a second account database.

**Observed at the audited base commit** `4c62f28fe28fbd4de6053e99a4622406d1c33626`:

| Existing component | Actual contract | Integration limitation |
|---|---|---|
| `FileAccountDirectory` | Filesystem-backed `GUA1` username → `UserId` index; atomic single-file replacement | Plain index is a separate file; not atomic with account credentials |
| `IdentityStore` | One `IdentityRecord` per store; credential is a PHC Argon2id string | No concrete durable multi-account adapter/codec is wired |
| `MultiAccountIdentityService` | Per-`UserId` in-memory map of `IdentityService` | Restart loses all accounts and sessions |
| `EncryptedBlobStore<FileBlobStore, ...>` | Encrypted opaque blob with generation/rollback checks | No serializer/repository adapter for the complete account state |
| `transaction_journal` | Journal codec/state machine plus abstract `JournalBackend` | No durable backend/replay coordinator is wired |
| Kernel `globus-identity-core` | Canonical fixed-size 64-byte `ATID` Argon2id record | Not byte-compatible with the userspace PHC credential field |
| Ramdisk `SRAM` | Entry names limited to 16 ASCII bytes; count is u16 | A separate entry per arbitrary `UserId` is not a safe catalogue format |
| Kernel `identity_file` | Loads only `sys.identity` | Singleton gate cannot represent multiple accounts |

**Important correction:** the existing `AccountDirectoryStore` interface and `FileAccountDirectory` implementation do not, by themselves, provide a durable multi-account identity database. The existing `MULTI_ACCOUNT_TRANSACTION_MODEL.md` already requires one encrypted account-state blob as the single commit point. M1 must implement that repository boundary by reusing `EncryptedBlobStore` and `FileBlobStore`, not by adding a second database or a plaintext sidecar.

**M1 order of work:**

1. Implement the single encrypted account-state repository described in `MULTI_ACCOUNT_TRANSACTION_MODEL.md`. It owns the existing username directory and per-account identity records in one versioned state object. Expose the existing `AccountDirectoryStore` behavior as a view/adapter over this repository; do not make `FileAccountDirectory` a second production source of truth.
2. Reconcile credential representation. The userspace `system/identity` currently stores PHC strings and uses its own Argon2id calls; the kernel uses the 64-byte `ATID` record and `globus-identity-core`. Select `ATID` as the shared login credential representation and move credential creation/verification behind `globus-identity-core`. Preserve an explicit, tested migration path for any existing PHC records; never reinterpret PHC bytes as ATID.
3. Add a bounded, versioned boot export from the unified repository. The export is a read-only projection, not persistent storage.
4. Use two reserved kernel-only SRAM entries: `sys.accounts` (the existing canonical username → `UserId` directory encoding) and `sys.credentials` (a versioned credential catalogue keyed by full `UserId`, each value containing exactly one 64-byte ATID record). This avoids SRAM's 16-byte entry-name limit and does not expose account records to Ring 3.
5. Kernel validates both entries, canonical username rules, unique usernames, unique UserIds, one-to-one directory/credential membership, all ATID records, maximum count/size, and absence of trailing bytes before publishing any account. Any error rejects the entire catalogue (fail closed); no partial account set is published.
6. Remove the production dependency on singleton `sys.identity`. A temporary compatibility path is allowed only behind an explicit migration flag and must never silently become the default when the multi-account catalogue is malformed or absent.

**M1 tests required before completion:**

- Unified repository: reopen after restart; two or more accounts; atomic commit; rollback/generation rejection; injected failures; username/record cross-reference invariants.
- Credential contract: identical test vectors in host and kernel for ATID issue/parse/verify; wrong password; malformed PHC migration; invalid parameters; salt/hash zeroization; no plaintext credentials in storage or logs.
- Boot catalogue: empty, missing, duplicate, mismatched, corrupt, truncated, oversized, unsupported-version, unknown-user, extra-credential, and trailing-byte cases.
- Ramdisk: kernel can read both `sys.accounts` and `sys.credentials`; Service/Program readers cannot see either.
- No account becomes visible until the complete catalogue passes validation.

**Current M1 status: PARTIALLY IMPLEMENTED; production gate remains BLOCKED.** The branch now contains an encrypted `AccountStateRepository` codec/repository over the existing `EncryptedBlobStore` and `FileBlobStore`, and `AccountDirectoryStore` is a read-only projection over `AccountState` / `AccountStateRepository`; the detached `FileAccountDirectory` writer is deprecated. The shared no_std codec now exports separate `sys.accounts` (GUA1) and `sys.credentials` (GCR1 containing existing ATID records) entries, and the kernel validates the pair before publication. The PHC-versus-ATID migration, production `IdentityKeyProvider`, credential export from the encrypted repository, and exact-HEAD QEMU/CI evidence remain open. Therefore M1 is not yet production IMPLEMENTED, TESTED, or E2E_VERIFIED.

### M2 — Username-bound login ABI
- Extend the existing ABI without renumbering frozen syscall IDs.
- Add bounded user-memory copy and zeroization.
- Bind the Greeter username field to the actual authentication decision.
- Add unknown-user, wrong-password, cross-account, and malformed-pointer tests.

### M3 — Session authority
- Create a kernel-owned opaque session handle after successful identity verification.
- Bind it to canonical UserId, expiry, lock state, and revocation.
- Integrate with the existing CapabilityAuthority; test cross-session isolation and revocation.

### M4 — Authorized process transition
- Add a process-start contract through the existing syscall gate and process manager.
- Resolve the shell by trusted catalogue name, not a userspace-provided arbitrary address.
- Build and register its address space; bind PID to session principal.
- Roll back all provisional resources on every failure path.

### M5 — Login-to-shell E2E
- QEMU BIOS and UEFI tests: two distinct accounts, correct and incorrect passwords, account isolation, successful shell start, logout, lock, timeout, process crash, and session revocation.
- Capture workflow Run → Job → Step/log evidence and artifact SHA-256 at the exact tested commit.
- Do not label the chain E2E_VERIFIED until the test reaches an authenticated shell session.

## 7. Acceptance criteria

- [ ] No parallel identity/account/credential/session authority is introduced.
- [ ] Two or more accounts boot from the same trusted account catalogue.
- [ ] Username and password are both part of the authentication decision.
- [ ] Correct credentials create a session bound to the correct UserId.
- [ ] Incorrect credentials, unknown users, corrupted records, and storage failures fail closed.
- [ ] One account cannot authenticate as, inspect, or revoke another account's session.
- [ ] Session capabilities are minimal and revocable.
- [ ] Shell process is created only after successful session issuance and executable validation.
- [ ] Logout/lock/timeout/crash revoke session-derived capabilities.
- [ ] BIOS and UEFI QEMU E2E tests reach the shell and prove account isolation.
- [ ] Evidence is bound to the exact current branch HEAD and artifact hashes.

## 8. Explicit non-goals

- No new wallet implementation or key-derivation algorithm.
- No change to ATC transaction signing or `ATC-STD-600`.
- No Aurora AI identity store.
- No broad capability rights or general-purpose arbitrary process execution.
- No production-readiness claim based only on unit tests, a successful kernel build, or a visible login prompt.

## 9. Current gate

This branch establishes the implementation contract only. The boot path is **not** complete until M1–M5 are implemented, tested, and evidenced. The current `sys.identity` singleton and synthetic boot credential fixture must not be treated as a production multi-account implementation.
