# 📊 Status — atc-shivacore

> **Stand:** 2026-10-08  
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
| Hardware page-table mutation | IMPLEMENTED (deterministisch evidenzgetestet; Boot-Wiring folgt in der Hardware-Kette) |
| Transactional intermediate-table rollback | IMPLEMENTED |
| M1.3 object/mapping lifetime integration | FOLLOW-UP |
| Final M1.2 conformance freeze | NOT YET |

## Engineering rule

`IMPLEMENTATION-HARDENED / CONDITIONAL PASS` is not a final freeze. The VMM must pass implementation, CI, security, conformance, and documentation gates before it can be marked `FINAL` / `ACCEPTED`.

## Current implementation

The M1.2 validation core is located at:

`modules/atc-shivacore/kernel/src/vmm.rs`

The implementation is enabled from the kernel library and contains deterministic unit tests for the security and boundary invariants.

See `docs/M1.2_VMM_IMPLEMENTATION.md` for the normative implementation boundary and remaining work.

## Architecture Delta — P0 #45 Cleanup Stufe 2 (2026-10-08)

Reverse-Dependency-Audit: Der Kernel-Crate hat **0 Referenzen** auf Service-Space-
Subsysteme (ai, mempool, contract, vm, net, tcpip, p2p, p2p_secure, sockets,
container_net wurden in Stufe 1 nach `service_space/` verschoben).

Verbleibende, in `lib.rs`/`main.rs` nicht registrierte Kernel-Dateien sind
**geplante Kernel-Arbeit** (COMPONENT_PLAN.md, K-Sprints 32–46), keine
Service-Space-Reste: block, container, cow, devfs, fs_journal, page_fault,
power, signals, smp, threads, tracing, user_io. Ihre Aktivierung folgt der
P0-Abnahmekette (VMM M1.2/M1.3 → Hardware-Kette). `hw_drivers.rs` ist der
Planungsmarker für die Hardware-Kette (PCIe/HPET/virtio, siehe README).

Hygiene: `scheduler_ready_queue_contract.tmp` entfernt;
`FILE_REGISTER.md` regeneriert (nur getrackte Quell-Dateien, kein
`target/`-Müll mehr, reflektiert den Service-Space-Move).

## M1.2 Hardware-Mutationspfad (2026-10-08)

`kernel/src/vmm.rs` enthält jetzt neben dem Validierungskern den
`HardwareMapper`: echten PML4 → PDPT → PD → PT Walk mit Entry-Programmierung
und transaktionalem Rollback. Validierung (W^X, User-Intervall, Kanonizität,
HHDM-Supervisor/RW/NX, PS-Level-Semantik) läuft fail-closed VOR der ersten
Allokation. Schlägt der Walk mitten in der Hierarchie fehl (Frame-Erschöpfung
usw.), werden alle neu allokierten InterTables freigegeben und alle
Entry-Schreibungen auf den alten Wert zurückgerollt. 8 neue Regressionstests
(u.a. Mid-Walk-Rollback, Hierarchie-Wiederverwendung, Huge-Page-Ablehnung,
Leak-Freiheit). Evidenz: `cargo test -p shivacore vmm` 17/17.

Das physische Backend ist über `FrameBackend` abstrahiert; die Verdrahtung
mit HHDM-Translation + BootInfoFrameAllocator (x86-boot) folgt in der
Hardware-Kette der P0-Abnahmekette.
