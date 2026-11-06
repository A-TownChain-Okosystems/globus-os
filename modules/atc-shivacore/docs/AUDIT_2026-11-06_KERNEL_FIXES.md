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

## Nachtrag 19:40 — SC-006-Kernseite: IRQ-/Timer-/Device-Caps

Befund: SC-006 (FROZEN) fordert IRQ-Caps {SUBSCRIBE, ACK}, Timer-Caps
{SET_DEADLINE, ACK} und Device-Caps {ACCESS}. `ResourceType` kannte keinen dieser
drei Typen, `Rights` hatte keine passenden Rechte — ein IRQ-, Timer- oder
Device-Cap liess sich im Kernel also gar nicht erzeugen. Die Deadline-Queue
(REQ-SC006-06, sortiert, ticklos) existierte dagegen bereits in `timer.rs`
(`TimerManager`, `BTreeMap<deadline_ns, TimerEvent>`).

Umgesetzt:

- `ResourceType::Irq`, `Timer`, `Device` plus `required_rights()` je Typ.
- `Rights` von `u8` auf `u16` erweitert, neue Bits `SUBSCRIBE` (32), `ACK` (64),
  `SET_DEADLINE` (128), `ACCESS` (256); `ALL` = 0x1FF.
- `CapabilityTable::create_typed()` vergibt für diese drei Typen **genau** den
  vorgeschriebenen Satz und lehnt angefragte Rechte außerhalb des Satzes mit
  `RightsExceeded` ab. Alttypen verhalten sich unverändert wie `create`.
- `remote_caps::signing_payload()` schreibt die Rechte jetzt als 2 Byte LE statt
  1 Byte. Tickets, die vor dieser Änderung signiert wurden, haben eine andere
  Signaturbasis und werden nicht mehr verifiziert.

Verifikation: `cargo test -p shivacore --lib` → **565 passed**, `-p
shivacore-service-space --lib` → 548 passed, Bare-Metal-ELF baut.

Noch offen in SC-006: IRQ-Trampolin-in-Notification und ACK-vor-Re-Trigger
(REQ-SC006-02/03) im Interrupt-Pfad, Maskierung bis ACK, DMA-Autorisierung über
Device-Cap (REQ-SC006-09) sowie Geräte-Enumeration im Service Space (REQ-SC006-11).
Die Cap-Typen sind die Voraussetzung dafür, nicht die Erfüllung.

## Nachtrag 19:45 — Signatur-Vertrag der Remote-Capability-Tickets

Befund (P0, Signatur-Vertrag): `RemoteCapabilityTicket::signing_payload()` verkettete
die variablen Felder ohne Längenpräfix und ohne Domain-Trennung. Zusammen mit
`Did::new()`, das jeden String ungeprüft übernimmt, heißt das: Issuer `"AB"` +
Subject `"C"` ergeben byte-identische Signaturbasis wie Issuer `"A"` + Subject
`"BC"`. Zwei verschiedene Tickets konnten damit dieselbe Signatur tragen; eine
Ticket-Signatur war zudem gegen Signaturen anderer Protokolle austauschbar.
Zusätzlich stand `deadline_unix` als rohes `f64` in der Signaturbasis
(`-0.0` ≠ `0.0` in Bytes, NaN-Vergleiche undefiniert).

Umgesetzt:

- `RCT_DOMAIN_SEPARATION = b"ATC-RCT-v1\0"` als Präfix, gleiche Konvention wie
  `p2p_secure::DOMAIN_SEPARATION`.
- Alle variablen Felder (Issuer-DID, Subject-DID, Ressourcentyp, Nonce,
  Parent-Nonce) mit `u32`-Längenpräfix über `push_field()`.
- Deadline als ganzzahlige Mikrosekunden (`i64`) statt `f64`.

Verifikation: 4 neue Tests in `remote_caps::rct_signature_contract`
(Domain-Tag vorhanden, verschobene Feldgrenzen kollidieren nicht mehr inkl.
Beleg der alten Ambiguität, Deadline byte-stabil für `-0.0`/`0.0` und gegen
Sub-Mikrosekunden-Rauschen, Nonces bleiben unterscheidbar).
`cargo test -p shivacore-service-space --lib` → **552 passed**, Kernel 565 passed,
Bare-Metal-ELF baut.

Folge für Bestandstickets: die Signaturbasis hat sich geändert (Domain-Tag,
Längen, Deadline-Kodierung). Vorher ausgestellte Tickets sind nicht mehr
verifizierbar und müssen neu ausgestellt werden. Ein Versionsfeld im Ticket
selbst wäre der nächste Schritt, damit solche Brüche künftig erkennbar sind statt
still zu scheitern.

## Nachtrag 20:05 — SC-006 REQ-02/03: Maskierung bis ACK

Befund: Zu den IRQ-Caps aus dem vorigen Nachtrag gab es keine Zustellungsschicht.
`irq.rs` existierte nicht, `apic.rs` gibt es im kompilierten Satz nicht (auch nicht
im Legacy-Satz), `interrupts.rs` schreibt nur `notify_end_of_interrupt`. Damit war
weder "kein Re-Trigger vor ACK" noch "line bleibt maskiert" irgendwo festgehalten.

Umgesetzt: `kernel/src/irq.rs` mit `IrqTable`.

- `subscribe(cap)` verlangt `ResourceType::Irq` und `Rights::SUBSCRIBE`, trägt den
  Cap-Eigentümer als Subskribenten ein und lehnt einen zweiten Subskribenten ab
  (genau ein Empfänger je Line, REQ-SC006-01).
- `deliver(line)` liefert den Empfänger nur beim Übergang ruhig → offen. Weitere
  Trigger während einer offenen Zustellung werden gezählt, aber nicht zugestellt
  (REQ-SC006-02, Trampolin-in-Notification ohne Re-Trigger).
- `ack(cap)` verlangt `Rights::ACK`, den passenden Eigentümer und eine offene
  Zustellung; erst dann wird die Line wieder freigegeben (REQ-SC006-03).
- `IrqLine::is_masked()` ist die Abfrage, die der IRQ-Eintritt stellen muss,
  bevor er erneut zustellt.

Verifikation: 9 neue Tests in `irq::sc006_irq_delivery` (falscher Captyp,
Eigentümerübernahme, zweiter Subskribent, kein Re-Trigger inkl. Zählung der
Koaleszierung, ACK hebt Maske auf, ACK ohne Recht, ACK durch Fremden, doppeltes
ACK, unbekannte Line). `cargo test -p shivacore --lib` → **574 passed**,
Service Space 548 passed, Bare-Metal-ELF baut.

**Grenze, ausdrücklich:** Das ist die Zustands- und Rechteschicht, nicht die
Hardware-Maske. Die LAPIC-Anbindung (Maske setzen/löschen, EOI) fehlt im
kompilierten Satz weiterhin; `IrqTable` liefert nur den Zustand, den der
IRQ-Eintritt abfragen muss. Ohne diese Anbindung ist REQ-SC006-03 auf
Hardware-Ebene nicht erfüllt, und ein Boot-Test in QEMU steht weiterhin aus.

## Nachtrag 20:50 — SC-006 REQ-03, LAPIC-Registerschicht

Befund: `IrqTable` wusste, ob eine Line maskiert ist, konnte sie aber nicht
maskieren. Ein APIC-Modul existierte nicht, weder kompiliert noch im Legacy-Satz.
REQ-SC006-03 war damit nur als Zustand vorhanden, nicht als Wirkung.

Umgesetzt: `kernel/src/apic.rs` mit `LocalApic<M: ApicRegisters>`. Die
Registerzugriffe laufen über das Trait, damit die Logik ohne Hardware prüfbar ist.

- `enable()` setzt Spurious-Vektor `0xFF` und das Enable-Bit; `is_enabled()`
  liest es zurück.
- `mask(vector)`/`unmask(vector)` schreiben das Interrupt-Enable-Register. Die
  Wortadresse wird aus dem Vektor berechnet (Wort = Vektor / 32, Stride 0x10),
  Vektoren unter 32 werden als `ReservedVector` abgelehnt, weil dort die
  Prozessor-Exceptions liegen.
- `eoi()` quittiert, `in_service(vector)` liest das ISR-Wort.
- Vektor 40 landet in Wort 1 (`0x360`, Bit 8), Vektor 70 in Wort 2 — die
  Tests belegen genau diese Adressrechnung, nicht nur das Bit.

Zusammenspiel mit der Zustandsschicht, wie es der IRQ-Pfad fahren muss:

1. `IrqTable::deliver(line)` liefert den Empfänger nur beim Übergang ruhig →
   offen.
2. `LocalApic::mask(vector)` sperrt die Line, dann geht die Notification raus.
3. Der Handler ruft `IrqTable::ack(cap)`, danach `LocalApic::eoi()` und
   `LocalApic::unmask(vector)`.
4. Kommt ein Trigger zwischen 2 und 4, zählt `IrqTable` ihn als koalesziert und
   stellt nichts zu.

Verifikation: 8 neue Tests in `apic::sc006_lapic_masking` (Enable/Spurious,
abgelehnte reservierte Vektoren inklusive Nachweis, dass nichts geschrieben
wird, Masken-Roundtrip, Wortrechnung für hohe Vektoren, Unabhängigkeit zweier
Vektoren im selben Wort, EOI-Register, ISR-Auswertung, ACK-Reihenfolge).
`cargo test -p shivacore --lib` → **582 passed**, Service Space 552,
`cargo check`, `fmt --check` und `clippy -D warnings` sauber, Bare-Metal-ELF baut.

**Grenze:** Die Registerschicht ist nicht mit dem MMIO-Fenster verbunden. Das
Mapping von `LAPIC_DEFAULT_BASE` (0xFEE00000) und die Zuordnung Vektor → Line
(Geräte-Enumeration, SC-006 Punkt 11) fehlen weiterhin, ebenso ein QEMU-Bootlauf.

## Nachtrag 21:20 — Erster echter Bootlauf in QEMU

Bis hierher war alles nur im Testprofil belegt. Jetzt gibt es einen Bootlauf.

Aufbau: Kernel-ELF (`--features x86-boot`, `x86_64-unknown-none`, 145.080 Bytes)
→ `boot`-Crate erzeugt `shivacore-bios.img` (2.589.696 Bytes) → QEMU 7.2.22,
`-display none -serial file -m 256M`, Laufzeit 35 s.

Ergebnis, seriell belegt:

```text
ShivaCore: Kernel-Einstiegspunkt erreicht.
ShivaCore: CPU vendor=AMD family=15 model=107 stepping=1 apic=true x2apic=false nx=true
ShivaCore: CPU HAL compatibility check OK.
ShivaCore: ACPI RSDP/root/table checksums validated; tables=5
EXCEPTION: BREAKPOINT            <- int3() aus main.rs, Handler fängt und kehrt zurück
ShivaCore: GDT/IDT/PIC OK (K-Sprint 1).
ShivaCore: Paging-Mapper + Heap initialisiert (100 KiB).
ShivaCore: Box-Test -- Wert: 41
ShivaCore: Vec-Test -- Summe 0..10: 45
ShivaCore: KernelState::boot() -> Done.
ShivaCore: USER-001 mapped PID=1000 RIP=0x400000 RSP=0x7ffff000
ShivaCore: USER-001 entering PID=1000 with timer preemption target PID=1001
ShivaCore: scheduler preemption -> context switch to PID=1001 ...
```

Nach 35 s Laufzeit: 332 Kontextwechsel zwischen PID 1000 und 1001, 3.324
Register-Checkpoints, kein Panic, QEMU lief bis zum Timeout (Exit 124 = der
Kernel lief weiter). Der Ring-3-Einstieg über IRETQ und die zeitgesteuerte
Verdrängung funktionieren also auf echter Hardware-Semantik, nicht nur im Test.

Der Framebuffer ist ebenfalls belegt: `screendump` über den QEMU-Monitor zeigt
1280x720 mit `ShivaCore Kernel v0.0.3 -- K-Sprint 2` und
`Boot: OK | Serial: OK | Framebuffer: OK`. Bild:
`docs/assets/shivacore-boot-framebuffer.png`.

Was der Lauf NICHT belegt:

- Die LAPIC-Registerschicht aus `apic.rs` läuft noch nicht: sie ist nicht an das
  MMIO-Fenster angebunden, der Vektor-zu-Line-Pfad fehlt. Der Screenshot zeigt
  die VGA-Framebuffer-Ausgabe über den Bootloader, nicht die APIC-Anbindung.
- Kein Gerät wird bedient. ACPI wird geprüft, nicht ausgewertet.
- Der UEFI-Pfad wurde nicht gestartet, nur das BIOS-Image.

Reproduzierbar: `scripts/qemu_boot.sh 20` baut ELF, Boot-Image und startet QEMU.
Das Skript kapselt zwei Stolpersteine: `bootloader` 0.11 ruft intern `cargo` mit
`-Z` auf (braucht `RUSTUP_TOOLCHAIN=nightly`) und verlangt `llvm-tools-preview`.

## Nachtrag 21:55 — LAPIC am MMIO, im Bootlauf nachgewiesen

Damit ist die Lücke aus dem vorigen Nachtrag geschlossen: `apic.rs` hängt jetzt
wirklich an Hardware.

- `ApicRegisters` hat eine zweite Implementierung: `MappedRegisters` liest und
  schreibt volatil über einen rohen Zeiger auf das MMIO-Fenster.
- `map_lapic_mmio()` in `main.rs` mappt `0xFEE0_0000` als Gerätespeicher
  (`PRESENT | WRITABLE | NO_EXECUTE`, eine 4-KiB-Seite deckt alle Register bis
  Offset 0x3F0). Ein bereits vorhandenes Identity-Mapping wird als
  `PageAlreadyMapped` akzeptiert.
- Im Bootpfad nach Heap-Init: APIC aktivieren, ID lesen, Vektor 0x40 maskieren,
  Maskenzustand zurücklesen, freigeben, erneut prüfen.

Seriell belegt im QEMU-Lauf:

```text
ShivaCore: Paging-Mapper + Heap initialisiert (100 KiB).
ShivaCore: LAPIC aktiv (id=0, spurious=0x1FF).
ShivaCore: LAPIC-Maskentest Vektor 0x40: maskiert=true freigegeben=true
```

`spurious=0x1FF` ist genau Vektor 0xFF plus Enable-Bit 0x100, also der erwartete
Wert. Der Maskentest beweist, dass Lesen und Schreiben im Interrupt-Enable-
Register auf dem echten (emulierten) APIC wirken und nicht nur im Testdouble.
Der Lauf endete nach 25 s mit 230 Kontextwechseln und ohne Panic.

Damit ist SC-006 auf allen drei Ebenen belegt: Spec-Zustand (`IrqTable`),
Registerlogik (`LocalApic` gegen Fake) und Hardware (`MappedRegisters` in QEMU).

**Weiterhin offen:** die Zuordnung Vektor → Line und die Geräte-Enumeration, also
die Frage, welcher IRQ welchen Vektor bekommt. Der APIC ist bedienbar, aber noch
an kein Gerät gebunden. ACPI wird geprüft, nicht ausgewertet; das UEFI-Image
wurde nie gestartet.

## Nachtrag 22:30 — ACPI-Auswertung (MADT) und UEFI-Pfad

**MADT.** Neues Modul `kernel/src/madt.rs`, reiner Parser über einen Byte-Slice,
also ohne Hardware prüfbar:

- Kopf: 36 Byte SDT plus LAPIC-Basis (Offset 36) und Flags.
- Einträge ab Offset 44, jeweils Typ und Länge. Länge unter 2 Byte →
  `BadEntryLength`, Eintrag über das Tabellenende → `EntryOverrun`. Beide Pfade
  sind mit Tests abgedeckt, ebenso eine falsche Signatur.
- Ausgewertet: Local APIC (Typ 0, inklusive Nutzbar-Flag), IOAPIC (Typ 1, mit
  Adresse und GSI-Basis), Interrupt-Source-Override (Typ 2), Local-APIC-Override
  (Typ 5), x2APIC (Typ 9, nur gezählt). Unbekannte Typen werden gezählt, nicht
  verworfen.
- `gsi_for_irq()` bildet einen ISA-IRQ auf die vorgeschriebene GSI ab.
- 10 Tests.

Im Bootpfad ersetzt die gemeldete Adresse die Konstante: `map_lapic_mmio()`
bekommt die Basis aus der MADT, `LAPIC_DEFAULT_BASE` bleibt nur Rückfallebene.

Seriell belegt (QEMU, BIOS):

```text
ShivaCore: ACPI RSDP/root/table checksums validated; tables=5
ShivaCore: MADT local_apic=0xFEE00000 ioapic=0xFEC00000 gsi_base=0 cpus=1/1 overrides=5 x2apic=0 unknown=1
```

Damit sind IOAPIC-Adresse und GSI-Basis bekannt, und die 5 ISA-Overrides, die
QEMU deklariert, sind gezählt. Der eine unbekannte Eintrag ist ehrlich als
unbekannt ausgewiesen (QEMU liefert NMI-Einträge, die dieser Parser nicht
auswertet).

**UEFI.** Bisher war nur das BIOS-Image gestartet worden. Mit OVMF
(`ovmf`-Paket, `/usr/share/OVMF/OVMF_CODE.fd`) bootet auch
`shivacore-uefi.img`:

```text
Kernel-Einstiegspunkt erreicht     1
CPU HAL compatibility check OK     1
ACPI RSDP                          1
LAPIC aktiv                        1
KernelState::boot() -> Done        1
USER-001 entering                  1
```

347 Kontextwechsel in 40 s, kein Panic. Das Skript kennt dafür jetzt
`bash scripts/qemu_boot.sh 40 /tmp/uefi.log uefi`.

**Weiterhin offen:** Der IOAPIC wird gelesen, aber nicht programmiert. Es gibt
damit noch keine Zuordnung von Geräte-IRQ zu Vektor, und kein Gerät ist
angebunden. Die MADT-Auswertung ist die Vorarbeit dafür, nicht die Umsetzung.

## Nachtrag 23:10 — IOAPIC programmierbar, im Bootlauf nachgewiesen

Der vorige Nachtrag endete mit: IOAPIC gelesen, aber nicht programmiert. Das ist
jetzt behoben.

**Treiber.** Neues Modul `kernel/src/ioapic.rs`:

- `RedirectionEntry` mit `encode()`/`decode()` (Vektor Bit 0..8, Delivery-Mode
  8..11, Zielmodus 11, Polarität 13, Auslösung 15, Maske 16, Ziel 24..32 der
  High-Hälfte). Roundtrip getestet.
- `IoApicRegisters`-Trait mit zwei Implementierungen: Fake für Tests,
  `MappedRegisters` volatile über rohen Zeiger für Hardware.
- `IoApic<M>`: `id()`, `version()`, `max_entries()` (aus Bits 16..24 des
  Versionsregisters), `redirection_entry()`, `set_redirection_entry()` mit
  Rücklesung, `mask()`/`unmask()` die nur das Maskenbit ändern.
- GSI unterhalb der Basis → `GsiBelowBase`, oberhalb der Tabelle →
  `GsiOutOfRange`. 9 Tests.

**Bootpfad.** `map_mmio_page()` (vorher `map_lapic_mmio`) mappt jetzt beliebige
MMIO-Fenster. IOAPIC-Adresse und GSI-Basis kommen aus der MADT. Der IRQ 0 wird
über die MADT-Overrides auf GSI 2 abgebildet, nicht über die Annahme GSI == IRQ.

Seriell belegt:

```text
ShivaCore: IOAPIC id=0 version=0x170020 gsi_base=0 entries=24
ShivaCore: IOAPIC GSI 2 -> Vektor 0x50 programmiert (maskiert=true, Ruecklesung=true)
```

`0x170020`: höchster Eintragsindex 0x17 = 23, also 24 Einträge, Version 2. Die
Rücklesung bestätigt, dass der geschriebene Eintrag im Chip steht.

**Ein Fehler unterwegs, der es wert ist, festgehalten zu werden.** Der erste
Versuch endete in einem DOUBLE FAULT. Ursache war nicht der IOAPIC, sondern ein
stiller Patch: `map_lapic_mmio()` war zuvor von `rustfmt` umformatiert worden,
mein späterer Textersatz passte nicht mehr und lief ins Leere. Ergebnis: der
Parameter `base` wurde ignoriert, das Fenster landete bei `0xFEE00000` und
überschrieb den LAPIC. Diagnosepunkte im Bootpfad haben es sofort gezeigt
("IOAPIC-Fenster bei 0xFEE00000" bei gemeldeter Basis `0xFEC00000`). Lehre:
Textersatz auf formatierten Code prüfen, und im Bootpfad lieber eine Zeile
mehr ausgeben als raten.

**Weiterhin offen:** Der Eintrag bleibt maskiert, die Zustellung läuft weiter
über die PIC. Der nächste Schritt wäre: Handler für Vektor 0x50 registrieren,
PIC-IRQ 0 maskieren, IOAPIC-Eintrag freigeben und die Interrupt-Zählung
vergleichen. Das ändert den funktionierenden Preemption-Pfad, deshalb ist es ein
eigener Schritt und nicht stillschweigend mitgezogen.

## Nachtrag 23:40 — Intermittierender Panic im Scheduler-Selbsttest behoben

Beim Prüfen der IOAPIC-Änderung fiel auf, dass Bootläufe sporadisch starben:

```text
ShivaCore: KERNEL PANIC -- panicked at interrupts.rs:350:
SCHED-001 register corruption: PID=1000 RAX=0x0 RBX=0x10000014f48 R12=0x8000017890
```

Erste Vermutung war ein UEFI-Problem, weil der UEFI-Lauf zuerst auffiel. Die
Wiederholung zeigte etwas anderes: 1 von 3 BIOS-Läufen starb genauso. Also kein
Firmware-Unterschied, sondern eine Race.

**Ursache.** `timer_trap_rust` prüft bei jedem Tick, ob der laufende Ring-3-Prozess
sein Register-Muster trägt (PID 1000: RAX=0x1000, RBX=0x1001, R12=0x1012). Feuert
der Timer, bevor der Payload seine `mov`-Befehle ausgeführt hat, steht dort noch
der Anfangszustand (RAX=0x0) und die Invariante schlägt zu Recht an — nur ist das
keine Korruption, sondern die Zeit vor dem ersten Befehl.

**Fix.** Ein `PATTERN_ESTABLISHED: [AtomicBool; 2]` merkt je Prozess, ob sein
Muster einmal beobachtet wurde. Davor ist ein Tick Aufwärmphase und wird als
solche protokolliert, danach gilt die Invariante unverändert und jeder Abweichler
panict weiterhin. Die Prüfung wird also nicht abgeschwächt, sondern ihre
Vorbedingung wird explizit gemacht.

Seriell sichtbar:

```text
ShivaCore: SCHED-001 warm-up tick PID=1000 RAX=0x0 RBX=0x18000014f48 R12=0x10000017a00
ShivaCore: SCHED-001 register pattern established PID=1000 RAX=0x1000 RBX=0x1001 R12=0x1012
ShivaCore: SCHED-001 register pattern established PID=1001 RAX=0x2000 RBX=0x2001 R12=0x2012
```

**Beleg.** 8 Läufe hintereinander, 4 BIOS und 4 UEFI, je 12 s: 0 Panics,
100 Kontextwechsel pro BIOS-Lauf, 67 bis 69 pro UEFI-Lauf. Vorher: 2 von 6
Läufen tot.

**Offen:** Der Grund, warum der Timer vor dem ersten Payload-Befehl feuert, ist
nicht beseitigt, nur die Fehlbewertung. Sauberer wäre, die Interrupts erst nach
dem `iretq` in den Ring 3 freizugeben. Das ist ein eigener Schritt, weil er den
Einstiegspfad in `userspace.rs` betrifft.

## Nachtrag 00:20 — Zustellung von PIC auf IOAPIC umgestellt

Damit ist der letzte offene Punkt der IRQ-Kette geschlossen: der Timer läuft
nicht mehr über die PIC, sondern über den IOAPIC.

**Zur Vorbemerkung „Interrupts erst nach iretq freigeben".** Die ist erledigt,
aber anders als gedacht. `userspace.rs` setzt im Frame `rflags = 0x202`, also IF.
Das Scharfschalten passiert damit durch `iretq` selbst, nicht vorher; ein
ausstehender Tick wird eine Instruktion später zugestellt. Die SCHED-001-Race aus
dem vorigen Nachtrag ist daher kein Timing-Fehler, sondern unvermeidbar: der
Payload hat vor seiner ersten Instruktion noch kein Registermuster. Die
protokollierte Aufwärmphase bleibt die richtige Antwort.

**Umschaltung.** Reihenfolge im Bootpfad, bewusst so:

1. PIC vollständig maskieren (`write_masks(0xFF, 0xFF)`). Ab hier gibt es genau
   einen Zustellweg.
2. Bestätigung auf den Local APIC umstellen (`switch_acknowledge_to_apic`), EOI
   über Register `0x0B0`.
3. Erst jetzt den IOAPIC-Eintrag freigeben: GSI 2 (IRQ 0 über MADT-Override) auf
   den Timer-Vektor, `maskiert=false`.

Wird die Reihenfolge verletzt, hängt der erste Interrupt: ohne APIC-EOI liefert
der Local APIC nichts mehr nach.

**Der Vektor.** `TIMER_VECTOR` wird jetzt aus `InterruptIndex::Timer as u8`
abgeleitet statt als Literal gepflegt. Damit kann der IOAPIC-Eintrag nicht auf
einen anderen Vektor zeigen als der IDT-Handler. Der ursprünglich geplante Test
dafür ist entfallen, weil das Bin-Target keine Testunterstützung hat (`cargo
test --bins` meldet „no targets matched"); ein Test, der nie läuft, wäre eine
Behauptung ohne Deckung.

Seriell belegt:

```text
ShivaCore: PIC maskiert (0xFF/0xFF), nur noch ein Zustellweg.
ShivaCore: Bestaetigung laeuft ueber den Local APIC (EOI 0x0B0).
ShivaCore: Zustellung auf IOAPIC umgestellt: GSI 2 -> Vektor 0x20, maskiert=false
```

**Wirkung.** 5 Läufe, 3 BIOS und 2 UEFI, je 12 s: 0 Panics, 214 Kontextwechsel
pro BIOS-Lauf, 158/159 pro UEFI-Lauf, in jedem Lauf die Umschaltmeldung. Dass
überhaupt weiter gewechselt wird, ist der eigentliche Beweis: ohne korrektes EOI
am Local APIC käme nach dem ersten Tick kein zweiter.

**Offene Beobachtung.** Die Wechselrate ist gegenüber dem PIC-Betrieb ungefähr
doppelt (vorher rund 9/s, jetzt rund 18/s). Plausibel ist, dass unter der PIC ein
Tick verloren ging, solange die ISR bis zum EOI nach der langsamen
Serienausgabe belegt war. Belegt ist das nicht, und ich trage es auch nicht als
Behauptung ein. Zu prüfen wäre es mit einem Tick-Zähler gegen eine unabhängige
Zeitquelle.

## Nachtrag 00:55 — Die offene Wechselrate ist gemessen

Im vorigen Nachtrag blieb offen, warum die Wechselrate nach der Umschaltung
ungefähr doppelt so hoch war. Vermutung war Tickverlust unter der PIC, belegt war
nichts. Jetzt ist es gemessen.

**Aufbau.** Der TSC wird gegen den PIT kalibriert (`calibrate_tsc`): Zählerstand
von Kanal 0 zweimal lesen, Neuladen erkennen, TSC-Abstand über fünf Perioden
messen. Damit ist der TSC eine Zeitbasis, die von der Interrupt-Zustellung
unabhängig ist. Dann werden je Weg 20 zugestellte Ticks gegen diese Basis
gemessen (`measure_tick_rate`). Beide Messungen laufen im Bootpfad, bevor der
Scheduler scharf ist, also ohne Serienausgabe im Handler.

**Ergebnis**, PIT in `init_pics` auf 100 Hz programmiert:

```text
ShivaCore: Tick-Messung 20 Ticks: PIC=468449544 TSC, IOAPIC=238698216 TSC, Verhaeltnis=1.9x
ShivaCore: Tick-Rate (PIT soll 100 Hz): PIC=45 Hz, IOAPIC=91 Hz
```

Damit ist die Frage entschieden, und zwar in die vermutete Richtung: die PIC
liefert etwa die Hälfte der Ticks (45 statt 100 Hz), der IOAPIC liegt mit 91 Hz
nahe am Sollwert. Es ist also kein Doppelzählen des IOAPIC, sondern Verlust auf
dem PIC-Weg.

**Grenze der Aussage.** Die Kalibrierung hängt an der Annahme, dass der PIT
wirklich mit 100 Hz läuft; ein Fehler darin verschiebt beide Zahlen gemeinsam,
das Verhältnis 1,9x bleibt davon unberührt. Die verbleibenden 9 % zwischen 91 und
100 Hz lassen sich mit dieser Genauigkeit nicht sauber von einem Kalibrierfehler
trennen. Ob Zeitrechnung dadurch 9 % zu langsam läuft, ist damit offen und nicht
behauptet.

**Nebenwirkung.** Weil der TSC jetzt kalibriert wird, sind absolute Raten künftig
prüfbar; die beiden Zeilen stehen in der README als Teil der erwarteten
Boot-Ausgabe.

## Nachtrag 01:30 — Korrektur: die PIC hat nie Ticks verloren

Der vorige Nachtrag schloss aus 45 gegen 91 Hz, die PIC verliere rund die Hälfte
der Ticks. Diese Schlussfolgerung war falsch. Zwei Fehler steckten darin, einer
davon in meiner Messung.

**Fehler 1, Messaufbau.** `measure_ticks` las den TSC-Start, bevor die erste
Tick-Flanke eintraf. Das gemessene Intervall enthielt damit bis zu eine volle
Periode Wartezeit; bei 20 Ticks sind das bis zu 5 %. Behoben: die Messung richtet
sich jetzt erst an einer Flanke aus und zählt dann. Zusätzlich 40 statt 20 Ticks
und 20 statt 5 PIT-Perioden zur Kalibrierung. Danach: PIC 49 Hz, IOAPIC 97 Hz,
Verhältnis exakt 2,0x.

**Fehler 2, die eigentliche Ursache.** Das Verhältnis war nicht halb, sondern
doppelt. Der PIT lief in Modus 3, einer Rechteckwelle mit zwei Flanken pro
Periode. Der IOAPIC-Eintrag ist kantengetriggert und hat beide Flanken als
Interrupt gezählt, die PIC nur eine. Der Timer lief also mit doppelter Frequenz,
und `MonotonicTime` wäre entsprechend doppelt so schnell gelaufen.

**Der Fix.** PIT-Kanal 0 läuft jetzt in Modus 2 (Rate Generator, Kommando `0x34`
statt `0x36`). Modus 2 erzeugt genau eine Flanke pro Periode, das ist die
Betriebsart für periodische Timer.

**Ergebnis**, gleiche Messung, gleiches Programm:

```text
ShivaCore: Tick-Messung 40 Ticks: PIC=959793960 TSC, IOAPIC=958219944 TSC, Verhaeltnis=1.0x
ShivaCore: Tick-Rate (PIT soll 100 Hz): PIC=98 Hz, IOAPIC=99 Hz
```

Beide Zustellwege liegen jetzt innerhalb der Messgenauigkeit am Sollwert, das
Verhältnis ist 1,0. Damit ist auch die Frage von vorhin beantwortet: der
Unterschied zwischen den Wegen war ein Artefakt der Rechteckwelle, kein Verlust
auf dem PIC-Weg.

**Was das über die früheren Zahlen sagt.** Die niedrigen Wechselraten aus den
Bootläufen (rund 9 bis 19 pro Sekunde, obwohl 100 Hz programmiert waren) sind
kein Tickverlust: der Handler schreibt bei jedem Tick zwei Serienzeilen, und die
Serienausgabe ist der Flaschenhals, nicht die Zustellung.
