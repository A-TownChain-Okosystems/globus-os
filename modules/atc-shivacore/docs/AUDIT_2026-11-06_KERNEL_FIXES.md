# Kernel-Audit 06.11.2026 — Befunde und Fixes

**Basis:** `globus-os` @ `f370b681e5c9` · Crate `modules/atc-shivacore/kernel`
**Werkzeug:** Rust 1.99.1 (test) / nightly (bare metal), Ziel `x86_64-unknown-none`
**Grenze:** Kein QEMU in der Audit-Umgebung. Build und Unit-Tests sind verifiziert,
der Boot-Pfad dieser Änderung ist **nicht** laufzeitverifiziert.

## Ausgangsbefund

| Befund | Vorher |
|---|---|
| Syscall-Gate 0x80 | schrieb nur eine serielle Zeile, kein Dispatch |
| Module ohne `mod`-Deklaration | 14 Dateien, 20.891 LOC, 872 `#[test]` |
| Kompilierte Kernel-Tests | 549 (von 1.422 gezählten `#[test]`) |
| `HandleClose` | gab `ok(0)` zurück, widerrief nichts |
| `CapabilityQuery` | gab `ok(1)` statt der Rechte |
| `MonotonicTime` | erhöhte den Zähler bei jedem Lesen |
| Ungenutzte TCB-Abhängigkeiten | `rand`, `ed25519-dalek`, `libm`, `pc-keyboard`, `volatile` |

## Fixes

1. **Syscall-Gate verdrahtet.** Neues Assembler-Gate `shivacore_syscall_gate`
   (identische Stack-Disziplin wie das Timer-Trampolin) ruft `syscall_gate_rust`.
   Neues Bibliotheksmodul `kernel/src/syscall_gate.rs` hält Dispatcher und
   Capability-Tabelle als eine Einheit und ist die einzige Stelle, die ein Gate
   aufruft. IDT-Eintrag 0x80 nutzt jetzt `set_handler_addr` mit DPL3.
   Registerkonvention nach SC-004 §3 (Cap-Slot, Opcode, 4 Argument-Wörter,
   2 Resultat-Wörter); die konkrete Zuordnung ist SC-DEC-L §13 und dort noch
   offen — im Code dokumentiert: `RDI` Cap-Slot, `RSI` Opcode, `RDX/RCX/R8/R9`
   Argumente, `RAX` Wert, `RBX` Status.
2. **Syscall-Semantik korrigiert.** `HandleClose` widerruft jetzt wirklich
   (inkl. Nachfahren, Eigentümerprüfung, `InvalidHandle`/`PermissionDenied`).
   `CapabilityQuery` liefert die Rechte-Maske. `MonotonicTime` liest nur noch;
   fortgeschrieben wird ausschließlich per `on_tick()` aus dem Timer-IRQ
   (`try_lock`, damit ein Tick im Ausnahmefall ausfällt statt im IRQ zu blockieren).
3. **14 nicht buildbare Dateien stillgelegt** nach
   `legacy/kernel-src-uncompiled/` mit Begründung. Beim Deklarieren aller 14
   Module entstehen 70 Compile-Fehler (u. a. `0xSHARED` in `threads.rs`,
   `{:20s}` in `tracing.rs`, fehlende Crate `bitflags`, Borrow-Fehler in
   `smp.rs`). Nichts gelöscht; Wiederverwendung braucht eine eigene Aufgabe.
4. **Ungenutzte Abhängigkeiten entfernt** (`rand`, `ed25519-dalek`, `libm`,
   `pc-keyboard`, `volatile`). Kein Kernel-Code referenzierte sie; Krypto und
   RNG gehören in den Service Space.

## Verifikation (lokal, reproduzierbar)

```
cargo test -p shivacore --lib                       -> 558 passed, 0 failed
cargo test -p shivacore-service-space --lib         -> 548 passed, 0 failed
cargo +nightly build -p shivacore --release \
  --target x86_64-unknown-none --features x86-boot  -> ELF 145.056 Bytes
```

Neue Tests: 5 im Gate (`gate_dispatches_yield_for_any_pid`,
`gate_enforces_capability_ownership`, `gate_close_revokes_through_the_table`,
`gate_time_only_moves_with_ticks`, `gate_rejects_unknown_syscall`) und 4 im
Dispatcher (`handle_close_revokes_the_capability`, `handle_close_denies_foreign_owner`,
`handle_close_rejects_unknown_handle`, `capability_query_returns_rights_mask`).
Der Test `monotonic_time_is_kernel_owned` wurde ersetzt: er hatte das
Hochzählen beim Lesen als Sollverhalten festgeschrieben.

## Offen (unverändert)

- Boot-Verifikation der Gate-Verdrahtung (QEMU-Lauf in CI) steht aus.
- SC-003 (Endpoint/Notification/Badge) und SC-005 (CNode/CSpace/mint) bleiben
  nicht umgesetzt; das gültige Modell ist weiterhin die flache `CapabilityTable`.
- SC-006 (IRQ-/Timer-/Device-Caps): `ResourceType` kennt diese Typen nicht,
  `hw_drivers.rs` liegt jetzt in `legacy/`.
- `globus-init` fehlt; Boot-Kette folgt SHIVA-BOOT-001, nicht AD-013(9).
- Evidence in `.atc/evidence/evidence.yaml` ist an `7f50c9cb` gebunden.
