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
- Replace the singleton boot credential fixture with a validated account catalogue using existing account IDs and credential record formats.
- Define and test the on-image layout and account-directory-to-record binding.
- Add duplicate, missing, corrupt, mismatched-ID, and oversized-record rejection tests.

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
