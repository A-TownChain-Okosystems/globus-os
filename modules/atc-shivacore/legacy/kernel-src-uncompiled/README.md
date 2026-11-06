# Nicht kompilierte Kernel-Quellen (stillgelegt 06.11.2026)

Diese 14 Dateien lagen in `kernel/src/`, waren aber in **keiner** `mod`-Deklaration
referenziert (`lib.rs` 21 Module, `main.rs` 7 Module). Sie wurden von keinem Build
erfasst und von keinem anderen Modul verwendet.

Nachweis beim Reaktivierungsversuch (Audit 06.11.2026): das Deklarieren aller 14
Module erzeugt **70 Compile-Fehler** — u. a. ungültige Literale (`0xSHARED` in
`threads.rs`), ungültige Format-Traits (`{:20s}` in `tracing.rs`), fehlende Crate
`bitflags`, Borrow-/Move-Fehler in `smp.rs` und `threads.rs`. Der Code war also
nicht nur ungebaut, sondern nicht baubar.

Wirkung der Stilllegung: 20.891 LOC und 872 `#[test]`-Funktionen verschwinden aus
der Kernel-Bilanz. Die tatsächlich kompilierte und getestete Kernel-Bibliothek
umfasst 549 Tests.

Status je Datei: `UNCOMPILED / NOT BUILDABLE / NO REFERENCES`. Eine
Wiederverwendung erfordert eine eigene Aufgabe (Reparatur oder Neuschnitt), keine
bloße `mod`-Deklaration. Nichts hiervon wird gelöscht — die Inhalte bleiben als
Referenz erhalten.
