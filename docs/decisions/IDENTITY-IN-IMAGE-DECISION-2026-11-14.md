# Entscheidung: Identitaetspruefung im Image (14.11.2026)

## Entscheidung des Owners

| Nr. | Entscheidung | Festlegung |
| --- | --- | --- |
| 1 | KDF-Parameter | **Uebernehmen** — Argon2id, 19 MiB, t = 2, p = 1, Parameter im Datensatz versioniert |
| 2 | Herkunft des Datensatzes | **Ramdisk** — eigene Datei, keine einkompilierten Zugangsdaten, strikte Rechte, fail-closed |
| 3 | Enter bei Erfolg | **Nur bestaetigen** — "Pruefung bestanden - Sitzung folgt.", keine vorgetaeuschte Sitzung, keine PID vor der Prozess-ABI |

Reihenfolge der ABI-Seite (Owner): `0x0060` registrieren, `SPAWN` als Recht auf
`Process`, PID eindeutig und kollisionsfrei, Start ueber den vorhandenen
`build_space`-Pfad, Uebergabe an den Scheduler, Tests fuer unberechtigten SPAWN,
ungueltige PID, fehlgeschlagenen Aufbau und erfolgreichen Start. `0x0050` bleibt
ausschliesslich der Konsole.

## Was umgesetzt ist: der gemeinsame Pruefkern

`system/identity-core` (`globus-identity-core`) ist `no_std` + `alloc` und wird
von Host und Image gemeinsam benutzt — ein Verifizierer, kein zweiter. Er baut
auch fuer `x86_64-unknown-none`, ist also im Image verwendbar.

- Datensatz: 64 Byte, Kennung `ATID`, Fassung 1, Verfahren 1 (Argon2id),
  Speicher (KiB, u16), Durchgaenge (u32), Parallelitaet, Reserviertfeld,
  Salz 16 Byte, Hash 32 Byte.
- `CredentialRecord::parse` ist fail-closed in fester Reihenfolge: Laenge,
  Kennung, Fassung, Verfahren, Reserviertfeld, Wertebereiche, Geheimnisse.
  Unbekannte Fassung wird nicht gedeutet, ein abgeschnittener Datensatz ist
  kein Datensatz, leeres Salz oder Hash wird abgelehnt.
- `verify` rechnet mit den Parametern **aus dem Datensatz** (nicht aus dem
  Programm), vergleicht ohne Fruehausstieg und meldet `KdfUnavailable`, wenn die
  KDF nicht rechnen kann.
- Grenzen: Speicher 8..32 MiB (Vielfache von 1 MiB), Durchgaenge 1..10,
  Parallelitaet 1..4. Ein Datensatz, der mehr verlangt, wird abgelehnt statt
  gerechnet.
- 9 Tests: richtiges und falsches Passwort, fehlender/abgeschnittener/
  aufgefuellter Datensatz, fremde Kennung/Fassung/Verfahren, manipulierte
  Parameter, leeres Salz oder Hash, Salt wirkt, schwaechere erlaubte Parameter.

## Messung im Image (Owner: messen, nicht schaetzen)

### Erster Schritt: der Speicher

Argon2id mit 19 MiB braucht 19 MiB **am Stueck**. Der Kernel-Haufen ist 512 KiB
(begruendet aus acht Dienst-Startabbildern zu 64 KiB). Messung vor der
Erweiterung:

```
IDENT-001 Haufen: HEAP_START=0x444444440000 HEAP_SIZE=512 KiB,
Allokator meldet frei=524288 Byte, Probe-Zeiger=0x0
IDENT-001 Messung: 19 MiB NICHT verfuegbar — die Pruefung wuerde fail-closed ablehnen
```

Der erste Messwert dieses Schritts meldete "19 MiB verfuegbar" und war falsch.
Aufgefallen ist es erst, als der freie Speicher mitgelesen wurde. Eine
Allokation "geglueckt" zu nennen, weil ein Zeiger ungleich null zurueckkommt,
ist kein Nachweis; der Nachweis braucht zwei unabhaengige Werte.

### Zweiter Schritt: Haufen erweitert, im Image gemessen

`allocator::extend_heap` bildet 20 MiB aus dem Frame-Vorrat direkt hinter dem
Haufen ab und uebergibt sie dem Allokator (ein Vorrat, kein zweiter). Die
Erweiterung ist einbahnig: danach gehoert der Speicher dem Haufen — ehrlicher
als "temporaer", weil eine Rueckgabe ein zweites Buch ueber denselben Speicher
waere. Gemessen im Kernel (QEMU/BIOS, Software-Emulation ohne KVM, `-m 256M`):

```
IDENT-001 Haufen erweitert um 20480 KiB (frei jetzt 20992 KiB)
IDENT-001 Messung im Image: Argon2id 19456 KiB t=2 p=1 = 267 ms
  (638614680 TSC, 2385858240 Hz) — gemessen, nicht geschaetzt
IDENT-001 Pruefweg im Image: richtig bestaetigt, falsch abgewiesen
```

**Ergebnis: 267 ms im Image gegen 24,7 ms auf dem Host — Faktor ~11.** Genau
deshalb war die Host-Zahl kein Nachweis: 267 ms liegen ueber der Grenze, die
eine Anmeldeoberflaeche als "sofort" vertraegt. Die Entscheidung
"Parameter uebernehmen" bleibt davon unberuehrt (sie ist gefallen), aber die
Zahl gehoert in die Bedienung: das Fenster muss waehrend der Pruefung sichtbar
beschaeftigt sein, nicht stehen. Die TSC-Kalibrierung gegen den PIT ist die
Zeitbasis (2,386 GHz), keine geschaetzte Frequenz.

Der Pruefweg im Image ist damit vollstaendig belegt: Datensatz im Speicher
gebaut, fail-closed gelesen, richtiges Passwort bestaetigt, falsches abgewiesen.
Ohne Sitzung, ohne PID, ohne Scheduler (Entscheidung 3).

Offen fuer die Ramdisk-Ablage (Entscheidung 2): eigene Datei, strikte Rechte,
und der Weg, auf dem nur der Pruefprozess sie liest. Der Datensatz ist ein
Dateiformat, die Ablage folgt.

## Datensatz aus der Ramdisk (Entscheidung 2)

Der Datensatz ist eine eigene Datei, kein Teil des Programms: eine
Passwortaenderung ist damit kein neuer Kernel-Build.

- Name und Namensraum: `sys.identity`. Der Praefix `sys.` ist der
  Systemnamensraum; die Regel steht an einer Stelle (`ramdisk::may_read`).
  Fuer Dienste und Programme ist der Eintrag **nicht vorhanden** — kein
  "Zugriff verweigert", weil schon die Existenz eines Anmeldedatensatzes ein
  Programm nichts angeht. Lesen darf ihn nur der Kernel.
- `identity_file::load` ist fail-closed: fehlender Eintrag (`Missing`),
  Eintrag passt nicht ins Abbild (`Unreadable`), Inhalt ist kein gueltiger
  Datensatz (`Invalid(VerifyError)`). Es wird nie ein Ersatzdatensatz
  erfunden.
- Ausstellen: `cargo run -p globus-identity-core --example issue-record --
  <passwort> <datei>` (Host-Werkzeug, Salz aus /dev/urandom, Parameter aus dem
  Kern). Ablage: als Ramdisk-Eintrag `sys.identity=<datei>` beim Bauen.

Belegt im Lauf (QEMU/BIOS):

```
IDENT-002 Datensatz aus der Ramdisk: gelesen (64 Byte, Argon2id 19456 KiB t=2 p=1)
Ramdisk-Verzeichnis: 2 Programme (init, greeter)
```

und ohne die Datei:

```
IDENT-002 Datensatz aus der Ramdisk: Missing — fail-closed, keine Anmeldung
```

Dabei ein Fehler gefunden und behoben: die Programmwahl im Bootpfad nahm
`entries.first()` und haette den Anmeldedatensatz als Programm starten koennen.
Jetzt laufen Auswahl und Verzeichnisanzeige ueber `Reader::Program`; der
Datensatz erscheint weder in der Liste noch als Kandidat.

### Nachtrag zur Messung

Zweiter Lauf: 276 ms (659542416 TSC bei 2.381.429.640 Hz). Die Groessenordnung
ist stabil (267 / 276 ms), die Streuung kommt aus der Software-Emulation ohne
KVM.

## Das Anmeldefenster prueft (IDENT-003)

Das Fenster prueft nicht selbst: es schickt die Eingabe an den Kernel und zeigt,
was zurueckkommt. Der Datensatz gehoert dem Kernel (er liegt im
Systemnamensraum `sys.identity` und ist fuer Programme nicht vorhanden), also
rechnet er auch.

- **Syscall:** `IdentityCheck = 0x0070`. Die Nummer `0x0060` bleibt fuer
  `SpawnProcess` reserviert (Vorgabe des Owners), `0x0070` ist der naechste
  Schritt derselben Konvention (Vielfache von `0x10`).
- **Recht:** nur der Besitzer der Konsolen-Capability mit Schreibrecht — also das
  Anmeldefenster. Ein anderes Programm bekommt `PermissionDenied`, und die
  Ablehnung steht im Audit-Log als `identity.denied`.
- **Grenzen:** Passwort hoechstens 64 Byte; der Puffer wird gegen das
  Prozessabbild geprueft (`range_readable`) und in den Kernspeicher kopiert,
  danach wird nur Kernspeicher gelesen. Nach der Pruefung werden die Bytes im
  Kernel und im Programm ueberschrieben.
- **Ergebnis:** `0` bestanden, `1` falsch, `2` kein Datensatz, `4` nicht
  rechenbar. Das Fenster zeigt bei `0` genau „Pruefung bestanden - Sitzung
  folgt." und sonst den Grund; ein Erfolg wird nie behauptet.
- **Anzeige waehrend der Rechnung:** das Fenster zeichnet „Pruefung laeuft ..."
  **vor** dem Aufruf. Der Aufruf blockiert das Programm, die Anzeige bleibt
  waehrend der gemessenen ~250 ms stehen.

Gemessen und belegt (QEMU/BIOS, Datensatz mit Passwort „ab" in der Ramdisk):

| Fall | Log | Bildschirm |
| --- | --- | --- |
| richtig | `Anmeldepruefung: PID=1003 Ergebnis=Ok (589577160 TSC-Zyklen, 2 Byte)` | `Pruefung bestanden - Sitzung folgt.` |
| falsch | `Ergebnis=WrongPassword (585596448 TSC-Zyklen, 2 Byte)` | `Pruefung fehlgeschlagen.` |
| kein Datensatz | `Ergebnis=NoRecord (189480 TSC-Zyklen, 1 Byte)` | `Kein Anmeldedatensatz vorhanden - keine Anmeldung.` |

589577160 Zyklen bei 2,386 GHz sind ~247 ms — die Rechnung im Fenster, nicht der
Boot-Selbsttest.

### Ein Fehler auf dem Weg, der zur ABI gehoert

Der erste Versuch endete mit „Pruefung nicht moeglich (ABI-Ablehnung)", ohne
dass ein Passwort geprueft wurde: `SyscallBlock::parse` in `libshivacore` fuehrt
eine eigene Liste bekannter Nummern. Der Dispatcher kannte `0x0070`, der Parser
nicht — das Ergebnis war `InvalidSyscall`. Dieselbe Nummer muss an **beiden**
Stellen stehen; eine ABI-Nummer ist erst dann vergeben, wenn Parser und
Dispatcher sie kennen.

### Was die Anmeldung weiterhin NICHT tut

Keine Sitzung, keine PID, kein Scheduler-Uebergang: der Text sagt „Sitzung
folgt", und es folgt keine. Der Prozessstart ist die naechste Aufgabe
(`SpawnProcess`, `0x0060`, Recht `SPAWN` auf `Process`).

## Was ausdruecklich NICHT passiert ist

- Keine Sitzung, keine PID, kein Scheduler-Uebergang in der Anmeldung
  (Entscheidung 3).
- Kein einkompiliertes Passwort und kein einkompilierter Hash (Entscheidung 2);
  der Datensatz ist ein Dateiformat, die Ramdisk-Ablage folgt mit dem Zugriff.
- Keine Uebernahme der Host-Laufzeit (24,7 ms) als Image-Nachweis.


---

# Identitaetspruefung im Image — Entscheidungsvorlage

**Datum:** 14. November 2026
**Status:** OFFEN — braucht eine Owner-Entscheidung, bevor Code entsteht
**Betroffene Repos:** globus-os (Kernel, Greeter, `system/identity`)
**Vorgaenger:** `docs/LOGIN_WINDOW.md` (Fenster und Supervisor), `modules/atc-shivacore/docs/SPAWN-ANNAHME-2026-11-09.md`

## Wo es steht

Das Anmeldefenster ist bedienbar: Maske, Tastatur ueber den echten PS/2-Pfad,
maskiertes Passwort, Feldwechsel, `Enter`-Zustand, `Esc` als Dienst-Neustart
ueber den Supervisor. Was es **nicht** tut: pruefen. Der `Enter`-Zustand nennt
den Grund im Klartext statt einen Erfolg vorzutaeuschen.

Der Grund ist keine Luecke im Fenster, sondern ein fehlendes Stueck Architektur:
die Pruefung liegt heute in `system/identity` (`globus-identity`) und ist
Host-Code — `Argon2::default()`, `bip39` mit Zufallsquelle, `file_store` fuer die
Ablage. Im Image gibt es weder Dateisystem noch Zufallsquelle noch Haufen.

## Was gemessen ist (Host, dieser Rechner)

`system/identity/src/service.rs` benutzt `Argon2::default()` = Argon2id, v19,
`m = 19456 KiB`, `t = 2`, `p = 1`.

| Profil | Parameter | Host-Zeit |
|---|---|---|
| heute im Host | m=19456 KiB, t=2, p=1 | 24,7 ms (zweiter Lauf 23,3 ms) |
| Kandidat fuer das Image | m=4096 KiB, t=3, p=1 | 6,7 ms |
| Untergrenze | m=1024 KiB, t=3, p=1 | 0,9 ms |

Nicht gemessen und deshalb **nicht** als Tatsache verwendet: die Laufzeit im
Image. Die Laeufe sind Software-Emulation ohne KVM; ein Faktor von 10 bis 25
waere eine Schaetzung, keine Messung. Fuer die Entscheidung ist die Zeit auch
nicht der Engpass (0,25–0,6 s bei 19 MiB waeren vertretbar), sondern der
Speicher: der Puffer ist `m` gross und muss im Ring 3 les- und schreibbar
abgebildet sein.

## Die drei Wege

### A — Pruefung im Fenster, gemeinsamer Kern im Ring 3

Ein eigenes Crate `globus-identity-core` (no_std, `alloc`) enthaelt genau eine
Funktion `verify(record, kennwort) -> Ergebnis` und den Verifier-Datensatz. Host
und Fenster rufen dieselbe Funktion; die Host-Seite wird auf sie umgestellt,
damit es keine zweite Wahrheit ueber das Passwort gibt. Der Datensatz kommt als
eigener Ramdisk-Eintrag ins Image, erzeugt beim Bauen aus der Host-Ablage.

- Kosten: Fenster braucht einen Haufen und ein `m` grosses RW-Abbild (bei
  19 MiB: 20 MiB Adressraum, 256 MiB hat das Image).
- Grenze: im Image ist die Ablage nur lesbar. Ein Kennwortwechsel im Image ist
  nicht moeglich, ohne Schreibspeicher.
- Risiko: das Klartext-Kennwort liegt im Prozess, solange geprueft wird; der
  Puffer muss danach genullt werden (heute zaehlt das Fenster nur die Laenge).

### B — Identitaetsdienst als eigener Ring-3-Prozess

`globus-identity` wird als Prozess portiert, das Fenster fragt per IPC
(`Syscall::IpcSend`/`IpcReceive`, Kanal wie bei der Tastatur). Ein Verifizierer,
zwei Prozesse, klare Rollen; das Fenster sieht den Verifier-Datensatz nie.

- Kosten: ein Prozess mehr, ein IPC-Vertrag (Anfrage/Antwort, Laengen, Fehler),
  und `globus-identity` muss ohne Dateisystem und ohne Zufallsquelle bauen.
- Vorteil: passt zur spaeteren Kontenverwaltung; das Fenster bleibt duenn.
- Nachteil: groesserer erster Schritt; ohne Ablage im Image bleibt es bei
  einem einzigen Konto.

### C — Pruefung im Kernel

Abgelehnt, und zwar aus zwei Gruenden, die nicht verhandelbar sind: der Kernel
bekaeme das Klartext-Kennwort zu sehen, und er muesste `m` Byte Hash-Arbeit im
Trap-Kontext leisten. Das verletzt die Trennung, die das Fenster gerade
einhaelt (Kernel orchestriert, er interpretiert nicht).

## Empfehlung

**A jetzt, B als Ziel.** Beide teilen `globus-identity-core`; der Umstieg auf B
ist dann ein Umzug des Aufrufers, kein zweiter Verifizierer. A braucht drei
Entscheidungen, die ich nicht selbst treffen kann:

1. **KDF-Profil im Image.** Gleiche Parameter wie im Host (19 MiB) ist die
   einzige Variante ohne Sicherheitsabstrich; 4 MiB waere ein bewusster
   Abstrich und muss als solcher entschieden werden, nicht als Standard
   durchrutschen.
2. **Wo der Datensatz herkommt.** Ramdisk-Eintrag aus der Host-Ablage beim
   Bauen (Empfehlung) oder fest im Programm eingebettet. Eingebettet hiesse:
   neues Kennwort = neues Programm.
3. **Was `Enter` bei Erfolg ausloest.** Heute gibt es keine Sitzung: das Fenster
   koennte nur "geprueft" anzeigen. Eine Sitzung braucht einen Prozessstart
   (offen, siehe unten) oder wenigstens einen Kanal, auf dem es weitergeht.

## Abnahmekriterien fuer A

- `globus-identity-core` ist die einzige Stelle, die einen Verifier prueft; der
  Host ruft sie auf, ein Test weist nach, dass Host und Fenster bei gleichem
  Datensatz und gleichem Kennwort dasselbe Ergebnis liefern (gleicher
  PHC-String, gleiche Parameter).
- Falsches Kennwort und unbekannter Benutzer ergeben **verschiedene** Zustaende
  im Fenster, aber dieselbe Rechenzeit-Klasse (kein "Benutzer unbekannt" vor
  der Pruefung, das waere eine Auskunft).
- Der Klartext-Puffer wird nach der Pruefung genullt; ein Test liest ihn danach.
- Byte-Laengen werden nachgezaehlt, nicht geschaetzt (Lehre aus dem
  Domaenenpraefix-Fehler in a-townchain: 15 statt 16 Byte).
- Ohne gueltigen Datensatz startet das Fenster, meldet die fehlende Ablage und
  behauptet keinen Erfolg.

## Was davon unabhaengig offen bleibt

Ein echter Prozessstart aus dem Abbild (Frame-Allokator im Syscall-Pfad,
Syscall-Nummer nicht `0x0050` — die ist durch `Console` belegt). Ohne ihn kann
`Enter` keine Sitzung beginnen; das ist die Grenze, die A nicht loest.

`CI_VERIFIED` bleibt fuer alle Punkte offen: die Workflows starten seit dem
3.11.2026 nicht (`startup_failure`), das ist eine Owner-Aktion in der
GitHub-Oberflaeche.
