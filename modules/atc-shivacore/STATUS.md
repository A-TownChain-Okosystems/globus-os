# 📊 Status — atc-shivacore

> **Stand:** 2026-09-17  
> **Version:** v1.0.0  
> **Milestone:** ShivaCore M1.2 VMM

---

## M1.2 VMM Status

| Area | Status |
|---|---|
| Canonical x86_64 VA validation | IMPLEMENTED |
| Strict userspace interval | IMPLEMENTED |
| Checked user-range arithmetic | IMPLEMENTED |
| 4 KiB / 48-bit physical validation | IMPLEMENTED |
| W^X enforcement | IMPLEMENTED |
| User mapping privilege boundary | IMPLEMENTED |
| Level-specific PS semantics | IMPLEMENTED |
| HHDM supervisor/RW/NX policy | IMPLEMENTED |
| Bootstrap mapping containment | IMPLEMENTED |
| Regression tests | IMPLEMENTED |
| Hardware page-table mutation | FOLLOW-UP |
| Transactional intermediate-table rollback | FOLLOW-UP |
| M1.3 object/mapping lifetime integration | FOLLOW-UP |
| Final M1.2 conformance freeze | NOT YET |

## Engineering rule

`IMPLEMENTATION-HARDENED / CONDITIONAL PASS` is not a final freeze. The VMM must pass implementation, CI, security, conformance, and documentation gates before it can be marked `FINAL` / `ACCEPTED`.

## Current implementation

The M1.2 validation core is located at:

`modules/atc-shivacore/kernel/src/vmm.rs`

The implementation is enabled from the kernel library and contains deterministic unit tests for the security and boundary invariants.

See `docs/M1.2_VMM_IMPLEMENTATION.md` for the normative implementation boundary and remaining work.
