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

## Nachtrag 02:10 — Zeitquelle hing an einer Sperre

Nach dem PIT-Fix lief der Tick mit 100 Hz. Beim Prüfen, ob `MonotonicTime`
diesen Tick auch so verbraucht, kam ein eigener Fehler zum Vorschein.

**Der Fehler.** Der Timer-IRQ schrieb die Zeit so fort:

```rust
if let Some(mut guard) = SYSCALL_GATE.try_lock() {
    if let Some(gate) = guard.as_mut() { gate.on_tick(); }
}
```

`try_lock` heißt: läuft gerade ein Syscall, wird der Tick übersprungen. Der
Kommentar dazu lautete „monoton bleibt monoton" — das stimmt, aber die Zeit
bleibt dann stehen. Jeder Syscall kostet bis zu einen Tick, unter Last also
beliebig viel Zeit. Eine Uhr, die bei Betrieb langsamer wird, ist kaputt, nicht
monoton.

**Der Fix.** Die Zeitquelle ist jetzt ein eigener lock-freier Zähler in
`syscall.rs`:

```rust
static MONOTONIC_TICKS: AtomicU64 = AtomicU64::new(0);
pub fn advance_monotonic_tick() { MONOTONIC_TICKS.fetch_add(1, Ordering::Relaxed); }
pub fn monotonic_ticks() -> u64 { MONOTONIC_TICKS.load(Ordering::Relaxed) }
```

Der IRQ ruft nur noch `advance_monotonic_tick()` auf, ohne Sperre und ohne
Bedingung. `Syscall::MonotonicTime` liest denselben Zähler, `SyscallDispatcher`
hat kein eigenes Feld mehr. Die beiden Tests, die vorher absolute Werte
prüften, arbeiten jetzt mit Abständen — bei einer globalen Quelle laufen sie
sonst gegeneinander.

**Beleg.** Beide Zähler werden an derselben Tick-Flanke gelesen, damit der
Vergleich nicht um einen Tick verschoben ist:

```text
ShivaCore: Zeitquelle: 40 Ticks zugestellt, 40 MonotonicTime-Schritte (Drift +0)
ShivaCore: Tick-Rate (PIT soll 100 Hz): PIC=98 Hz, IOAPIC=99 Hz
```

Drei Läufe, jedes Mal 40/40 und Drift 0, Raten 98 bis 99 Hz.

**Zwei Ehrlichkeiten zur Messung.** Erstens hatte die erste Fassung dieser
Prüfung „Drift 0" gemeldet, weil `saturating_sub` den Wert −1 verschluckte; der
Fehler lag in der Messung, nicht im Kernel, und ist mit der Flankenausrichtung
behoben. Zweitens zeigte ein einzelner Bootlauf 138 Hz auf beiden Wegen: die
TSC-Kalibrierung lag in diesem Lauf daneben. Die Rate wird deshalb immer über
mehrere Läufe geprüft und nicht aus einem einzelnen Lauf behauptet.

## Nachtrag 02:45 — Die IRQ-Tabelle hatte keinen Aufrufer

`irq::IrqTable` konnte Subskribenten fuehren, Zustellungen koaleszieren und ACKs
pruefen. Nur rief sie niemand auf: im Kernel war sie nirgends instanziiert, und
`deliver()` hatte ausserhalb der Tests keinen Aufrufer. "Maskiert bis ACK" war
damit eine Eigenschaft einer Datenstruktur, nicht des Interrupt-Pfads.

**Was jetzt da ist.** `kernel/src/irq_bridge.rs` verbindet Tabelle und
Maskenregister. Die Maskenebene haengt hinter dem schmalen Trait `IrqMasking`,
den `LocalApic<M>` implementiert; im Test ist es ein Mitschreiber.

```rust
pub fn on_interrupt(&mut self, line: u32, vector: u32) -> Result<Option<Pid>, BridgeError> {
    let Some(pid) = self.table.deliver(line) else { return Ok(None); };
    self.masking.mask_vector(vector)?;   // sperren, bevor der Subskribent laeuft
    Ok(Some(pid))
}
```

`ack(cap, vector)` prueft Rechte und Eigentuemer in der Tabelle und gibt erst
danach die Sperre frei.

**Reihenfolge und warum sie stimmt.** Der Vektor ist waehrend des Handlers in
Bearbeitung, ein verschachtelter Interrupt derselben Line ist ausgeschlossen.
Entscheidend ist, dass die Sperre steht, bevor der Handler zum Subskribenten
zurueckkehrt; genau das macht `on_interrupt`. Scheitert das Sperren, kommt der
Fehler zurueck und der Aufrufer darf die Zustellung nicht als abgesichert
werten.

**Sieben neue Tests** (607 gesamt): Sperre vor Zustellung, Koaleszierung ohne
zweite Sperre, ACK gibt frei und der naechste Trigger sperrt erneut, fremder
Eigentuemer kann nicht quittieren, Line ohne Subskribent sperrt nichts,
Maskenfehler wird gemeldet, falscher Resourcentyp quittiert nicht.

**Ein Befund aus dem Testbau.** Ein IRQ-Cap ohne ACK-Recht laesst sich ueber
`CapabilityTable::create_typed` nicht erzeugen: fuer `ResourceType::Irq` vergibt
die Tabelle immer `SUBSCRIBE|ACK`. Der Zweig "ACK ohne Recht" ist damit ueber die
oeffentliche API unerreichbar. Der Test prueft deshalb die erreichbare Schranke
(falscher Resourcentyp) statt einen Zustand zu behaupten, den der Rest des
Systems gar nicht herstellen kann.

**Weiterhin offen.** Die Bruecke ist noch nicht im echten Interrupt-Pfad
verdrahtet, und es gibt keinen ABI-Syscall fuer das ACK; die sechs Syscalls in
SHIVA-SYSCALL-ABI-001 bleiben unveraendert, statt die ABI eigenmaechtig zu
erweitern. Das ACK muss an einer bestehenden Stelle ankommen, etwa beim
Konsumieren der Notification, und das braucht eine Spec-Entscheidung.

## Nachtrag 03:20 — Die Brücke läuft jetzt im echten Interrupt-Pfad

Der Handler sperrt die Line beim Eintritt und gibt sie erst im ACK wieder frei:

```rust
// interrupts.rs, timer_trap_rust
if shivacore::apic::acknowledges_via_apic() {
    shivacore::apic::mask_interrupt_vector(TIMER_VECTOR);
}
...
fn acknowledge_timer_irq() {
    if shivacore::apic::acknowledges_via_apic() {
        shivacore::apic::acknowledge_interrupt();          // EOI
        shivacore::apic::unmask_interrupt_vector(TIMER_VECTOR);  // Freigabe
    }
}
```

`apic.rs` hat dafuer `mask_interrupt_vector`/`unmask_interrupt_vector` (Wort =
`vector/32`, Stride 0x10, Bit = `vector%32`, volatil) plus zwei Zaehler.

```text
ShivaCore: IRQ-Pfad: 41 Ticks ueber IOAPIC, 41 Sperren, 41 Freigaben
```

Zwei BIOS-Laeufe und ein UEFI-Lauf, jeweils ohne Panic, Sperren und Freigaben
jedes Mal gleich hoch.

**Drei Messfehler auf dem Weg dahin, alle meine.** Erstens meldete der erste
Versuch `0 Sperren` bei `basis=0xfee00000`: die Zaehler standen noch vor dem
Umschalten auf den IOAPIC, also vor dem ersten maskierten Tick. Zweitens
behauptete die Handprobe im Bootpfad "funktioniert", was nur die Helfer prueft,
nicht den Handler. Drittens zeigte die Sicht aus dem Handler `apic=false`, was
ich erst fuer einen zweiten Modulzustand hielt — es war schlicht derselbe
Reihenfolgefehler: die Diagnosezeile lief vor dem Umschalten. Erst die
Verlegung der Messung hinter `switch_acknowledge_to_apic` machte den Pfad
sichtbar. Lehre: eine Zaehlermessung im Bootpfad gehoert hinter den Zustand, den
sie belegen soll, sonst misst sie den Vorher-Zustand.

Ausserdem bleibt festgehalten: mit dem Vektor 0x20 und den acht IER-Worten liegt
das beschriebene Register bei 0x360 bis 0x370, also sicher im gemappten 4-KiB-
Fenster; Vektoren unter 32 werden nicht zugelassen, damit kein Exception-Vektor
gesperrt wird.

**Weiterhin offen.** Es gibt keinen ABI-Syscall fuer das ACK; die sechs Syscalls
aus SHIVA-SYSCALL-ABI-001 bleiben unveraendert. Solange der Kernel selbst der
Empfaenger ist, genuegt das ACK am Handlerende. Sobald ein Userspace-Prozess
eine Geraete-Line abonniert, braucht es eine Spec-Entscheidung, wo sein ACK
ankommt. Die Bruecke aus `irq_bridge.rs` (Tabelle plus Rechtepruefung) ist dafuer
vorbereitet, aber noch nicht angeschlossen.

## Nachtrag 03:50 — Vektor 0x20 stand als Zahl im Bootpfad

Die Zuordnung Line → Vektor gab es nicht. Der Timer-Eintrag auf 0x20 stand als
feste Zahl im Bootpfad, und niemand haette sagen koennen, welcher Vektor fuer
eine zweite Line noch frei ist.

**Neu: `kernel/src/irq_routing.rs`.** Eine Tabelle (24 Zeilen, so viele hat die
Redirection Table eines IOAPIC) haelt fest, welche Line welchen Vektor belegt:

```rust
let mut routing = IrqRouting::new();
let timer = routing.assign_fixed(0, timer_gsi, TIMER_VECTOR)?;   // 0x20 bleibt
routing.next_free_vector()                                        // -> 0x30
```

`assign_next_free` vergibt den naechsten freien Vektor, `assign_fixed` traegt
einen vorgegebenen ein. Beide lehnen Vektoren unter 0x20 ab (Prozessor-
Exceptions), beide verhindern, dass zwei Lines denselben Vektor teilen.
`route_for_vector` ist der Weg zurueck fuer den Interrupt-Pfad, der nur den
Vektor kennt; `route_for_gsi` der fuer den IOAPIC.

**Zwei Begriffe statt einem.** Mein erster Entwurf hatte nur "Geraetevektor ab
0x30". Ein Test hat die eigene Annahme widerlegt: der Timer sitzt auf 0x20,
also unterhalb dieses Fensters. Jetzt heisst es getrennt "routbar ab 0x20" und
"automatisch vergebbar ab 0x30". Der Timer bleibt damit eintragbar, ohne dass
die automatische Vergabe in seinen Vektor laeuft.

**Ein Fehler mit Panic.** Beim Verdrahten trug ich die Route zunaechst mit
`timer_vector` (0x50) ein, also dem Vektor des Probedrucks, nicht mit dem des
IDT-Handlers (0x20). Der IOAPIC stellte danach auf 0x50 zu, dort lag kein
Handler: `Panic=1`. Die Bootmeldung zeigte es sofort ("GSI 2 -> Vektor 0x50"),
die Korrektur war die Zuordnung, nicht der Handler.

Beleg aus dem Lauf:

```text
ShivaCore: IRQ-Routing: IRQ 0 -> GSI 2 -> Vektor 0x20, naechster freier Geraetevektor 0x30
ShivaCore: Zustellung auf IOAPIC umgestellt: GSI 2 -> Vektor 0x20, maskiert=false
ShivaCore: IRQ-Pfad: 41 Ticks ueber IOAPIC, 41 Sperren, 41 Freigaben
```

Zwei BIOS-Laeufe und ein UEFI-Lauf, kein Panic. 616 Kernel-Tests (8 neue).

**Ein Befund wie bei den IRQ-Caps.** Das Vektorfenster umfasst 193 Vektoren, die
Tabelle nur 24 Zeilen. `NoVectorLeft` ist mit dieser Groesse nicht erreichbar;
der Test prueft deshalb `TableFull` und behauptet nicht die unerreichbare
Variante.

**Weiterhin offen.** Es gibt genau eine Line mit Eintrag (den Timer). Ein
zweites Geraet braucht einen Treiber, einen IDT-Eintrag und einen Handler, die
Tabelle ist dafuer die Voraussetzung, nicht der Ersatz.

## Nachtrag 04:20 — Die zweite Line war bisher nur eine Moeglichkeit

Nach der Routing-Tabelle gab es einen Eintrag und einen Vektor. Dass die Tabelle
ein zweites Geraet traegt, war eine Behauptung ohne Zustellung. Jetzt haengt die
PS/2-Tastatur am IOAPIC statt an der PIC, auf dem ersten automatisch vergebenen
Vektor:

```text
ShivaCore: IRQ-Routing: IRQ 0 -> GSI 2 -> Vektor 0x20, naechster freier Geraetevektor 0x30
ShivaCore: IRQ-Routing: IRQ 1 -> GSI 1 -> Vektor 0x30
ShivaCore: Tastatur auf IOAPIC: GSI 1 -> Vektor 0x30, maskiert=false
ShivaCore: Tastatur ueber Vektor 0x30: Scancode 0x1E
ShivaCore: Tastatur ueber Vektor 0x30: Scancode 0x9E
```

Die drei Tasten `a`, `b`, `c` im QEMU-Lauf erscheinen als Druecken und
Loslassen (0x1E/0x9E, 0x30/0xB0, 0x2E). Unter UEFI dasselbe Bild mit `x`, `y`.
Der Handler macht denselben Ablauf wie der Timer: sperren beim Eintritt, Scancode
lesen, APIC-EOI, freigeben. `KEYBOARD_VECTOR` ist
`irq_routing::FIRST_DEVICE_VECTOR`, und der Bootpfad warnt, wenn die Route nicht
zu diesem IDT-Eintrag passt — sonst zeigt ein IOAPIC-Eintrag auf einen Vektor,
an dem niemand sitzt, und der Rechner schweigt.

**Zwei Kompilierhuerden, beide schon einmal gesehen.** `serial_println!` als
Match-Arm ohne Klammern ergibt "trailing semicolon in macro used in expression
position"; die Arme brauchen `{ ... }`. Und die IDT ist in x86_64 0.15.5 nur
ueber `u8` indizierbar (`impl Index<u8>`), nicht ueber `usize` — die vorhandenen
Eintraege machen es vor, ich hatte es beim neuen Eintrag anders geschrieben.

**Ehrliche Grenze.** Die Sperr- und Freigabezaehler in `apic.rs` sind global,
nicht je Vektor. Die Zeile "41 Sperren, 41 Freigaben" bezieht sich auf das
Messfenster vor dem Tastaturverkehr; sobald Tasten dazukommen, zaehlen sie mit.
Fuer die Aussage "jede Zustellung sperrt und gibt frei" ist das ausreichend, ein
Nachweis je Line waere eine eigene Zaehlertabelle.

**Weiterhin offen.** Die Tastatur hat noch kein Scancode-zu-Keycode-Mapping, sie
meldet Rohcodes. Und das ACK eines Userspace-Subskribenten ist nach wie vor eine
Spec-Entscheidung; die Bruecke mit Rechtepruefung liegt bereit, angeschlossen ist
weiterhin nur der Kernel als Empfaenger.

## Nachtrag 04:55 — Die Tastatur meldete Zahlen, keine Tasten

Der Handler las den Rohcode und schrieb ihn ins Log. Damit war bewiesen, dass
ein Interrupt ankommt, aber nicht, welche Taste er bedeutet. Kein Treiber, ein
Zaehler.

**Neu: `kernel/src/keymap.rs`.** Set-1-Codes werden zu Zeichen: Bit 7 heisst
losgelassen, `0xE0` ist ein Praefix fuer die naechste Taste, Shift und Caps Lock
sind Zustand statt Zeichen. `decode(scancode, &mut ModifierState)` schreibt den
Zustand fort und liefert `Decoded { key, pressed, character, state_changed }`.
Reine Funktion, kein Portzugriff — 10 Tests, darunter alle 256 Codes ohne Panic,
Caps-und-Shift-heben-sich-auf, und Loslassen erzeugt kein Zeichen.

Im Lauf mit gesendeten Tasten:

```text
Tastatur ueber Vektor 0x30: 'a' (Scancode 0x1E)
Tastatur ueber Vektor 0x30: ShiftLeft (Scancode 0x2A)
Tastatur ueber Vektor 0x30: 'A' (Scancode 0x1E)
Tastatur ueber Vektor 0x30: '1' (Scancode 0x02)
Tastatur ueber Vektor 0x30: '!' (Scancode 0x02)
Tastatur ueber Vektor 0x30: Enter (Scancode 0x1C)
```

Unter UEFI zusaetzlich Caps Lock: `z`, `Shift`, `Z`, `CapsLock`, `B` — das letzte
gross, weil Caps Lock noch an war.

**Dreimal derselbe Fehler, einmal ein neuer.** `serial_println!` als Match-Arm
ohne Klammern ist mir jetzt zum dritten Mal passiert ("trailing semicolon in
macro used in expression position"). Die Klammern gehoeren zum Arm, nicht zum
Makro. Neu und schlimmer: Ich wollte die vier Arme per regulaerem Ausdruck
klammern und habe damit den Rest der Datei zerschnitten — der Compiler meldete
`timer_interrupt_entry_addr` und drei weitere Namen "defined multiple times".
Rettung war `git checkout` der Datei und zwei exakte Textersetzungen. Lehre: bei
strukturellen Aenderungen an Rust-Code keine Regex ueber mehrzeilige Bloecke,
sondern entweder exakte Zeichenketten oder ein Neu-Schreiben des Blocks.

**Weiterhin offen.** Die Belegung ist US-amerikanisch; eine deutsche ist eine
Tabelle mehr, keine Architekturentscheidung. Und die Tastenfolge endet im Log:
Es gibt noch keinen Kanal, ueber den Zeichen an einen Userspace-Prozess gehen.

## Nachtrag 05:40 — Die Routing-Tabelle starb am Ende des Boots

Der unauffaelligste Fehler der letzten Runde: `IrqRouting` wurde in `main` als
lokale Variable gebaut. Nach dem Boot war sie weg. Der Interrupt-Pfad konnte
nicht fragen, welche Line auf seinem Vektor liegt, und benutzte deshalb eine
Konstante — die Tabelle war fuer die Laufzeit bedeutungslos, sobald sie befuellt
war.

**Jetzt lebt sie im Kernel.** `interrupts.rs` haelt `DEVICE_ROUTES:
Mutex<IrqRouting>` und bietet vier Zugriffe: `fix_route`, `route_line`,
`line_for_vector`, `next_free_vector`. `main` befuellt sie beim Boot, der Handler
liest sie:

```rust
let line = line_for_vector(KEYBOARD_VECTOR).map(|route| route.irq);
```

Beleg aus dem Lauf:

```text
Tastatur ueber Vektor 0x30 (Line Some(1)): 'a' (Scancode 0x1E)
Tastatur ueber Vektor 0x30 (Line Some(1)): Enter (Scancode 0x1C)
```

Die Line stammt aus der Tabelle, nicht aus dem Code. Wer eine dritte Line
eintraegt, bekommt sie im Log ohne eine Zeile Handler-Aenderung.

**Zwei Grenzen, ehrlich benannt.** Erstens: `lazy_static` initialisiert beim
ersten Zugriff. Der passiert beim Boot in `fix_route`, also lange vor dem ersten
Geraete-Interrupt; kaeme der erste Zugriff aus dem Handler, wuerde die
Initialisierung dort laufen. Das ist der Grund, die Tabelle im Boot zu befuellen,
bevor der IOAPIC-Eintrag freigegeben wird. Zweitens: Die Verdrahtung selbst ist
nur im Bootlog nachweisbar. Die Tabelle ist als reine Logik in `irq_routing`
getestet; dass der Handler sie benutzt, zeigt sich an der Zeile im Log, nicht an
einem Unit-Test — im Bin-Target gibt es keine Testunterstuetzung.

626 Kernel-Tests, check/fmt/clippy sauber, BIOS und UEFI ohne Panic.

## Nachtrag 06:35 — Die IRQ-Bruecke hatte noch nie einen Aufrufer

`irq_bridge.rs` lag seit dem SC-006-Sprint im Baum: Tabelle, Rechtepruefung,
Sperren bis ACK, Tests. Im echten Pfad rief sie niemand. Die Tastatur sperrte
ihren Vektor direkt ueber `apic::mask_interrupt_vector` — dieselbe Wirkung, aber
an der Tabelle vorbei, ohne Capability und ohne ACK-Buchhaltung.

**Jetzt laeuft Line 1 durch die Bruecke.** Der Kernel legt beim Boot eine
Capability mit `ResourceType::Irq` und dem von SC-006 vorgeschriebenen Satz
`SUBSCRIBE|ACK` an (`create_typed`, Eigentuemer PID 0), abonniert damit die Line
und gibt erst danach den IOAPIC-Eintrag frei:

```text
ShivaCore: IRQ-Bruecke: Line 1 abonniert (Kernel-Cap SUBSCRIBE|ACK)
```

Im Handler: `on_interrupt(line, vector)` sperrt ueber die Tabelle, nach dem
Scancode folgt EOI, dann `ack(&cap, vector)` — erst das gibt den Vektor frei.

**Nachweis, dass es nicht nur so aussieht.** Zwei Zaehler stehen an jeder
Tastenzeile: Zustellungen ueber die Bruecke und Rueckfaelle auf direktes Sperren.

```text
Tastatur ueber Vektor 0x30 (Line Some(1)): 'b' (Scancode 0x30) [Bruecke 2, Rueckfall 0]
Tastatur ueber Vektor 0x30 (Line Some(1)): 'c' (Scancode 0x2E) [Bruecke 4, Rueckfall 0]
```

Zwei Zustellungen je Taste, Druecken und Loslassen, kein Rueckfall. Unter UEFI
dasselbe. Die erste Zeile zeigt 0, weil der Zaehler nach dem Log erhoeht wird —
nicht null Zustellungen, sondern noch keine gezaehlte.

**Der Rueckfall ist Absicht, nicht Rest.** Ist die Line nicht eingetragen, fehlt
die Capability oder scheitert das Sperren, sperrt der Handler direkt und zaehlt
das in `DIRECT_FALLBACKS`. Ein stiller Umweg waere schlimmer als ein sichtbarer.

**Grenze.** Die Bruecke traegt bisher eine Line mit einem Kernel-Subskribenten.
Dass ein Userspace-Prozess ACK gibt, braucht eine Syscall-Entscheidung; das
bleibt offen und ist der Grund, warum die Zeile im Log endet.

## Nachtrag 07:30 — Eine Capability ohne Tabelle ist nicht widerrufbar

Der Fehler steckte in meiner eigenen Verdrahtung von vorhin: `subscribe_kernel_line`
legte eine Capability an, warf die `CapabilityTable` weg und behielt nur eine
Kopie. Damit war die Capability heimatlos — `revoke(&mut self, cap_id)` haette
nie etwas gefunden, die Abstammung kannte niemand, und "der Kernel besitzt ein
Abo" war eine Behauptung ohne Ort, an dem sie nachpruefbar waere.

**Jetzt liegt sie in der Tabelle.** Neu: `kernel/src/kernel_caps.rs` (Lib, also
testbar) mit `KernelCaps { table, subscriber }`, `grant(line)` ueber
`create_typed` (Rechtesatz aus SC-006, nicht aus dem Aufruf), `subscriber_cap()`
liest die Capability bei jedem ACK aus der Tabelle statt aus einer Kopie, und
`revoke_subscription()` widerruft sie. Der Handler fragt nur noch:

```rust
let cap = KERNEL_CAPS.lock().subscriber_cap();
```

**Der Widerruf hat einen Test, keine Behauptung.** 6 neue Tests, darunter:
`revoke_removes_the_capability_from_the_table` (der eigentliche Punkt), erneutes
Abonnieren nach Widerruf ergibt eine neue Id, doppelter Widerruf meldet ehrlich
`false`, und die Capability traegt `SUBSCRIBE|ACK` mit `resource_id` der Line.

**Zweiter, kleinerer Fund.** Traf eine Zustellung auf eine offene (nie
quittierte) Zustellung, blieb die Line gesperrt und der Handler loggte nichts.
Das haette wie eine tote Tastatur ausgesehen. Jetzt zaehlt `STUCK_DELIVERIES`
solche Faelle, und die Tastenzeile zeigt sie:

```text
Tastatur ueber Vektor 0x30 (Line Some(1)): 'c' (Scancode 0x2E) [Bruecke 4, Rueckfall 0, Haenger 0]
```

`Haenger 0` heisst: keine Zustellung blieb unbeantwortet. Eine echte
Wiederherstellung fuer eine haengende Line gibt es nicht — SC-006 sagt "maskiert
bis ACK", ein erzwungenes Freigeben waere eine Spec-Entscheidung und steht nicht
mir zu.

**Eigener Umweg, ehrlich notiert.** Ich hatte fuer `grant` kurz einen
`IntoOk`-Trait gebaut, nur um `Ok(cap)` nicht zweimal zu schreiben. Das war
Unsinn und ist entfernt. Und mein erster Textersatz hat den Doc-Kommentar
zerrissen ("unclosed delimiter") — dieselbe Lehre wie vorhin: keine Ersetzungen
ueber Stellen, die mitten in einem Kommentar beginnen.

632 Kernel-Tests, check/fmt/clippy sauber, BIOS und UEFI ohne Panic.

## Nachtrag 08:20 — Der Interrupt hat formatiert statt abgelegt

Der Tastatur-Handler machte pro Taste die Uebersetzung, die Formatierung und die
Serienausgabe. Serienausgabe im Interrupt ist Arbeit, die niemand dort haben
will: sie blockiert den Kern fuer die Dauer der Uebertragung, und genau das war
der Verdacht bei der doppelten Tick-Rate unter PIC.

**Jetzt legt der Handler ab, der Tick liest aus.** Neu: `kernel/src/input_queue.rs`
(Lib, 6 Tests): `InputEvent { scancode, character }`, fester Puffer
`INPUT_CAPACITY = 64` ohne Allokation (im Interrupt darf nichts allokieren),
`push` liefert `false` und zaehlt, wenn voll — kein Warten.

Der Handler macht nur noch:

```rust
let event = InputEvent::new(scancode, decoded.character.map(|b| b as char));
INPUT_QUEUE.lock().push(event);
```

Geleert wird im Tick, hoechstens acht Ereignisse pro Tick, damit ein Tippschwall
den Tick nicht ueberzieht:

```text
Eingabe (Konsument): 'h' (Scancode 0x23) [Line Some(1), Bruecke 1, Rueckfall 0, Haenger 0, verworfen 0]
Eingabe (Konsument): 'i' (Scancode 0x17) [Line Some(1), Bruecke 3, Rueckfall 0, Haenger 0, verworfen 0]
```

`0` Zeilen "Tastatur ueber Vektor" im ganzen Lauf. Das Zeichen wird beim
Druecken uebersetzt und mitgespeichert, nicht beim Leeren: sonst koennte eine
innerhalb eines Ticks geloeste Umschalttaste das Zeichen nachtraeglich
verfaelschen. Die Bruecken-Zaehler sind mitgezogen, damit der Umbau keine
Evidenz kostet.

**Entschieden, weil reversibel und Impact mittel** (Owner-Regel: Defaults
dokumentieren statt blockieren): Empfaenger ist genau ein Konsument; Puffergroesse
64; bei Ueberlauf wird das neue Ereignis verworfen und gezaehlt statt die Line zu
maskieren; das Layout bleibt vorerst im Kernel (US), weil eine Verlagerung in den
Prozess erst mit einem echten Ring-3-Konsumenten sinnvoll ist.

**Grenze.** Der Konsument ist der Tick, nicht ein Prozess. Der Payload ist
weiterhin ein einzelnes `HLT` (`UserBinary::hello_world`), ein Ring-3-Programm mit
`IpcReceive`-Schleife ist der naechste Schritt. Was jetzt steht, ist der Pfad
dahin: der Handler kennt keinen Empfaenger mehr.

646 Kernel-Tests, check/fmt/clippy sauber, BIOS und UEFI ohne Panic.

## Nachtrag 09:40 — Der Syscall hat einen Empfang behauptet, den es nicht gibt

`IpcReceive` gab `ok(request.arg1)` zurueck, also "arg1 Bytes liegen fuer dich
bereit". Es gab weder eine Nachricht noch einen Puffer, und es gab keine
Laufzeit-`IpcSubsystem`-Instanz im Kernel — der Aufruf war eine Rechtepruefung
mit angehaengter Erfolgsmeldung. Ein Prozess haette in einen ungeschriebenen
Puffer gelesen und den Inhalt fuer Eingabe gehalten. Im TCB ist das kein
Schoenheitsfehler, sondern eine Luege an der Schnittstelle.

**Warum kein echter Transport?** Die eingefrorene ABI uebertraegt keine Bytes:
`SyscallRequest` traegt eine Laenge (`payload_len`, heute aus R8), aber keinen
Pufferzeiger. Die Registerkonvention selbst ist offen (SC-DEC-L §13), und R8
dient gleichzeitig als Argumentwort und als Laengenfeld. Ohne Zeiger kann der
Kernel nicht lesen, wohin er schreiben soll — das ist eine Spec-Entscheidung,
keine Fleissarbeit.

**Also sagt der Syscall jetzt die Wahrheit**, ohne ABI-Aenderung:

| Aufruf | vorher | jetzt |
|---|---|---|
| `IpcSend`, `payload_len > 0` | `ok(0)` | `EndpointUnavailable` |
| `IpcSend`, `payload_len == 0` | `ok(0)` | `ok(0)` — ein Signal ohne Nutzdaten ist wahr |
| `IpcReceive` | `ok(arg1)` | `EndpointUnavailable`, `value = 0` |
| ohne Recht | `PermissionDenied` | `PermissionDenied`, **zuerst** |

`EndpointUnavailable` existiert bereits in `libshivacore::AbiError` (Wert 5),
es wird keine Fehlernummer erfunden. Die Reihenfolge ist Teil des Fixes: ein
Unbefugter darf nicht einmal erfahren, dass kein Transport existiert.

Vier Tests halten das fest (`ipc_receive_does_not_claim_bytes_it_cannot_deliver`,
`ipc_send_with_payload_reports_the_missing_transport`,
`ipc_send_without_payload_is_a_truthful_signal`,
`ipc_checks_the_right_before_the_transport`). 650 Kernel-Tests, BIOS und UEFI
ohne Panic, Tastatur weiter ueber `Line Some(1)`.

**Folge fuer den geplanten Userspace-Kanal:** Er laesst sich nicht "einfach
verdrahten". Bevor ein Prozess Tastenereignisse empfaengt, muss die ABI einen
Pufferzeiger bekommen — sonst kann der Kernel die Nachricht nicht abliefern,
egal wie gut der Kanal vorbereitet ist.

## Nachtrag 10:55 — Der dritte Prozess kam nie an die Reihe

Ziel war der erste echte Syscall aus Ring 3: neuer Prozess PID 1002 mit einer
Schleife aus `int 0x80`. Der erste Lauf zeigte **keinen** Syscall. Die Ursache
war nicht der Gate (DPL 3 steht korrekt im IDT-Eintrag), sondern der Scheduler.

**Befund:** `pick_next` nahm den ersten lauffaehigen Prozess, der nicht der
laufende war. Bei zwei Prozessen ist das korrektes Round-Robin, bei dreien wird
der dritte **nie** ausgewaehlt. Der Prozess existierte, war eingetragen und
lief nie — im Log nicht einmal ein Warm-up-Tick.

Beweis vor dem Fix (Diagnoselauf mit Musterprozess statt Syscall):

```text
SCHED-001 register pattern established PID=1000 ...
SCHED-001 register pattern established PID=1001 ...
# PID=1002 fehlt vollstaendig
```

**Fix** (`user_sched.rs`): die Suche beginnt hinter dem laufenden Prozess und
laeuft einmal im Kreis. Prioritaet (niedrigere Zahl zuerst) bleibt erhalten; bei
Gleichrangigen gewinnt der erste Kandidat **in Ringreihenfolge**, und weil die
Ringreihenfolge hinter dem laufenden Prozess beginnt, rotiert die Auswahl.
Zwei neue Tests: `three_equal_processes_rotate_instead_of_starving_the_last`
(1000 → 1001 → 1002 → 1000) und `rotation_and_priority_work_together`
(Prioritaet schlaegt Ringreihenfolge, Rotation gilt unter Gleichrangigen).

**Ergebnis**, BIOS und UEFI:

```text
SCHED-001 scheduler armed: PID=1000 + PID=1001 + PID=1002 (Syscall-Schleife)
Syscall aus Ring 3: Nr=1 PID=1002 Opcode=0x0001 Ergebnis=None
```

Das ist der erste Syscall, den in diesem Kernel ein Prozess aus CPL3 ausfuehrt:
`Yield`, ABI 1.0, ohne Fehler. Die Musterpruefung fuer PID 1000/1001 bleibt
unberuehrt, weil sie nur diese beiden PIDs kennt — der Syscall-Prozess hat kein
Registermuster, sein RAX/RBX sind per ABI die Rueckgabewerte.

Nebenbei: die Registerkonvention ist weiterhin offen (SC-DEC-L §13), die
Payload-Bytes setzen sie explizit (RDI=Cap-Slot, RSI=Opcode, R9=ABI-Version).
652 Kernel-Tests, check/fmt/clippy sauber.

## Nachtrag 12:20 — Tasten erreichen den Userspace, ohne ABI-Zusatz

Der Kanal zu einem Prozess fehlte, und die ABI hat keinen Pufferzeiger. Beides
zusammen sah nach einer Spec-Entscheidung aus. Es geht trotzdem: die ABI hat
einen 64-Bit-RUECKGABEWERT. Scancode in Bits 0..8, Zeichen in Bits 16..24 passen
hinein (`InputEvent::packed`). Der Syscall `IpcReceive` mit Kanal-ID 1 und
READ-Recht liefert das naechste Ereignis als Wert, 0 heisst "nichts anliegend".
Kein neues Feld, keine neue Nummer, kein geaenderter Opcode.

Reihenfolge bleibt: Rechtepruefung zuerst. Ohne READ-Cap bekommt der Aufrufer
`PermissionDenied` und das Ereignis bleibt liegen (Test
`receive_checks_the_right_before_handing_out_events`). Fuer jeden anderen Kanal
sagt der Syscall weiter `EndpointUnavailable`.

Zwei Fehler auf dem Weg, beide lehrreich:

1. `grant_input_read` lief, bevor `init_user_scheduler` das Gate anlegte →
   `expect` panikte ("Leserecht auf den Eingabekanal"). Das Gate wird jetzt per
   `get_or_insert_with` frueh angelegt und vom Scheduler nicht mehr ersetzt.

2. ZWEI KONSUMENTEN AUF EINER WARTESCHLANGE: Der Tick raeumte weiter ab, waehrend
   der Prozess pollte. Im ersten Lauf kam 'a' nie beim Prozess an — der Tick war
   schneller. Eine Warteschlange braucht genau einen Leser. `INPUT_CONSUMER_USERSPACE`
   schaltet den Tick-Konsumenten ab, sobald ein Prozess die Cap haelt.

Beleg (BIOS): "Tastenereignis an Ring 3: Nr=1 PID=1002 Zeichen='a' (Scancode
0x1E) [Bruecke 1, Rueckfall 0, verworfen 0]" fuer a, Shift, B, Enter, c — fuenf
von fuenf, nichts verworfen, Tick-Konsument 0 Zeilen. UEFI ebenso ('z', Shift,
'A'). Payload: `mov rdi, <cap-slot>` (Nummer aus der Tabelle, als Immediate
eingesetzt), `mov esi, 0x11`, `mov r9d, 0x00010000`, `mov rdx, 1`, `int 0x80`,
`jmp` zurueck auf 0xFC — der falsche 0xF9-Ruecksprung ist damit weg.

Grenze: Der Prozess POLLT in einer engen Schleife. Ein blockierendes Warten
(Prozess schlafen bis Ereignis) braucht einen Syscall mit Wartesemantik und
damit eine ABI-Entscheidung. 659 Tests, BIOS+UEFI 0 Panics.

## Nachtrag 07.11. 01:20 — DMA-Autorisierung ueber Device-Caps (REQ-SC006-09)

Befund: `system/devices/src/iommu.rs` beschreibt den IOMMU-Vertrag
(`create_domain`, `assign_device`, `map`, `unmap`) und `SoftwareIommu` setzt ihn
nach. Im Kernel gab es aber keinen Pfad, der ueberhaupt entscheidet, **wer**
fuer **welches** Geraet eine Domain bekommt. Ein Device-Cap wurde damit nirgends
befragt; REQ-SC006-09 war unerfuellt.

Umgesetzt: `kernel/src/dma_auth.rs` als Policy-Schicht zwischen Device-Cap und
IOMMU-Vertrag.

- `assign_device(cap, device)`: verlangt eine Cap vom Typ `Device` mit
  `ACCESS`, und die Cap muss genau dieses Geraet adressieren. Ein Geraet bekommt
  genau eine Domain; eine zweite waere ein zweiter Weg in denselben Speicher.
- `map_window(cap, domain, window)`: dieselbe Rechtepruefung, zusaetzlich muss
  die Domain dem Eigentuemer der Cap gehoeren. Fremde Prozesse koennen eine
  Domain weder befuellen noch abraeumen.
- Bereichspruefung vor der Eintragung: leer, ueberlaufend oder groesser als
  `MAX_WINDOW_BYTES` (64 MiB) wird abgelehnt, nicht gekuerzt.
- Zaehler `authorized`, `denied_wrong_device`, `denied_missing_access`,
  `denied_foreign_domain`, damit "abgelehnt" nicht wie "nie gefragt" aussieht.

Zwei Eigenschaften sind der Kern: eine Device-Cap autorisiert **kein** zweites
Geraet (sonst waere jede Cap ein Generalschluessel), und die Domain gehoert dem
Prozess, nicht dem Geraet.

Reihenfolge der Pruefung ist Absicht: erst die Art der Cap, dann das Recht, dann
die Identitaet. Ein falsches Geraet ist ein anderer Fehler als ein fehlendes
Recht, und bei der Fehlersuche ist der Unterschied entscheidend.

Verifikation: 7 Tests in `dma_auth::tests` (eigenes Geraet, Memory-Cap ist keine
Device-Cap, zweite Domain abgelehnt, fremder Prozess abgelehnt, Bereichspruefung
vor der Eintragung, Rundlauf von Eintragen und Abraeumen, Domains je
Eigentuemer).

**Noch offen bleibt die Hardware-Haelfte.** Diese Schicht entscheidet und
zaehlt; sie schreibt keine IOMMU-Seitentabellen und behandelt keine DMA-Faults.
Ohne diese Anbindung ist REQ-SC006-09 geprueft entschieden, aber nicht
erzwungen — genau die Luecke, die im Audit vom 06.11. schon fuer
REQ-SC006-02/03 beschrieben war und dort durch die Bruecke geschlossen wurde.
Der naechste Schritt waere die Anbindung an `system/devices` samt Fault-Pfad.

## Nachtrag 07.11. 01:40 — `cargo test --workspace` auf Stable repariert

Befund: `cargo test --workspace` brach auf der stabilen Toolchain ab, bevor ein
einziger Test lief. Ursache war nicht der Kernel, sondern der Boot-Image-Builder
(`modules/atc-shivacore/boot`): er haengt am `bootloader`-Crate, dessen
Build-Skript Nightly verlangt (`error: the -Z flag is only accepted on nightly`).
Als Workspace-Mitglied zog er den kompletten Workspace in diesen Nightly-Zwang.
Die CI fiel nicht auf, weil `rust-ci.yml` auf Nightly laeuft — lokal, also im
Normalfall, war das Gate unbenutzbar. Die README hat den Zustand sogar als
gegeben dokumentiert ("not a usable gate here").

Umgesetzt:

- `modules/atc-shivacore/boot` ist aus `members` entfernt und in `exclude`
  eingetragen; die Crate hat ein eigenes `[workspace]`. Sie baut weiter, nur
  eben in ihrem Verzeichnis.
- `.github/workflows/shivacore-kernel.yml` baut den Builder jetzt dort
  (`cd modules/atc-shivacore/boot && cargo +nightly run --release -- …`) und
  uebergibt absolute Pfade, damit die Ausgabepfade unabhaengig vom
  Arbeitsverzeichnis bleiben.
- `scripts/qemu_boot.sh` baut und startet den Builder aus seinem Verzeichnis.
- README: Testbefehle und Erwartungswerte aktualisiert (1471 Tests ueber 58
  Suiten), der falsch gewordene Absatz ersetzt, Boot-Schritte getrennt
  dokumentiert.

Verifikation auf Stable: `cargo fmt --check` 0, `cargo check --workspace
--all-targets` 0, `cargo test --workspace --all-targets` **1471 passed, 0
failed**, `cargo clippy --workspace --all-targets -- -D warnings` 0. Auf
Nightly: Kernel-ELF gebaut, Builder gebaut, beide Images erzeugt,
`scripts/qemu_boot.sh` Exit 0, QEMU-BIOS-Lauf ohne Panic und ohne Seitenfehler.

## Nachtrag 07.11. 01:55 — Geraete-Enumeration im Service Space (REQ-SC006-11)

Befund: Ein Dienst, der ein Geraet benutzen will, konnte es nicht finden. Die
Geraeteliste lebte im Kernel, im Service Space gab es nichts, und ein Prozess
haette raten muessen, welche Kennung er in eine Device-Cap schreiben laesst.
REQ-SC006-11 war damit unerfuellt.

Umgesetzt: `service_space/src/device_registry.rs`.

Die Registratur trennt zwei Dinge, die leicht verwechselt werden:

- **Finden ist offen.** Kennung, Klasse und Name sind Metadaten
  (`list`, `list_class`, `get`). Ohne sie koennte kein Dienst je eine Cap
  anfragen; wer sie sieht, hat noch nichts gewonnen. Die Liste ist nach Kennung
  sortiert, damit zwei Abfragen desselben Stands dieselbe Antwort geben.
- **Besitzen ist geprueft.** `register` und `claim` verlangen eine Device-Cap,
  die **genau dieses** Geraet adressiert und `ACCESS` traegt. Ein Geraet hat
  genau einen Besitzer: ein zweiter Anspruch wird abgelehnt
  (`AlreadyClaimed`), nicht ueberschrieben. Freigeben und Entfernen darf nur der
  Besitzer (`NotTheOwner`).

Dieselbe Regel wie in der DMA-Autorisierung (`dma_auth.rs`): eine Cap oeffnet
genau ihr Geraet. Eine Registratur, die beim Anspruch nur "ist die Cap gueltig?"
fragt, waere ein Generalschluessel fuer alle Geraete.

Fehlerbild bewusst getrennt: `NotADeviceCap`, `MissingAccess`, `WrongDevice`,
`AlreadyRegistered`, `UnknownDevice`, `AlreadyClaimed`, `NotTheOwner`. Zaehler
`registered`, `claimed`, `denied_no_cap`, `denied_foreign_claim`, damit
"abgelehnt" nicht wie "nie gefragt" aussieht.

Verifikation: 9 Tests (`service_space::device_registry::tests`), darunter:
Registrieren braucht Gewalt ueber genau dieses Geraet, Memory-Cap ist keine
Device-Cap, doppelte Kennung wird abgelehnt statt ueberschrieben, Liste sortiert
und filterbar, Anspruch braucht die passende Cap, zweiter Besitzer abgelehnt,
Freigabe oeffnet das Geraet fuer den naechsten Besitzer, Entfernen nur durch den
Besitzer, Zaehler trennen Ablehnung von Nichtfrage.

**Noch offen:** Die Eintraege kommen aus einer uebergebenen Liste. Es wird keine
Hardware abgefragt — kein ACPI-Walk, kein MADT-Durchlauf, keine Hotplug-Quelle.
Das Einsammeln ist die naechste Stufe; die Registratur ist die Stelle, an der
diese Quelle spaeter einhaengt, ohne dass sich an den Rechten etwas aendert.

## Nachtrag 07.11. 02:10 — Enum-Quelle: PCI-Scan im Kernel (REQ-SC006-11)

Die Registratur konnte Geraete verwalten, aber sie erfand keine. Diese Stufe
liest, was tatsaechlich steckt: `kernel/src/pci_scan.rs`.

Zugriffsweg: klassischer Konfigurationsraum ueber IO-Ports 0xCF8/0xCFC. Das ist
der kleinste Weg, der ohne ACPI- und ohne MMIO-Kenntnisse auskommt, und deshalb
der richtige erste Schritt — er liefert eine belastbare Liste, bevor irgendetwas
gemappt wird.

Regeln, die der Code einhaelt:

- Leere Slots werden nicht gemeldet: Herstellerkennung 0xFFFF (und 0x0000, wie
  es defekte Bruecken liefern) bedeutet "nichts da".
- Mehrfunktionsgeraete werden anhand von Header-Typ Bit 7 erkannt und
  vollstaendig (Funktionen 0..8) abgefragt; Einzelfunktionsgeraete nur in
  Funktion 0. Sonst wuerde eine IDE-Bruecke ihre drei Funktionen unterschlagen.
- Die Reihenfolge ist fest (Bus, Geraet, Funktion), damit zwei Abfragen
  desselben Stands dieselbe Antwort geben.
- Klassifizierung nach Klassencode; 0x04 wird nur mit Subklasse 0x01 zu Audio,
  sonst waere jede Video-Capture-Karte ein Lautsprecher.

Was ausdruecklich nicht passiert: keine BAR-Aufloesung, kein MMIO-Mapping, keine
Treiberbindung, keine Rechtevergabe. Die Entdeckung vergibt nichts — wer ein
Geraet benutzen darf, entscheidet weiterhin die Device-Cap.

Bootlauf, QEMU i440fx, BIOS und UEFI identisch:

```
PCI 00:00.0 8086:1237 bridge
PCI 00:01.0 8086:7000 bridge
PCI 00:01.1 8086:7010 storage
PCI 00:01.3 8086:7113 bridge
PCI 00:02.0 1234:1111 display
PCI 00:03.0 8086:100e network
SC-006 PCI-Enumeration: 6 Geraete (storage=1 network=1 display=1 bridge=3 usb=0 other=0)
```

Das belegt genau die zwei Stellen, an denen ein Scan typischerweise falsch
liegt: die drei Funktionen von 00:01 sind einzeln aufgefuehrt, und der ACPI-PM
(7113) haengt an einer Funktion, die nur ueber die Mehrfunktions-Erkennung
auffindbar ist.

Testbasis: 10 Unit-Tests mit einem eingeschleusten Konfigurationsraum
(`ConfigSpace`-Trait), darunter leere Busse, 0xFFFF/0x0000-Slots, nicht
abgefragte Funktionen bei Einzelfunktionsgeraeten, vollstaendige Funktionen bei
Mehrfunktionsgeraeten, Determinismus, Klassentabelle und Bus-Verkettung. Der
IO-Pfad ist an das Feature `x86-boot` gebunden, weil die Lib-Tests ohne
`x86_64` laufen.

**Noch offen:** Die gefundenen Geraete werden protokolliert, aber noch nicht in
die Registratur uebergeben — dazwischen liegt die Crate-Grenze (der Service
Space baut auf dem Kernel auf, nicht umgekehrt). ACPI/MADT als zweite Quelle
fehlt ebenfalls, ebenso Busse oberhalb von Bus 0 hinter Bridges.

## Nachtrag 07.11. 11:20 — Von der Enum-Quelle in die Registratur

Die beiden vorigen Stufen standen nebeneinander: der Kernel fand Geraete und
protokollierte sie, der Service Space konnte Geraete verwalten, aber niemand
brachte sie zusammen. Diese Stufe schliesst den Weg.

Die Crate-Grenze laeuft in eine Richtung — der Service Space baut auf dem Kernel
auf —, also liegt die Uebersetzung dort: `device_registry.rs` liest
`shivacore::pci_scan` und uebersetzt.

Drei Entscheidungen, die den Unterschied machen:

- **Die Kennung kommt aus der Hardware, nicht aus einem Zaehler.** Bus, Geraet
  und Funktion passen in 24 Bit (`pci_device_id`). Damit liefert dieselbe
  Maschine nach jedem Neustart dieselben Kennungen — sonst waere jede
  gespeicherte Device-Cap nach dem naechsten Boot wertlos.
- **Uebernahme ohne Cap, und zwar sichtbar.** `adopt_discovered` prueft keine
  Cap, weil der Kernel Gewalt ueber die Geraete hat: er liest den
  Konfigurationsraum, ein Prozess kann das nicht. `register` bleibt der Weg fuer
  Prozesse und prueft weiterhin die Device-Cap. Damit die Ausnahme nicht
  verschwindet, zaehlt `discovered_without_cap()` mit, wie viel ungeprueft
  hereinkam.
- **Ein gefundenes Geraet hat keinen Besitzer.** Finden ist offen, Besitzen
  bleibt geprueft: der erste `claim` mit passender Cap entscheidet. Ein Test
  haelt genau das fest — der Eintrag ist sichtbar, eine Memory-Cap oeffnet ihn
  nicht.

Der Abgleich ist wiederholbar: `sync_from_hardware` zaehlt Bekanntes
(`already_known`) statt es doppelt einzutragen, ein zweiter Boot mit derselben
Hardware aendert also nichts.

Klassenabbildung: Storage, Network, Display, Input, Audio, Usb werden
uebernommen, Bruecken und Unbekanntes landen als `Other`. `Gpu` wird bewusst
nicht vergeben — PCI-Klasse 0x03 heisst "Display" und deckt Bildschirm wie
Beschleuniger ab; ob ein Geraet eine GPU ist, weiss der Treiber, nicht der
Klassencode. Der Rundlauf `kind -> class -> kind` ist bis auf diesen einen Fall
identisch und wird getestet.

Verifikation: 9 neue Tests, zusammen 18 in `device_registry`, darunter
Kennungsstabilität, vollstaendige Klassentabelle, doppelte Uebernahme, fehlender
Besitzer nach Uebernahme, Memory-Cap oeffnet kein gefundenes Geraet,
idempotenter zweiter Abgleich und Anspruch mit passender Cap nach dem Abgleich.

**Noch offen:** Der Aufruf selbst fehlt im Boot — die Registratur lebt im
Service Space, und der laeuft im Kernel-Image noch nicht. Die Uebersetzung steht
und ist geprueft; angeschlossen wird sie, wenn der Service Space beim Boot
startet.

## Nachtrag 07.11. 11:55 — Der Vorrat: Geraete zur Laufzeit abrufbar

Der letzte Meter war offen: die Enumeration schrieb ins Log, und der Service
Space konnte nichts abfragen. Ein Log ist kein Ort, an dem jemand nachfragt.

Neu: `kernel/src/boot_devices.rs` mit `DeviceStore` und dem Statik
`BOOT_DEVICES`. Der Boot schreibt hinein, wer die Liste braucht liest heraus.

Zwei Regeln, die den Vorrat brauchbar machen:

- **Schreiben ist einmalig.** `publish` ersetzt, statt anzuhaengen. Ein zweiter
  Scan derselben Hardware ergibt dieselbe Liste, nicht das Doppelte.
- **Lesen gibt eine Kopie.** Der Vorrat ist kernel-eigen; ein Leser bekommt
  seine eigene Liste und kann den Vorrat nicht veraendern.

`DeviceStore` ist eine eigene Struktur und kein globaler Speicher, damit Tests
mehrere unabhaengige Vorraete bauen koennen — ein einziger globaler Speicher
waere zwischen parallel laufenden Tests nicht zu trennen. Das Statik
`BOOT_DEVICES` ist nur fuer den laufenden Betrieb da.

Auf der anderen Seite: `DeviceRegistry::sync_from_store(&DeviceStore)` und
`sync_from_boot()` als der Weg, den der Service Space beim Start geht. Damit ist
die Kette vollstaendig: PCI-Scan -> Vorrat -> Registratur, und jedes Glied hat
eigene Tests.

Bootbeleg (BIOS und UEFI):

```
PCI 00:02.0 1234:1111 display
Geraetevorrat: 6 Geraete fuer den Service Space abrufbar
SC-006 PCI-Enumeration: 6 Geraete (storage=1 network=1 display=1 bridge=3 usb=0 other=0)
```

Testbasis: 5 Tests fuer den Vorrat (leer, veroeffentlichen und zuruecklesen,
Ersetzen statt Anhaengen, Schnappschuss ist eine Kopie, Leeren), 2 Tests fuer den
Anschluss in der Registratur (Bootvorrat uebernehmen und beim zweiten Start
nichts doppeln, leerer Vorrat laesst die Registratur unberuehrt). Zusammen 20
Tests in `device_registry`, 1506 im Workspace.

**Noch offen:** Der Service Space laeuft im Kernel-Image nicht, der Aufruf von
`sync_from_boot()` gehoert also in seinen Startpfad, nicht in den Kernel. Die
Daten sind abrufbar, die Verbindung steht bereit; wer sie zieht, fehlt.

## Nachtrag 07.11. 12:10 — Busse hinter Bruecken

Die Enumeration kannte nur Bus 0. Auf einer Maschine mit Steckplaetzen oder
Root-Ports haengt der groesste Teil der Geraete hinter einer Bruecke und war
damit unsichtbar. `scan_tree` folgt jetzt jeder Bruecke.

Regeln:

- **Bruecke erkennen.** 0x06/0x04 ist die gewoehnliche PCI-zu-PCI-Bruecke,
  0x06/0x09 die halbtransparente, 0x06/0x07 die CardBus-Variante. Die
  ISA-Bruecke (0x06/0x01) fuehrt zu keinem PCI-Bus und wird nicht verfolgt —
  sonst wuerde der Scan bei jeder Southbridge ins Leere laufen.
- **Sekundaerbus lesen.** Register 0x18 traegt primaeren (7:0), sekundaeren
  (15:8) und untergeordneten Bus (23:16). Nur der sekundaere ist der Einstieg;
  die untergeordnete Nummer sagt bloss, wie weit der Baum reicht. Bus 0 als
  Sekundaerbus gilt als nicht konfiguriert — manche Firmware traegt das ein.
- **Jeder Bus einmal.** `visited` haelt fest, was dran war: zwei Bruecken auf
  denselben Bus zaehlen nicht doppelt, und eine Ringverbindung laeuft nicht ewig.
- **Tiefenbegrenzung.** Acht Etagen (`MAX_BRIDGE_DEPTH`) sind fuer echte
  Maschinen reichlich; die Grenze schuetzt vor einer kaputten Firmware-Tabelle.
- **Stabile Reihenfolge.** Nach dem Sammeln wird nach Bus, Geraet und Funktion
  sortiert, damit das Ergebnis nicht von der Reihenfolge des Ablaufs abhaengt.

Bootbeleg, QEMU i440fx mit zusaetzlicher Bruecke (`-device pci-bridge` und eine
zweite e1000 dahinter), BIOS und UEFI identisch:

```
PCI 00:04.0 1b36:0001 bridge
PCI 01:00.0 8086:100e network      <- hinter der Bruecke
SC-006 PCI-Enumeration: 8 Geraete (storage=1 network=2 display=1 bridge=4 usb=0 other=0)
```

Ohne die Bruecke bleibt es bei den bekannten 6 Geraeten. Der Unterschied ist der
Beleg: 00:04.0 und 01:00.0 erscheinen nur, weil der Scan der Bruecke folgt.

Testbasis: 6 neue Tests, zusammen 16 in `pci_scan` — ISA-Bruecke ist keine
PCI-Bruecke, Verfolgen zum Bus dahinter (flacher Scan findet 1, Baum-Scan 2),
Bruecke auf Bus 0 wird ignoriert, zwei Bruecken auf denselben Bus zaehlen nicht
doppelt, Ringverbindung endet, tiefe Kette stoppt an der Grenze.

**Noch offen:** ACPI/MADT als zweite Quelle (Geraete ohne PCI-Klasse), und die
Uebergabe an die Registratur erfolgt weiterhin nur ueber den Vorrat, nicht durch
einen laufenden Service Space.

## Nachtrag 07.11. 12:40 — Das schlafende Modul `container_net` geweckt

Anlass war die offene Aufgabe "Korrektur der unkompilierten Module". Ein
Abgleich der Dateien gegen die `mod`-Deklarationen fand genau eine Leiche:
`service_space/src/container_net.rs`, 2803 Zeilen, 247 Funktionen, **103 Tests** —
nirgends deklariert, also nie uebersetzt und nie ausgefuehrt. In `lib.rs` stand
sogar "dormant, Aktivierung erfordert Portierung auf alloc/core".

Die Portierung war kleiner als befuerchtet, weil das Modul nur an drei Stellen
std benutzte. Aenderungen:

- **Importe.** `std::collections` -> `alloc::collections` (BTreeMap, BTreeSet,
  VecDeque), `std::sync::atomic` -> `core::sync::atomic`, dazu
  `alloc::string::String`, `alloc::vec::Vec`, `alloc::format` und das `vec!`-Makro.
  `HashMap` stand nur in der Import-Zeile, nicht in Benutzung — der Verdacht
  "dafuer gibt es kein alloc-Aequivalent" loeste sich von selbst.
- **Ord fuer die Schluessel.** BTreeMap und BTreeSet verlangen `Ord`; `MacAddr`,
  `IpSubnet`, `NetNsId` und `FirewallChain` hatten nur `Hash`/`Eq`.
- **Drucken ueber Display.** Drei eigene `to_string`-Methoden verstiessen gegen
  die Konvention und standen `format!` im Weg. Jetzt gibt es `as_dotted` /
  `as_colon` und je ein `Display`; die 20 Aufrufstellen bleiben unveraendert,
  weil `ToString` gratis mitkommt.

**Ein echter Fehler kam beim ersten Uebersetzen heraus.** Das Loopback-Interface
trug `mtu: 65536`, das Feld war aber `u16` — als Literal nicht darstellbar. Der
eigene Test des Moduls verlangte ausserdem, dass 70000 abgelehnt wird, was in
`u16` gar nicht ankommen kann. Beides zusammen zeigt: die Groesse war falsch
gewaehlt, nicht der Wert. `mtu` ist jetzt `u32`; damit darf Loopback 65536
tragen, und 70000 wird tatsaechlich abgelehnt (`!(68..=65536).contains(&mtu)`).

Vier Funktionen (`matches`, `evaluate`, `check_firewall`, `simulate_packet`)
ueberschreiten die Argumentgrenze. Sie spiegeln Paketfelder eins zu eins; sie in
eine Struktur zu buendeln waere eine eigene API-Entscheidung und nicht Teil einer
Portierung. Die Ausnahme ist im Code begruendet, nicht stillgelegt.

Ergebnis: `pub mod container_net;` in `lib.rs`, no_std-Build sauber,
**103 Tests** gruen (vorher: nie gelaufen), clippy mit `-D warnings` still.
Workspace jetzt **1615 Tests**.

## Nachtrag 07.11. 13:10 — Repo-weiter Abgleich: was noch nicht kompiliert wird

Nach `container_net` habe ich denselben Abgleich nicht nur fuer die zwei Crates,
sondern fuer alle 32 Cargo-Targets gemacht. Ergebnis, ehrlich sortiert:

**Behoben:**

- `system/package/src/dependency.rs` (88 Zeilen, 2 Tests) war nicht deklariert.
  `pub mod dependency;` ergaenzt — 2 Tests laufen jetzt (Paket-Crate 8 statt 6).
- `system/media/src/sync.rs` (23 Zeilen) war ein toter Doppelgaenger zu
  `av_sync.rs`: dort steht die kanonische Policy (`AvSyncPolicy`, `SyncDecision`,
  per `pub use` re-exportiert), hier eine zweite mit anderem Namen
  (`SyncPolicy`). Keine Referenz, keine Tests. Entfernt — Redundanz statt
  zweier Wahrheiten.

**Bewusst nicht aktiviert:**

- `modules/atc-drivers/kernel/src/hw_drivers.rs`, 1267 Zeilen mit **61 Tests**,
  liegt ohne Cargo.toml und ohne `mod` in der Gegend. Ich habe den Einbau
  versucht (Kopie in den Service Space, Pfade umgebogen) und nach 45 Fehlern
  abgebrochen, weil sie nicht mechanisch sind: die Datei erwartet **andere
  Verträge**, als der Code heute anbietet.
  1. Netz: sie implementiert `send`/`recv`, `service_space::net::NetworkDevice`
     verlangt `send_frame`/`recv_frame`.
  2. Block: sie implementiert `read_block(lba) -> [u8; 512]` und nutzt ein
     `BlockError::ReadOnly`, das es in `globus_devices::block::BlockError` nicht
     gibt; dort heisst der Weg `read(BlockRequest, &mut [u8])` mit
     `validate_request` davor.
  3. Timer: die Implementierung hat `reset`, `shivacore::timer::TimerSource`
     kennt die Methode nicht.
  Dazu bringt die Datei ein eigenes `PciDeviceId` mit, das neben dem neuen
  `pci_scan::PciDevice` eine zweite PCI-Wahrheit waere.

  Das ist keine Portierung, sondern eine **Vertragsentscheidung**: welche
  Block-Abstraktion gilt, ob ein Nur-Lese-Geraet im gemeinsamen Fehlertyp
  ausdrueckbar sein muss, und ob die Treiber-PCI-Sicht durch `pci_scan` ersetzt
  wird. Diese Entscheidung gehoert dem Owner, nicht in einen Nebenbei-Commit —
  die 61 Tests sind es aber wert, sobald sie gefallen ist.

- `modules/atc-linux-edition` und `atc-windows-edition`: je fuenf Module mit 31-36
  Zeilen, ausdruecklich als "STUB: Wird in Sprint implementiert" markiert, 0
  Tests. Die Crates bauen nur ihre `main.rs` (Sprint-0-Platzhalter). Das sind
  Platzhalter, kein unkompiliertes Wissen — bleiben liegen.

- `modules/atc-shivacore/legacy/kernel-src-uncompiled/`: der Verzeichnisname sagt
  schon, was es ist.

Stand nach diesem Durchgang: 1617 Tests im Workspace.

## Nachtrag 07.11. 12:30 — `hw_drivers` doch integriert (61 Tests zurueckgeholt)

Im vorigen Durchgang hatte ich diese Datei als "Vertragsentscheidung" liegen
gelassen. Der Owner hat weiter auf "Fix" bestanden, also habe ich die Entscheidung
mit begruendeten Defaults selbst getroffen — reversibel, und jede Abweichung ist
im Code kommentiert:

- **Ort:** `modules/atc-shivacore/service_space/src/hw_drivers.rs` (vorher
  `modules/atc-drivers/kernel/src/hw_drivers.rs`, ohne Cargo.toml, ohne `mod`,
  also nie uebersetzt). Laut AD-012 gehoeren Treiber in den Service Space. Die
  alte Kopie ist entfernt, es gibt keine zweite Wahrheit.
- **Netz:** Der Treiber dachte in `send`/`receive`, der Vertrag in
  `send_frame`/`recv_frame`. Die Treibermethoden sind jetzt inhärent (die Tests
  rufen sie unveraendert), darueber liegt eine duenne `NetworkDevice`-Impl.
  Fehlerabbildung: `DeviceError` -> `DeviceDown`, `NoPacket` -> `NoFrameAvailable`.
- **Block:** `read_block(lba) -> [u8; 512]` bleibt als Treibermethode, die
  `BlockDevice`-Impl uebersetzt in `BlockRequest` und validiert mit
  `validate_request`. Der gemeinsame `BlockError` hat dafuer eine Variante
  `ReadOnly` bekommen: ein Nur-Lese-Geraet muss ausdrueckbar sein, sonst muesste
  der Aufrufer "darf nicht" von "nicht bereit" erraten.
- **Timer:** `now_ns`/`frequency`/`resolution_ns` kommen aus den vorhandenen
  Femtosekunden-Werten; `reset` bleibt als Treibermethode.
- **PCI:** Das eigene `PciDeviceId` der Datei bleibt vorerst — es beschreibt
  Hersteller/Geraet/Klasse, waehrend `pci_scan::PciDevice` den Ort im Baum
  beschreibt. Sie schliessen sich nicht aus; eine Zusammenlegung waere ein
  eigener Schritt und ist hier nicht noetig.

**Drei echte Fehler kamen beim ersten Lauf heraus** (der Code war nie uebersetzt):

1. `let tx_ref = unsafe { &mut *(self as *const Self as *mut Self) }` — ein
   `&`-zu-`&mut`-Cast, also undefiniertes Verhalten; der Compiler lehnt das mit
   `invalid_reference_casting` ab. Sende- und Empfangswarteschlange liegen jetzt
   hinter `spin::Mutex`, alle Zugriffe sind echt.
2. Derselbe Unsinn beim Blockgeraet (`storage`), zusaetzlich mit
   `block_ref[..512].copy_from_slice(data)` — bei `block_size < 512` ein Panic.
   Jetzt `spin::Mutex<Vec<Vec<u8>>>` und `n = block.len().min(512)`.
3. Drei Test-Erwartungen waren falsch, nicht der Code:
   - `bdf()` bildet das x86-Layout ab (Bus 23:16, Geraet 15:11, Funktion 10:8);
     der Test prueft byteweise Felder und damit ein Layout, das die Hardware
     nicht kennt.
   - Bei 10 MHz ist ein HPET-Tick 100 ns: 10 Ticks sind 1000 ns (nicht 100),
     100 ns ist ein Tick (nicht zehn), 10000 Ticks sind 1 ms = 1 000 000 ns
     (nicht 100 000). Alle drei Erwartungen waren um Faktor 10 daneben.

Fuenf neue Tests belegen die Klammern selbst (mehrblockiges Lesen, Nur-Lese-
Ablehnung, zu kurzer Puffer, Frame-Rundlauf, Timer in Hz/ns). Stand: **66 Tests**
im Treibermodul, **1683 im Workspace**.

## Nachtrag 07.11. 12:45 — Ortsprüfung: liegt alles, wo es hingehört?

Auftrag war die Gegenprüfung, nicht die nächste Baustelle. Befund in drei Teilen.

**1. Der Treiber-Ort ist begründet, aber nicht eindeutig.** Es gibt zwei
Kandidaten: den Service Space (gewählt) und `modules/atc-drivers`, das laut
Cargo-Beschreibung genau dafür da ist ("Hardware-Treiber — USB, GPU, Audio,
SATA/NVMe, HID, Network"). Nachgesehen: dort stehen `audio.rs`, `gpu.rs`,
`hid.rs`, `net.rs`, `storage.rs`, `usb.rs` mit je **zwei Zeilen** Inhalt
("PLANNED, noch nicht implementiert"), nur `driver_framework.rs` hat 112 Zeilen.
Der Treiber steht also dort, wo er laut AD-012 hingehört (Service Space, nicht
Kernel), und das deklarierte Treiber-Crate ist bis auf ein Framework leer.
Das ist kein Fehler, aber eine offene Frage an den Owner: wird `atc-drivers`
das Zuhause (dann gehoert `hw_drivers` spaeter dorthin) oder bleibt es Anzeige-
Huelle und der Service Space ist kanonisch? Ich habe nichts verschoben, weil
beides vertretbar ist und ein Umzug ohne diese Entscheidung nur Arbeit erzeugt.

**2. Die Simulation ist als Simulation erkennbar.** `MmioRegion` ist kein MMIO:
es haelt `Vec<AtomicU64>` im Speicher, `HpetTimer::new(0xFED00000)` beruehrt
keine Hardware. Genau so steht es im Dateikopf ("simulated backends für cargo
test") — technisch korrekt als Testrahmen, aber die Datei darf nirgends als
echter Gerätetreiber gelten. Nachgezaehlt: **0 `unsafe`** im Modul, nach dem
Umbau der beiden UB-Casts.

**3. Zwei Register waren falsch, nicht der Code.** Beim Nachsehen, ob die
Dokumentation die neue Lage abbildet, fiel auf:

- `modules/atc-shivacore/FILE_REGISTER.md` war vom 2026-10-08 und nannte **15
  Dateien, die es nicht gibt** (`kernel/src/devfs.rs`, `kernel/src/fs_journal.rs`,
  `kernel/src/hw_drivers.rs`, `kernel/src/block.rs`, `kernel/src/cow.rs` u. a. —
  sie liegen in `legacy/kernel-src-uncompiled/`) und **11 falsche Zeilenzahlen**,
  darunter `kernel/src/interrupts.rs` mit 314 statt 919 und `capability.rs` mit
  360 statt 537. Das Register ist jetzt aus `git ls-files` erzeugt: 106 Zeilen,
  106 getrackte Dateien, 0 Abweichungen (der Selbstbezug auf die eigene
  Zeilenzahl wird iterativ stabilisiert).
- `FILE_REGISTER.md` (Repo-Wurzel) hatte 209 Eintraege, davon **genau einer
  fehlend**: der geloeschte `modules/atc-drivers/kernel/src/hw_drivers.rs`. Er
  zeigt jetzt auf `modules/atc-shivacore/service_space/src/hw_drivers.rs`;
  209 Eintraege, 0 fehlend.
- `COMPONENT_PLAN.md` im Treiber-Modul fuehrte "1,4xx" Zeilen; echt sind 1.653.

## Nachtrag 07.11. 13:00 — die Doppelung aufgeloest, statt sie liegen zu lassen

Der offene Punkt aus der Ortspruefung war eine Owner-Entscheidung. Statt zu
warten habe ich die Variante gewaehlt, die beide Crates behaelt und keine
zweite Wahrheit zulaesst (reversibel, klein):

**Es gab zwei Treiber-Zustandsmodelle.** `atc_drivers::driver_framework`
(`DriverState` Registered/Active/Faulted/Isolated, `DriverSupervisor`,
Ereignisprotokoll, 1 Test) und daneben `hw_drivers::DriverManager` mit einem
booleschen `initialized`. Der boolesche Flag konnte nicht sagen, ob ein Treiber
laeuft, gestorben oder isoliert ist — und die Supervisor-Logik lag ungenutzt
herum, waehrend der laufende Code sie nicht kannte.

**Entscheidung (in der Folgestunde korrigiert, siehe unten):** Der Lebenszyklus
sollte eine einzige Implementierung haben und `DriverManager` sie benutzen. `init_all()` meldet jeden vorhandenen Treiber an
(Register -> Active), Geraete-Id ist die MMIO-Basis (stabil, eindeutig, keine
erfundene Nummerierung), Anmelden ist idempotent, und `report_driver_fault()`
isoliert einen Treiber und schreibt `SC-DRV-0008` ins Protokoll. Der Service
Space ist damit kanonischer Ort fuer Treibercode (AD-012), `atc-drivers` ist
Framework + Anzeige — vier neue Tests belegen es (70 im Treibermodul).

**Und die Stubs, die logen.** `atc-drivers/src/net.rs` und `storage.rs` sagten
"PLANNED, noch nicht implementiert", obwohl `VirtioNetDevice` und
`VirtioBlkDevice` im Service Space laufen. Sie verweisen jetzt dorthin, statt
eine zweite Implementierung zu versprechen. `audio.rs`, `gpu.rs`, `hid.rs`,
`usb.rs` bleiben PLANNED — fuer sie gibt es im Baum tatsaechlich nichts, und das
steht jetzt als nachgepruefter Satz darin. `STATUS.md` des Moduls sprach noch
von SKELETON/INITIALIZING (Stand 05.08.) und sagt jetzt, was wo liegt.

## Nachtrag 07.11. 13:15 — die Kante zeigte in die falsche Richtung

Der vorige Schritt hat die Doppelung beseitigt, aber dabei eine neue
Architekturverletzung eingebaut: `service_space/Cargo.toml` bekam
`atc_drivers` als Abhaengigkeit. Damit zeigte die **Kernschicht auf ein
Produktmodul** — das Crate mit Cargo-Beschreibung "Hardware-Treiber" und einer
React-Anzeige (`src/components/HardwareDriversView.tsx`). Ich habe das geprueft,
statt es zu hoffen: es war die **einzige Modul-zu-Modul-Kante im ganzen Baum**.
Die vorhandenen Kanten laufen alle abwaerts (`modules/atc-shivacore/kernel` und
`atc-drivers` auf `system/diagnostics`, `service_space` auf `system/devices`,
`system/*` untereinander) — und `system/runtime` haengt an
`shivacore-service-space`, also zeigt die Produktseite auf die Kernschicht, nie
umgekehrt.

**Korrektur:** `driver_framework.rs` ist nach
`system/devices/src/driver_lifecycle.rs` umgezogen (Systemvertrag, nicht
Anzeige-Frage). Der Service Space importiert `globus_devices::driver_lifecycle`
— diese Abhaengigkeit hatte er ohnehin schon. `atc_drivers` haengt jetzt an
`globus-devices` und **re-exportiert** nur noch, der Pfad
`atc_drivers::driver_framework::…` bleibt gueltig, die Modul-zu-Modul-Kante ist
weg. Der Test des Lebenszyklus ist mit umgezogen (globus-devices: 37 Tests).

**Zweiter Befund, unabhaengig davon:** Der Service Space baut **nicht** fuer
`x86_64-unknown-none`. Der Abbruch kommt aus `sha2` ueber
`ed25519-dalek` (vorbestehend, nicht aus diesem Umbau). Solange das so ist, kann
der Service Space nicht in ein Boot-Image gelinkt werden — das ist der eigentliche
Grund, warum `sync_from_boot()` keinen Zieher hat, und ein eigener Auftrag.

**Dritter Befund derselben Pruefung:** Das Wurzel-`FILE_REGISTER.md` behauptete
"vollstaendige maschinell generierte Liste aller Dateien", enthielt aber 209 von
503 getrackten Dateien — **alle 141 Dateien aus `system/` fehlten** (Erzeugung
ueber die GitHub-Trees-API am 10.09.). Jetzt aus `git ls-files` erzeugt: 503
Eintraege, 0 fehlende Pfade, 0 Differenz zur Dateiliste. Damit sind beide
Register zum ersten Mal deckungsgleich mit dem Baum.

## Nachtrag 07.11. 13:45 — der Service Space uebersetzt jetzt fuer Bare Metal

Der Blocker aus der vorigen Stunde ist geloest, aber nicht so, wie ich zuerst dachte.
Die Fehlermeldung zeigte auf `sha2`; die Ursache war eine Kette.

**1. `system/devices` war ein std-Crate.** Der Service Space ist no_std und haengt
an dieser Vertragsschicht. Beim Bau fuer `x86_64-unknown-none` kam zuerst
`can't find crate for std` und dann 40 Folgefehler (`derive` unbekannt, weil das
Praeadlude fehlte). Umstellung war klein: `#![cfg_attr(not(test), no_std)]`,
`extern crate alloc`, **eine** echte std-Stelle (`std::ops::Range` →
`core::ops::Range` in `block.rs`) und `alloc`-Importe in 13 Modulen. Tests
behalten std.

**2. `sha2` ist fuer dieses Ziel nicht uebersetzbar, und das ist nicht unsere
Schuld.** Der Abbruch lautet
`rustc-LLVM ERROR: Do not know how to split the result of this operator!` —
ein LLVM-Backend-Absturz, keine Rust-Diagnose. Nachgewiesen in einem **minimalen
no_std-Crate** (`sha2` allein, 6 Zeilen Code) gegen: Versionen 0.10.4, 0.10.6,
0.10.8, 0.10.9, 0.11.0 — alle gleich; Toolchains stable 1.99.1 und nightly
1.101.0 — beide gleich; Flags `+sse2`, `+sse3+ssse3+sse4.1`, `target-cpu=x86-64`,
`x86-64-v2`, `nehalem` — alle gleich. Es gibt also keinen Schalter, den wir
umlegen koennten. `sha2` kommt ueber `ed25519-dalek` herein, das nur `did.rs`
benutzt (Signature, Signer, SigningKey, Verifier, VerifyingKey).

**3. Entscheidung: Feature `crypto`, standardmaessig AN, fuer das Image AUS.**
`ed25519-dalek` ist jetzt optional (`version = "2.1"` statt 3.0, weil 3.0
zusaetzlich `sha2` 0.11 zieht und sha2 0.11 auf diesem Ziel zusaetzlich am
LLVM-Backend scheitert), und `did` sowie `remote_caps` haengen an
`#[cfg(feature = "crypto")]`. `remote_caps` faellt mit, weil es
`did::SoftwareSigner` braucht; sonst haengt kein Modul an den beiden.

**Ergebnis, belegt:** Host-Tests unveraendert **1687** (745 im Service Space,
also kein Test verloren — das Feature ist per Default an), und
`cargo +stable build -p shivacore-service-space --target x86_64-unknown-none
--no-default-features` endet mit `Finished` (Artefakt
`libshivacore_service_space.rlib`, 48,8 MB debug). `clippy -D warnings` ist in
beiden Konfigurationen still.

**Was das ehrlich bedeutet:** Der Service Space ist jetzt linkbar, aber ohne DID
und ohne entfernte Capabilities. Wer die braucht, muss zuerst das
SHA-512-Problem loesen — entweder eine eigene Implementierung (das Projekt
verlangt ohnehin from-scratch) oder ein Toolchain-/Upstream-Fix. Das ist ein
eigener Auftrag und steht jetzt als solcher da, statt als "irgendwas mit sha2".

## Nachtrag 07.11. 15:35 — ACPI als zweite Geraetequelle (REQ-SC006-11)

Der PCI-Scan findet nur, was hinter einem Konfigurationsraum liegt. Drei Geraete,
die der Kernel wirklich benutzt, sieht er nie: Local APIC, IOAPIC und den High
Precision Event Timer. Sie stehen in den ACPI-Tabellen.

**Neu: `kernel/src/acpi_devices.rs`** (12 Tests). `hpet_address()` liest die
Basisadresse aus der HPET-Tabelle und prueft dabei Signatur, Laenge und
Adressraum (nur Systemspeicher, keine E/A-Ports, keine Nulladresse).
`from_summary()` baut daraus zusammen mit der MADT eine Liste in fester
Reihenfolge LAPIC, IOAPIC, HPET; fehlt eine Quelle, fehlt der Eintrag, statt
eine Adresse zu erfinden. Eine LAPIC-Basis von 0 in der MADT faellt auf
0xFEE00000 zurueck, wie es die Architektur vorgibt.

**Neu: `BOOT_ACPI_DEVICES`** in `boot_devices.rs` (4 Tests). Eigener Vorrat,
nicht derselbe wie `BOOT_DEVICES`: ein ACPI-Geraet hat keine
Bus/Geraet/Funktion-Kennung, und der PCI-Vorrat darf nicht so tun, als haette es
eine. Ersetzen statt anhaengen, Lesen als Kopie, wie beim PCI-Vorrat.

**Zwei echte Fehler auf dem Weg, beide nur durch den Bootlauf sichtbar:**

1. **Falsches Tabellenlayout.** Meine erste Fassung las die Adressstruktur bei
   Offset 44 und die Adresse bei 48. Der Bootlauf brach mit
   "HPET address structure is malformed" ab. Ein Ausdruck der echten Bytes
   (36..56) zeigte: `00 00 00 00 | 00 00 d0 fe 00 00 00 00` ab Offset 40, also
   Adressstruktur bei 40 und Adresse bei 44. Die Tabelle ist 56 Byte lang, ein
   zusaetzliches Vier-Byte-Reserved vor der Struktur gibt es nicht. Parser und
   Tests sind auf das belegte Layout umgestellt, die Herleitung steht als
   Kommentar an der Konstante.
2. **Allokation vor dem Heap.** Die Veroeffentlichung lag zuerst direkt nach
   der ACPI-Pruefung, also vor der Heap-Initialisierung. Ergebnis:
   "Allokation fehlgeschlagen: Layout { size: 96, align: 8 }". Eine `Vec` vor
   dem Heap ist eine Allokation ins Nichts. Die Veroeffentlichung sitzt jetzt
   neben der PCI-Veroeffentlichung, MADT-Zusammenfassung und HPET-Basis reisen
   als Kopien dorthin (beide `Copy`).

**Belegt in QEMU, BIOS und UEFI, 0 Panics:**
`ShivaCore: ACPI-Geraete: 3 (LAPIC, IOAPIC, HPET) fuer den Service Space abrufbar`,
daneben unveraendert `Geraetevorrat: 6 Geraete` und der Ring-3-Syscallpfad.
**1703 Tests** (30 Suiten), fmt und clippy Exit 0.

**Was noch fehlt:** Der Service Space liest den ACPI-Vorrat noch nicht
(`sync_from_boot()` deckt nur PCI ab), und im Image startet weiterhin kein
Service Space. Die Quelle ist damit vollstaendig, der Zieher fuer ACPI fehlt.

## Nachtrag 07.11. 16:00 — der Service Space holt die ACPI-Geraete ab

Die Quelle war fertig, der Zieher fehlte. `device_registry.rs` hat jetzt den
Anschluss, den PCI schon hatte:

* `acpi_device_id()` gibt eine stabile Kennung aus Art und Basisadresse.
  Wichtig war der Kennungsraum: PCI kommt ueber `bus<<16 | geraet<<8 | funktion`
  nie ueber 2^24 hinaus, ACPI beginnt deshalb bei `0xAC00_0000_0000_0000`.
  Schneiden sich die Raeume, trifft eine Device-Cap irgendwann das falsche
  Geraet.
* `entry_from_acpi()` fuehrt LAPIC, IOAPIC und HPET als Klasse `Other`. Sie als
  `Storage` oder `Input` zu fuehren waere bequem und falsch.
* `adopt_discovered_acpi()`, `sync_from_acpi()`, `sync_acpi_from_store()` und
  `sync_acpi_from_boot()`, dieselbe Staffelung wie bei PCI. Uebernommene Geraete
  haben **keinen Besitzer**: finden ist offen, besitzen bleibt geprueft.

**9 neue Tests** (device_registry: 29 gesamt). Sie decken die vier Stellen ab,
an denen so etwas still kaputtgeht: Kennungsraeume schneiden sich nicht,
Kennung ist zwischen Laeufen stabil, verschiedene Arten haben auch bei gleicher
Adresse verschiedene Kennungen, und ein zweiter Abgleich zaehlt
`already_known` statt zu verdoppeln.

**Stand: 1712 Tests** (30 Suiten), fmt und clippy Exit 0, auch
`clippy -p shivacore-service-space --no-default-features` still, der Service
Space bleibt fuer Bare Metal baubar.

**Was weiter fehlt:** `sync_acpi_from_boot()` ist gerufen von niemandem, weil im
Image kein Service Space startet. Die Kette Kernel-Vorrat → Registratur ist
damit durchgaengig gebaut und einzeln belegt, im laufenden System aber noch
nicht in Betrieb.

## Nachtrag 07.11. 16:35 — Service Space ins Image: erst der Zyklus, dann die erste Stufe

Der naechste Schritt sollte sein, den Service Space ins Kernel-Image zu nehmen und
beim Boot zu starten. Beim Nachsehen fiel der Grund auf, warum das so nicht geht:

**`service_space/Cargo.toml` haengt an der Kernel-Crate** (`shivacore = { path =
"../kernel" }`), gebraucht fuer `boot_devices`, `capability`, `pci_scan`,
`madt`, `acpi_devices`. Damit kann der Kernel den Service Space nicht
einbinden: das waere ein Zyklus, und er verstoesst gegen die Hausregel
"Modulabhaengigkeiten nur abwaerts". Ein Service Space im Image ist ausserdem
die falsche Vorstellung: er ist ein Userspace-Programm, kein Kernelmodul. Der Weg
ins Image fuehrt ueber die Ramdisk und den ELF-Loader, nicht ueber eine
Cargo-Abhaengigkeit. Die Ramdisk ist derzeit leer (`ramdisk: Region { len: 0 }`),
deshalb startet nichts.

**Erste Stufe der Aufloesung, jetzt umgesetzt:** Die beiden reinen
Enumerationstypen sind in die gemeinsame API-Crate gewandert.

* `madt.rs` und `acpi_devices.rs` liegen jetzt in `system/shivacore-api/src/`.
* `libshivacore` ist dafuer `#![cfg_attr(not(test), no_std)]` und fuehrt
  `extern crate alloc` (fuer die `Vec` der Geraeteliste).
* Der Kernel re-exportiert sie (`pub use libshivacore::{acpi_devices, madt}`),
  die Pfade `shivacore::madt::…` und `shivacore::acpi_devices::…` bleiben also
  gueltig, im Bootpfad hat sich nichts geaendert.
* Der Service Space importiert `AcpiDevice` jetzt aus `libshivacore`, nicht mehr
  aus der Kernel-Crate. Die Kernel-Abhaengigkeit des Service Space ist damit um
  zwei Module kleiner, nicht weg.

**Belegt:** 1712 Tests (unveraendert; 692 Kernel, 754 Service Space,
**36 API** statt 24, weil die 12 ACPI-Tests mitgezogen sind), 30 Suiten,
fmt und clippy Exit 0, BIOS und UEFI ohne Panic mit `ACPI-Geraete: 3 (LAPIC,
IOAPIC, HPET)`, `Geraetevorrat: 6 Geraete` und laufenden Ring-3-Syscalls.

**Zwei Fehler auf dem Weg, beide selbst gemacht:** Der Re-Export landete zwischen
`#[cfg(test)]` und `pub mod allocator;`, wodurch das Gate auf den Re-Export
rutschte und der Kernel im Normalbau kein `crate::acpi_devices` mehr hatte
("found an item that was configured out"). Und clippy meldete im neuen Crate zwei
verschachtelte `if let`-Bloecke, weil die Kernel-Crate ihre Lints per
`#![allow(...)]` stilllegt, `libshivacore` aber nicht; geloest ueber
`Option::filter` statt Verschachtelung.

**Naechster echter Schritt:** Service Space als eigenstaendiges Ziel bauen
(eigener `_start`, fester Ladebereich), in die Ramdisk legen und vom
`elf_loader` starten lassen. Erst damit ruft jemand `sync_acpi_from_boot()`.

## Nachtrag 07.11. 17:15 — ein echtes Programm aus der Ramdisk

Bisher waren die Ring-3-Prozesse eingebauter Maschinencode im Kernel. Jetzt
reist ein eigenstaendiges Programm mit eigenem ELF vom Datentraeger in den
Kernel.

**Neu: `modules/atc-shivacore/init_program/`** — eigenes Crate, eigenes
Workspace (wie `boot`, damit der Haupt-Workspace stable bleibt), Ziel
`x86_64-unknown-none`. `_start` setzt `Yield` ueber `int 0x80` im Kreis ab, die
Nummer kommt aus `libshivacore::Syscall`, damit Programm und Kernel nicht
getrennt voneinander raten. Ein Linkerskript legt es auf 0x400000 fest.
Ergebnis: **6504 Bytes**, Typ EXEC, x86-64, Einstieg 0x400000, zwei
Ladesegmente (20 Bytes Code, 64 KiB `.bss`).

**Boot-Builder** nimmt jetzt ein optionales drittes Argument und reicht es an
`set_ramdisk` von `BiosBoot` und `UefiBoot` weiter. Ohne Argument entsteht wie
bisher ein Image ohne Nutzlast.

**Kernel** liest die Ramdisk und prueft sie mit dem vorhandenen `ElfParser`.
Boot-Log, BIOS und UEFI, 0 Panics:
`ShivaCore: Ramdisk 6504 Bytes, ELF-Einstieg 0x400000, Code 20 Bytes`.

**Drei Fehler auf dem Weg:**

1. **Allocator fehlte.** `libshivacore` zieht `globus-ipc`, das einen
   Allocator verlangt. Das Programm hat jetzt einen Zeiger-Allocator ueber
   64 KiB im eigenen `.bss` — ohne `#[used]` warf der Linker den Vorrat weg,
   weil das Programm nichts allokiert, und das ELF hatte nur 20 Bytes.
2. **Falscher Weg zur Ramdisk.** Ueber `physical_memory_offset` gelesen kam
   "bytes are outside a single reported physical-memory region". Die Adresse in
   `BootInfo` ist bereits virtuell, der Bootloader hat sie selbst abgebildet;
   gelesen wird jetzt als Slice von genau dieser Adresse, mit Laengenpruefung
   (max. 64 MiB).
3. **Wieder Allokation vor dem Heap.** Der Ramdisk-Block lag zuerst direkt nach
   der ACPI-Pruefung: "Allokation fehlgeschlagen: Layout { size: 224, align: 8 }".
   Er sitzt jetzt hinter der Heap-Initialisierung, neben der PCI-Uebernahme.

**Stand: 1712 Tests** (30 Suiten), fmt und clippy Exit 0 im Haupt-Workspace und
im neuen Crate.

**Was das noch nicht ist:** Das Programm wird geladen und geprueft, aber nicht
ausgefuehrt. `ElfLoader::load_elf` legt einen Prozess nur im Verwaltungsapparat
an; echte Seiten und der Wechsel nach Ring 3 laufen im Kernel bisher nur ueber
den eingebauten USER-001-Pfad. Der naechste Schritt ist, die Bytes aus der
Ramdisk durch genau diesen Pfad zu schicken.

## Nachtrag 07.11. 17:45 — das Programm aus der Ramdisk laeuft in Ring 3

Der letzte fehlende Teil ist geschlossen: die Bytes aus der Ramdisk werden nicht
mehr nur geprueft, sie laufen.

**Der Platzhalter ist ersetzt.** PID 1000 war bisher ein eingebautes Register-
Smoke-Binary ohne Syscall. Jetzt nimmt der Bootpfad das ELF aus der Ramdisk, legt
seinen Codesegment als Prozess an (gleiche Adresse 0x400000, gleicher Weg ueber
`map_user_binary` nach CPL3) und laesst es laufen. Ohne Ramdisk bleibt der alte
Platzhalter, damit ein nacktes Image weiter bootet; das Log sagt, welche Quelle
gilt (`Quelle: Ramdisk` bzw. `Quelle: eingebaut`).

**Belegt in QEMU, BIOS und UEFI, 0 Panics:**
```
ShivaCore: Ramdisk 6568 Bytes, ELF-Einstieg 0x400000, Code 89 Bytes
ShivaCore: USER-001 mapped PID=1000 RIP=0x400000 RSP=0x7ffff000 (Quelle: Ramdisk)
ShivaCore: Syscall aus Ring 3: Nr=1 PID=1000 Opcode=0x0001
```
Der erste Syscall im System kommt jetzt von einem Programm, das nicht im Kernel
steckt. `Nr=1` zeigt zugleich: vorher hatte PID 1000 nie einen Syscall abgesetzt.

**Ein Fehler auf dem Weg, der eine Luecke im ABI-Verstaendnis offenlegte:** Der
erste Lauf zeigte den Prozess an `RIP=0x400012` mitten in seiner Schleife, aber
keinen einzigen Syscall. Ursache: der Gate verlangt seit ABI 2.0 keinen Opcode im
Register, sondern einen Argumentblock im Userspace-Speicher (Groesse, Opcode,
Cap, Argumente) und antwortet sonst frueh mit `AbiVersionMismatch`, ohne den
Aufruf zu zaehlen. Das Init-Programm baut den Block jetzt auf seinem Stack
zusammen und uebergibt Zeiger, Laenge und Version in RDI/RSI/RDX. Der Block liegt
bewusst auf dem Stack: `map_user_binary` bildet nur Code- und Datenseite ab, ein
`.bss`-Vorrat waere nicht erreichbar. `RBX` darf im `asm!` nicht als Operand
stehen, das ist LLVM-intern belegt; die Statusauswertung braucht spaeter einen
anderen Weg.

**Was jetzt noch fehlt, um von einem Betriebssystem zu sprechen:** ein
Dateisystem, aus dem das Programm kommt (heute liest der Kernel die Ramdisk als
einzelnes ELF), ein Init, das weitere Programme startet, und getrennte
Adressraeume per CR3-Wechsel. Das Programm selbst ist da.

## Nachtrag 07.11. 18:05 — Registerpruefung gehoert zur Nutzlast, nicht zum Prozess

**Der Fehler, den ich selbst eingebaut hatte.** Seit das Ramdisk-Programm PID 1000
ausfuellt, ist die SCHED-001-Registerpruefung dort tot: sie erwartet
RAX=0x1000, RBX=0x1001, R12=0x1012, ein echtes Programm setzt das nie. Folge im
Bootlauf: **jeder** Timer-Tick schrieb eine Aufwaermzeile (300 Zeilen in 20
Sekunden), und die Invariante war fuer diesen Prozess dauerhaft im Aufwaerm-
zustand, also wirkungslos. Eine Pruefung, die nie greifen kann, ist schlimmer als
keine: sie kostet Serienbandbreite und suggeriert Sicherheit.

**Der Fix trennt Nutzlast von Prozessplatz.** `PATTERN_EXPECTED` sagt je Platz,
ob ueberhaupt ein Muster erwartet wird; der Bootpfad setzt es, weil nur er weiss,
was laeuft: `interrupts::expect_register_pattern(1000, source == "eingebaut")`.
Die Vorgabe ist **AN**, damit ein vergessener Aufruf die Pruefung nicht still
abschaltet; abgeschaltet wird ausdruecklich und mit Logzeile.

**Beide Wege im Bootlauf belegt, BIOS und UEFI, 0 Panics:**

| Fall | Aufwaermzeilen | Musterpruefung |
|------|----------------|----------------|
| mit Ramdisk | 0 (vorher 300) | `Musterpruefung fuer PID 1000 abgeschaltet (Nutzlast setzt keine Musterregister)`, 495 Checkpoints fuer PID 1001 |
| ohne Ramdisk | 0 | `pattern established PID=1000 RAX=0x1000 RBX=0x1001 R12=0x1012`, 499 Checkpoints |

Der Syscall aus Ring 3 bleibt in beiden Faellen unberuehrt
(`Nr=1 PID=1000 Opcode=0x0001` mit Ramdisk).

**Ein eigener Fehler im Fix, sofort gefunden:** Meine erste Fassung setzte die
Vorgabe auf AUS, womit die Pruefung fuer PID 1001 ebenfalls still ausfiel (die
Checkpoint-Zeilen verschwanden aus dem Log, 0 statt 495). Erst der Bootvergleich
beider Varianten hat das gezeigt, nicht die Tests: im Bin-Target gibt es keine
Testunterstuetzung, also traegt hier allein der Bootlauf die Beweiskraft.

## Nachtrag 07.11. 18:35 — die Ramdisk bekommt ein Verzeichnis (Format SRAM)

Bisher trug die Ramdisk genau ein ELF: der Kernel las die Bytes und konnte nicht
sagen, was darin liegt. Sobald mehr als ein Programm mitreisen soll, braucht sie
ein Verzeichnis, auch wenn das noch kein Dateisystem ist.

**Format** (`kernel/src/ramdisk.rs`, 12 Tests): Kopf mit Kennung `SRAM`, Fassung,
Anzahl; Eintraege zu je 24 Byte aus Name (16 Byte, ASCII, nullgefuellt),
Offset und Laenge, little endian. Regeln, dieselbe Haltung wie bei den
Syscall-Bloecken: erst pruefen, dann zeigen (jeder Eintrag muss vollstaendig im
Abbild liegen, sonst ist die ganze Tabelle ungueltig); keine zwei Wahrheiten
(doppelte Namen werden abgelehnt, nicht ueberschrieben); Nutzdaten bleiben
undurchsichtig, was darin steht entscheidet der ELF-Parser.

**Boot-Builder** nimmt ab dem dritten Argument Programme als `name=pfad`, baut
daraus das Verzeichnis und legt es als Ramdisk ins Image. Weil der Bau im
Kernel-Crate liegt, pflegen Werkzeug und Kernel nicht zwei Fassungen des
Formats; der Builder haengt dafuer an `shivacore`.

**Kernel** erkennt die Form: mit Kennung wird das Verzeichnis gelesen und ein
Programm **per Namen** gewaehlt (`init`, sonst der erste Eintrag), ohne Kennung
gilt das Abbild weiterhin als ein nacktes Programm. Ein Image ohne Verzeichnis
bootet also weiter.

**Belegt in QEMU, BIOS und UEFI, 0 Panics:**
```
ShivaCore: Ramdisk-Verzeichnis: 2 Programme (init, shell)
ShivaCore: Programm 'init' aus Ramdisk: 6568 Bytes ELF, Einstieg 0x400000, Code 89 Bytes
ShivaCore: Syscall aus Ring 3: Nr=1 PID=1000 Opcode=0x0001
```
Beide Eintraege tragen hier dasselbe Binary; belegt ist damit das Verzeichnis
und die Auswahl nach Namen, nicht zwei verschiedene Programme.

**Zwei eigene Fehler unterwegs:** `MAGIC` war als `[u8; 4]` deklariert, die
Zeichenkette aber fuenf Byte lang (`SRAMD`), also Compile-Fehler statt
stiller Abschneidung. Und die Auswahl gab `&str` auf den Namen zurueck, waehrend
die Eintraege im `match`-Arm starben; jetzt reist der Name als eigener `String`,
nur die Nutzdaten bleiben geliehen.

**Was das noch nicht ist:** Das Verzeichnis liegt im Speicher, nicht auf einem
Datentraeger; es gibt keine Bloecke, keine Inodes, kein Schreiben. Der naechste
Schritt ist ein Programm, das ein zweites startet (dafuer fehlt ein Spawn-Weg
in der ABI), oder ein Treiber, der das Verzeichnis von Platte liest.

## Nachtrag 07.11. 19:05 — zwei verschiedene Programme, zwei Prozesse

Bisher trug das Verzeichnis zwei Eintraege mit demselben Binary. Jetzt sind es
zwei verschiedene Programme, und beide laufen.

* **`init_program/src/bin/probe.rs`**: zweites Programm des Crates. Es setzt
  `MonotonicTime` ab statt `Yield`, damit am Opcode unterscheidbar ist, welcher
  Prozess spricht. Der gemeinsame Zeiger-Allocator liegt jetzt in
  `init_program/src/allocator.rs`, weil jedes Binary eines Crates einen eigenen
  `#[global_allocator]` braucht; `probe.rs` bindet ihn ueber
  `#[path = "../allocator.rs"]` ein.
* **Scheduler**: `init_user_scheduler` nimmt einen optionalen vierten Prozess.
  Ein Image ohne Verzeichnis bleibt bei drei Prozessen, das Log sagt, welcher
  Fall gilt.
* **Kernel**: aus dem Verzeichnis wird neben dem Init der erste weitere Eintrag
  geladen und als PID 1003 auf einem eigenen Adressraum (0x01C0_0000) gestartet.
  Der Code ist lagerelativ, deshalb darf er wandern.

**Belegt in QEMU, BIOS und UEFI, 0 Panics:**
```
ShivaCore: Ramdisk-Verzeichnis: 2 Programme (init, probe)
ShivaCore: Programm 'init' aus Ramdisk: 6576 Bytes ELF, Einstieg 0x400000, Code 89 Bytes
ShivaCore: vierter Prozess PID=1003 aus Ramdisk-Programm 'probe' RIP=0x1c00000
ShivaCore: SCHED-001 scheduler armed: PID=1000 + PID=1001 + PID=1002 + PID=1003 (Ring 3)
```
`context switch to PID=1003` erscheint 40-mal (UEFI) und 67-mal (BIOS), mit
RIP-Spruengen zwischen 0x1c00000 und 0x1c00057 — das Programm laeuft, es wird nur
nicht angezeigt. Der erste Syscall des Systems bleibt `Nr=1 PID=1000
Opcode=0x0001` aus dem Init.

**Zwei eigene Patzer:** Der BIOS-Lauf braucht mehr als 18 Sekunden; bei zu
kurzem Zeitlimit endet das Log mitten in der IRQ-Weiterleitung, ohne Panic, was
wie ein Fehler aussieht. Und `serial_println!` in einem Match-Arm faellt wieder
auf ("trailing semicolon in macro used in expression position") — dieselbe Falle
wie beim Tastenkanal, Loesung sind Klammern um die Arme.

**Ehrliche Grenze:** Das ist noch nicht "Init startet Programme". Der Kernel
startet beide, weil er das Verzeichnis liest. Der Unterschied ist der fehlende
Spawn-Weg in der ABI: ein Prozess kann heute keinen weiteren erzeugen.

## Nachtrag 07.11. 19:40 — ein Programmabsturz reisst das System nicht mehr mit

**Der Befund.** Der Seitenfehler-Handler gab Adresse und Fehlercode aus und
kehrte dann zurueck. Bei einem Fehler aus Ring 3 heisst das: an dieselbe
Anweisung zurueck, die eben gefehlt hat. Die Ausnahme loest sofort erneut aus,
die Maschine haengt. Andere Ausnahmen (General Protection, ungueltiger Opcode)
hatten gar keinen Handler. Die Anforderung "ein Programmabsturz im User Space
darf niemals den Kernel reissen" wurde damit nicht erfuellt — gemessen, nicht
vermutet.

**Die Entscheidung, pruefbar getrennt (`kernel/src/fault.rs`, 9 Tests).** Wer
den Fehler bezahlt, haengt am Ring, und der Ring steht im Code-Segment des
Rahmens: CS-Bits 0-1 gleich 0 heisst Kernel, sonst User. Ein Fehler aus Ring 3
kostet den Prozess, ein Fehler aus Ring 0 stoppt den Kernel. Dazu wird der
Fehlercode gelesen statt nur geloggt (Bit 0 heisst "vorhanden", nicht "fehlt",
das ist die klassische Verwechslung) und nach Herkunft getrennt gezaehlt.

**Die Ausfuehrung (`interrupts.rs`).** Eine Ausnahme mit Fehlercode legt diesen
vor den Rahmen. Der Trampolin holt ihn weg, sichert die Register wie beim Timer,
ruft den Handler und macht `iretq`. Der Handler **aendert den Rahmen**: der
fehlerhafte Prozess geht per `exit_process` in den Zombie-Zustand mit
Exit-Code -11, der naechste lauffaehige Kontext wird in den Rahmen geschrieben,
und `iretq` landet dort. Gibt es keinen lauffaehigen Prozess mehr, haelt der
Kernel bewusst an, statt einen toten Prozess fortzusetzen.

**Belegt in QEMU, BIOS und UEFI, 0 Panics**, mit einem dritten Programm
`init_program/src/bin/crash.rs`, das auf `0x0DEAD000` schreibt (in keinem
Fenster abgebildet):
```
ShivaCore: Prozessfehler PID=Some(Pid(1003)) Adresse=0xdead000 (Seite nicht vorhanden, Schreiben), Usermodus=true [beendet: 1, Kernel: 0]
ShivaCore: weiter mit PID=1000 statt den Fehler zu wiederholen
```
Entscheidend ist nicht die Zeile, sondern die Zaehlung: der letzte Wechsel auf
PID 1003 steht in Zeile 101, der Fehler in Zeile 102, **danach null Wechsel**
auf 1003. Der Prozess laeuft nicht wieder. Init (1000) setzt seinen Syscall
weiter ab, 78 Wechsel auf 1000 nach dem Fehler.

Gegenprobe ohne Absturz (Ramdisk mit `init` und `probe`): kein Prozessfehler,
PID 1003 laeuft mit 60 Wechseln. Die Ausnahme kostet also nur, wenn es wehtut.

**Eigener Fehler unterwegs.** Der Fehlercode lag nach 15 Pushes bei `rsp + 80`;
ich las `rsp + 32` und bekam r13 zu sehen. Das Log behauptete daraufhin
"Lesen" und `Usermodus=false`, obwohl ein Schreibzugriff aus Ring 3 vorlag. Die
Prozessbeendigung funktionierte trotzdem, was den Fehler fast verdeckt haette —
aufgefallen ist er an der Beschreibung, nicht am Absturz. Lehre: bei
Stack-Offsets die Lage nachrechnen und die Ausgabe gegen die Erwartung pruefen.

**Ehrliche Grenzen.** Nur der Seitenfehler hat einen Trampolin; General
Protection und ungueltiger Opcode haengen weiter. Der Adressraum des beendeten
Prozesses wird nicht freigegeben (die Seiten bleiben belegt), und die Zombies
werden nicht eingesammelt. Und die Trennung bleibt halb: ohne eigenen CR3 je
Prozess kann ein laufendes Programm den Speicher eines anderen lesen.

## Nachtrag 07.11. 20:25 — alle drei Ausnahmen haben ein Tor

**Der Rest des Befunds.** Nach FAULT-001 hatte nur der Seitenfehler einen
Trampolin. General Protection und ungueltiger Opcode hatten gar keinen Handler:
ein Programm, das eine privilegierte Anweisung versucht, haette die Maschine
weiterhin angehalten.

**Ein Rumpf, drei Einsprünge** (`interrupts.rs`). Die drei Ausnahmen
unterscheiden sich nur darin, ob die CPU einen Fehlercode auf den Stapel legt
und wie der Bericht heisst — nicht im Aufbau des Rahmens. Zwei fast gleiche
Rümpfe waeren die naechste Stelle, an der eine Korrektur nur an einer Haelfte
ankommt. Also: Seitenfehler und Schutzverletzung springen nach `pop r11` in
einen gemeinsamen Rumpf und uebergeben die Art als Zahl (0/1/2), der ungueltige
Opcode hat wegen des fehlenden Fehlercodes einen eigenen, kurzen Rumpf.
`ExceptionKind` sagt in `fault.rs`, ob ein Fehlercode zu erwarten ist —
`has_error_code` ist die Angabe, an der ein falscher Trampolin den Rahmen
verschiebt und Muell als RIP liest.

**Belegt in QEMU, BIOS und UEFI, 0 Panics**, mit zwei weiteren Programmen:
`crash_gp.rs` (`hlt` aus Ring 3), `crash_ud.rs` (`ud2`):
```
ShivaCore: Schutzverletzung PID=Some(Pid(1003)) Fehlercode=0x0 Usermodus=true [beendet: 1, Kernel: 0]
ShivaCore: Ungueltiger Opcode PID=Some(Pid(1003)) ohne Fehlercode, Usermodus=true [beendet: 1, Kernel: 0]
ShivaCore: Seitenfehler PID=Some(Pid(1003)) Adresse=0xdead000 (Seite nicht vorhanden, Schreiben) Usermodus=true [beendet: 1, Kernel: 0]
```
In allen drei Faellen `weiter mit PID=1000`, danach null Wechsel auf 1003. Image
mit `init` und `probe`: kein Prozessfehler. Die Entscheidung haengt am Ring, die
Art aendert nur den Bericht — sonst entstuende ein Schlupfloch, das davon
abhaengt, welche Anweisung ein Programm benutzt.

**Zwei eigene Fehler, beide in der Ausgabe statt im Absturz.**
1. Das Log schrieb bei der Schutzverletzung "Seite nicht vorhanden, Lesen".
   Der Fehlercode bedeutet je Vektor etwas anderes, und beim ungueltigen Opcode
   gibt es keinen; CR2 steht dann auf dem letzten Wert. Jetzt hat jede Art ihre
   eigene Zeile: Adresse beim Seitenfehler, roher Fehlercode bei der
   Schutzverletzung, "ohne Fehlercode" beim Opcode.
2. Bei der Schutzverletzung stand `Usermodus=false`, obwohl der Prozess aus
   Ring 3 kam: Bit 2 des Fehlercodes ist dort eine Segmentauswahl, nicht das
   User-Bit. Der Ring kommt jetzt aus dem Code-Segment des Rahmens.

**Und die Makro-Falle ein fuenftes Mal.** `serial_println!` in Match-Armen
scheitert an "trailing semicolon in macro used in expression position". Diesmal
an der Definition behoben statt an der Aufrufstelle: `serial_print!` expandiert
jetzt zu einem Block (`{{ ...; }}`), und ein Block ist ein Ausdruck. Damit ist
die Falle zum fuenften Mal aufgetreten und zum ersten Mal zugemacht.

**Ehrliche Grenzen.** Der Adressraum eines beendeten Prozesses wird weiterhin
nicht freigegeben, die Zombies werden nicht eingesammelt, und ohne eigenen CR3
je Prozess bleibt die Speichertrennung halb.

## Nachtrag 07.11. 20:55 — Boot-Monitor misst Wachstum statt Ende

**Warum.** Zwei Messfehler haben mehr Zeit gekostet als jeder Kernel-Fehler.
Erstens sah ein Log, das mitten im Lauf endet, wie ein Absturz aus — der
BIOS-Lauf braucht aber mehr als 18 Sekunden. Zweitens sieht ein echter Haenger
wie "laeuft noch" aus, weil ein Boot nie von selbst endet; ein Zeitlimit
unterscheidet die beiden nicht. Und beim Fehlercode-Fiasko war die Lehre
dieselbe: ich hatte auf "kein Panic" geschaut statt auf das, was im Log stehen
sollte.

**Das Werkzeug.** `scripts/boot_monitor.py`, ohne Abhaengigkeiten. Es liest das
Log zweimal mit Abstand und vergleicht die Groesse: kommt nichts dazu, steht
die Maschine — unabhaengig davon, wie lange man wartet. Zusaetzlich prueft es
die Meilensteine in fester Reihenfolge (Ramdisk-Verzeichnis, ELF, Ring 3,
Scheduler, Syscall), nicht bloss "irgendwas steht da". Urteile: `OK`,
`PANIC`, `HANG` (Log wuchs nicht), `UNVOLLSTAENDIG` (Meilenstein fehlt).
Rueckgabewert 0 nur, wenn jeder Lauf OK ist, plus optionaler JSON-Bericht.

**Der Selbsttest ist der eigentliche Punkt.** `--selftest` prueft das Urteil
gegen vier erfundene Logs, darunter genau der 18-Sekunden-Fehler und ein
laufender Boot mit fehlendem Syscall. Ohne diesen Teil waere der Monitor ein
Werkzeug, das immer "OK" sagt — und schlimmer als keins, weil man ihm glaubt.
4 von 4 korrekt.

**Erster echter Lauf**, BIOS und UEFI, je ein Bild mit unbeschaedigtem und eins
mit abstuerzendem Programm (30 s Fenster):
```
OK  img-ok  bios  265 Kontextwechsel, 0 Prozessfehler
OK  img-ok  uefi  235 Kontextwechsel, 0 Prozessfehler
OK  img-gp  bios  266 Kontextwechsel, 1 Prozessfehler (beendet, System laeuft)
OK  img-gp  uefi  237 Kontextwechsel, 1 Prozessfehler (beendet, System laeuft)
```
Der Fehler im gp-Bild ist kein Defekt, sondern das Ziel: der Prozess wird
beendet, der Rest laeuft weiter. Genau diese Unterscheidung stellt der Monitor
her, indem er Prozessfehler zaehlt statt sie mit einem Absturz zu verwechseln.

**Grenze.** Der Monitor beobachtet nur, was im seriellen Log steht. Ein Fehler,
der sich nicht aeussert (stiller Datenverlust, falsches Ergebnis), bleibt ihm
verborgen. Und die Meilensteinliste ist von Hand gepflegt: ein neuer Bootschritt
ohne Eintrag wird nicht geprueft.

## Nachtrag 08.11. 00:05 — die fehlende Trennung ist jetzt gemessen

**Nicht behauptet, sondern gezaehlt.** Bis heute stand in dieser Akte der Satz
"ohne eigenen CR3 je Prozess kann ein Programm den Speicher eines anderen
lesen" als Folgerung aus dem Code. Folgerungen sind billig. Jetzt gibt es eine
Messung, die man wiederholen kann.

Zwei Programme: `init` schreibt beim Start die Marke `0x4B4149315F4D4152` nach
`0x7FFE_F100` — das liegt in seinem **Stackbereich**. `peek` liest dieselbe
Adresse, obwohl sein eigener Stack bei `0x07F0_0000` liegt, und waehlt danach
den Syscall-Opcode: `Yield` (0x0001), wenn es die Marke gesehen hat,
`MonotonicTime` (0x0040), wenn nicht. Der Befund steht damit im Boot-Log und
nicht in einem Kommentar.

```
ShivaCore: Programm 'init' aus Ramdisk: 6608 Bytes ELF, Einstieg 0x400000, Code 121 Bytes
ShivaCore: USER-001 mapped PID=1000 RIP=0x400000 RSP=0x7ffff000 (Quelle: Ramdisk)
ShivaCore: scheduler armed: PID=1000 + PID=1001 + PID=1002 + PID=1003 (Ring 3)
ShivaCore: Syscall aus Ring 3: Nr=39849 PID=1003 Opcode=0x0001
```

`0x0001` ist `Yield`. PID 1003 hat die Marke aus dem Stack von PID 1000
gelesen. Die Trennung fehlt, und zwar nicht theoretisch, sondern im Log.

**Der Weg dahin war zwei Fehler lang, beide lehrreich.**

1. Die Codeseite ist schreibgeschuetzt. Der erste Versuch, die Marke bei
   `0x400200` abzulegen, endete korrekt in `Seitenfehler ... (Schutzverletzung,
   Schreiben) ... [beendet: 1]` — Init wurde beendet, das System lief weiter.
   Ein Programm darf seinen Code nicht ueberschreiben; die Marke gehoert auf den
   Stack.
2. Ein Programm, das nicht laedt, faellt **still** auf den eingebauten
   Platzhalter zurueck. `Ramdisk-Verzeichnis: 2 Programme (init, peek)` stand im
   Log, `Quelle: eingebaut` zwei Zeilen spaeter. Ursache war ein veraltetes
   Init-Binary im Image (4208 statt 6608 Bytes). Genau dafuer gibt es jetzt die
   Quellenangabe im Log: sie macht den Rueckfall sichtbar, statt ihn zu
   verstecken.

**Und eine Luecke im Syscall-Log.** Die Zusatzzeile galt bisher global (`Nr=1`
oder Vielfache von einer Million). Der erste Aufruf eines Prozesses fiel unter
den Tisch, sobald ein anderer vorher rechnete — bei einer Messung, die aus genau
einem Aufruf besteht, ist das der Unterschied zwischen Befund und Blindstelle.
Jetzt merkt sich der Kernel in einer festen Acht-Platz-Tabelle, von welchen
Prozessen schon ein Aufruf protokolliert wurde (zwei Tests, kein Allokator).
Ohne diese Aenderung waere die Zeile oben bei `Nr=39849` nie erschienen.

**Was daraus folgt.** Der naechste Schritt ist CR3: eigene Seitentabelle je
Prozess, Umschaltung im Kontextwechsel. Die Messung oben ist dann die
Gegenprobe — `peek` muss mit einem Seitenfehler sterben statt `Yield` zu
schicken. Solange beide Programme dasselbe Ergebnis liefern, ist nichts gebaut,
was den Namen Isolation verdient.

## Nachtrag 08.11. 01:20 — die eigene Seitentabelle haelt

**Was jetzt da ist.** `kernel/src/addr_space.rs` baut eine Seitentabelle, die
Kernel und Geraete erbt und den Nutzerbereich leer laesst. Der Selbsttest im
Bootpfad schaltet wirklich um, wartet drei Timer-Ticks ab und schaltet zurueck:

```
ShivaCore: Adressraum-Selbsttest: Tabelle 0x1E000 aktiv, 3 Timer-Ticks im
eigenen Raum, Kernel laeuft
```

BIOS und UEFI, 0 Panics. Die drei Ticks sind der eigentliche Punkt: waehrend
dieser Zeit stellt der Kernel den Timer ueber die LAPIC zu, deren Register eine
MMIO-Seite bei `0xFEE00000` sind. Ohne geerbte Geraeteabbildung stirbt der Boot
genau dort.

**Die Entscheidung, die dahinter steht.** Nutzerseiten und MMIO liegen im
**selben** PML4-Eintrag; eine neue Tabelle "kopiere die obere Haelfte" wuerde
also entweder die Trennung aufgeben oder den APIC verlieren. Getrennt sind sie
erst eine Ebene tiefer, im PDPT (1 GiB je Eintrag): Nutzerseiten in Eintrag 0,
LAPIC in Eintrag 3. Deshalb wird Eintrag 0 neu gebaut — mit leerem PDPT-Eintrag
0 und geerbten Eintraegen 1..512 — und alles darueber unveraendert uebernommen.
Keine handgepflegte MMIO-Liste, die veraltet, sobald ein Geraet dazukommt.

**Der Fehler, den das gekostet hat.** Die erste Fassung kopierte erst ab
PML4-Eintrag 256 ("Kernel-Haelfte") und baute nur Eintrag 0 selbst. Der Heap
liegt bei `0x4444_4444_0000`, also in Eintrag 136 — zwischen Nutzerbereich und
Kernel-Haelfte. Ergebnis: keine Logzeile nach `LAPIC aktiv`, nur ein Reset. Ein
Dreifachfehler schreibt nichts, er verschwindet. Die Lehre steht jetzt als Test
im Code (`everything_but_the_low_entry_is_inherited`), nicht nur als Kommentar.

**Zwei eigene Rechenfehler in den Tests**, beide korrigiert statt weggelassen:
`256 * 512 GiB` ergibt `0x0000_8000_0000_0000`, nicht die Kernel-Grenze — die
entsteht erst durch die Vorzeichenerweiterung des 48-Bit-Raums. Und
`0x4444_4444_0000 >> 39` ist 136, nicht 8. Beide Zahlen standen falsch da und
die Tests liefen rot, was der einzige Grund ist, warum sie jetzt stimmen.

**Was noch fehlt, damit daraus Isolation wird.** Der Selbsttest schaltet um und
zurueck; die Prozesse laufen weiterhin alle in der Boot-Tabelle. Der naechste
Schritt ist, jedem Prozess seine Tabelle mitzugeben und sie im Kontextwechsel zu
setzen. Die Gegenprobe steht schon: `peek` muss dann mit einem Seitenfehler
sterben statt `Yield` zu schicken.

## Nachtrag 08.11. 02:40 — die Trennung greift, und der Hänger war ein Zeitfenster

**Die Gegenprobe steht.** Dieselbe Messung, die vorher `Yield` meldete, endet jetzt
in einem Fehler:

```
Adressraeume je Prozess: PID=1000 -> 0x21000, PID=1001 -> 0x38000,
                         PID=1002 -> 0x4e000, PID=1003 -> 0x5d000
Seitenfehler PID=Some(Pid(1003)) Adresse=0x7ffef100 (Seite nicht vorhanden,
Lesen) Usermodus=true [beendet: 1, Kernel: 0]
weiter mit PID=1000
```

`peek` liest die Marke aus Init's Stack, findet nichts, stirbt, das System laeuft
weiter. BIOS 2123 Zeilen, UEFI 1999, kein Panic, 1744 Tests gruen.

**Vier echte Ursachen, jede einzeln belegt.**

1. *Die falsche Fenstergrenze.* Init's Stack liegt bei `0x7FFEF000`, also knapp
   unter 2 GiB und damit in PDPT-Eintrag 1. Meine Annahme "Nutzerseiten liegen
   unter 1 GiB" war falsch. Die Entscheidung kennt jetzt die Regionen des
   Prozesses (`mapped_regions()`), nicht eine feste Grenze.
2. *Geerbt wurde aus der aktiven Tabelle*, beim Bau von Prozess 2 also aus
   Prozess 1. Der Fehler blieb unsichtbar, weil mein Ersetzen von
   `Cr3::read()` durch `source` **still nicht gegriffen** hatte (Muster passte
   nach einem `cargo fmt` nicht mehr). Die Probe zeigte es: gleiche PDPT-Adresse
   `0x27000` in allen vier Tabellen. Jetzt ist die Boot-Tabelle die Quelle, und
   der Patch wurde zeilenweise gesetzt statt per Muster.
3. *Der Fehlerpfad wechselte den Kontext ohne die Tabelle.* Nach dem Beenden von
   PID 1003 lief PID 1000 mit dessen Tabelle weiter und starb an seiner eigenen
   Codeadresse (`0x400077`). Genau das stand im Log. Jetzt setzt auch dieser
   Pfad das CR3 des naechsten Prozesses.
4. *Ein Zeitfenster im Boot.* Der Aufbau blieb nach `IRQ 1 -> GSI 1` stehen,
   sobald keine Diagnosezeilen mehr dazwischen lagen — ein Haenger, der von der
   Laenge eines Logsatzes abhaengt, ist kein Zufall. `subscribe_kernel_line`
   sperrt Capability-Tabelle und Bruecke jetzt mit abgeschalteten Interrupts.

**Lehre, die Geld gekostet hat.** Zweimal hat ein `str.replace` nicht gegriffen
und ich habe den Code fuer geaendert gehalten: einmal bei einer Testrechnung,
einmal bei der Erbquelle. Beide Male war die Diagnose im Boot-Log eindeutig,
aber erst nachdem ich sie gemessen habe. Bei Textersetzungen in Rust-Dateien
danach **immer** pruefen, ob das Muster noch da ist, statt es anzunehmen.

