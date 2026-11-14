# Anmeldefenster im Ring 3 — Bau, Nachweis, Grenzen

Stand: 14.11.2026. Belegt ist ein bedienbares Anmeldefenster im laufenden
QEMU-Image: Maske, Tastatureingabe ueber den echten PS/2-Pfad, maskiertes
Passwortfeld, Feldwechsel, Loeschen, Enter-Zustand. Nicht belegt und ausdruecklich
offen: die Pruefung der Anmeldedaten (es gibt nichts, wogegen geprueft werden
koennte).

## Was gebaut wurde

Die Strecke vom Kernel bis zum Fenster bestand vorher aus vier Teilen, von denen
nur der letzte fehlte.

| Schicht | Aenderung |
|---|---|
| ABI (`system/shivacore-api`) | `Syscall::Console = 0x0050` mit `CONSOLE_OP_CLEAR`/`CONSOLE_OP_WRITE`, `CONSOLE_MAX_BYTES = 1024`; `input`-Modul als einzige Quelle des gepackten Tastenereignisses; `CONSOLE_MAX_BYTES <= MAX_SYSCALL_PAYLOAD` als Uebersetzungspruefung |
| Kernel: Recht | `ResourceType::Console` als eigene Capability; ohne Schreibrecht kein Zeichnen, Ablehnung im Audit-Log |
| Kernel: Bildschirm | `clear_screen`, `write_text`, Besitzerumschaltung: nach der Uebergabe schreibt der Kernel nur noch seriell, damit zwei Schreiber sich nicht ueberschreiben |
| Kernel: Nutzerkopie | `range_readable` prueft den Puffer gegen das Prozessabbild, bevor Bytes kopiert werden. Vorher konnte die ABI keine Nutzdaten tragen |
| Kernel: Trap | `handle_console` vor der Disposition, wie `Exit`: Rechte, Puffergrenze, Prozessabbild, Kopie, Ausgabe |
| Kernel: Start | vierter Prozess (PID 1003) aus dem zweiten Ramdisk-Eintrag, zwei Caps, Nummern als Startvertrag auf dem Stapel |
| Kernel: Tastatur | mit Fenster liest der Greeter; PID 1002 liest nicht mehr mit, sonst teilen sich zwei Leser die Tasten |
| Programm | `modules/atc-shivacore/greeter` (eigenes Crate, 0x1C00000): Maske, Feldwechsel, Maskierung, Enter-Zustand |

Die Konsole ist eine Ressource wie jede andere. Es gibt keinen Syscall, der ohne
Cap auskommt, und keinen Pfad, der am Recht vorbei zeichnet.

## Zwei Fehler, die das Fenster selbst gezeigt hat

Beide waren nicht im Fenster, sondern darunter, und beide sind jetzt an der
Ursache behoben.

**1. Der Startvertrag wurde zu spaet gelesen.** Die beiden Cap-Nummern lagen bei
`[rsp]` und `[rsp+8]`. Der Einstieg war eine Rust-Funktion, deren Prolog erst
Register schiebt und `rsp` verschiebt — danach standen an `[rsp]` die eigenen
gesicherten Werte. Ergebnis: zwei Nummern gelesen, beide falsch, beide Syscalls
abgelehnt. Der Einstieg ist jetzt ein Assembler-Stueck in `.text._start`, das
liest, bevor irgendetwas den Stapel anfasst.

**2. RBX ist ein Ausgaberegister der ABI, kein Callee-Register.** Die ABI
schreibt den Status nach RBX. RBX ist aber nach SysV callee-saved, also darf der
Compiler Werte darueber hinweg dort halten — er weiss nichts von der ABI. Die
Konsolen-Nummer stand nach dem ersten Aufruf nicht mehr in RBX; der zweite Aufruf
ging ohne Capability hinaus. Belegt im Log: `arg0=1 cap=None` direkt nach einem
erfolgreichen `arg0=0 cap=Some(5)`. Der Syscall laeuft jetzt durch eine
Assembler-Huelle (`atc_syscall`), die die Callee-Register sichert und den Status
nach RDX holt; fuer den Compiler sind beide Ausgaenge fluechtig.

Dazu ein dritter Befund, der beim Bauen auffiel: `.got` war im Linkerskript
verworfen. Der Compiler loest den `memset`-Aufruf ueber eine Tabellenzelle auf,
und die zeigte auf Adresse 8 — Seitenfehler beim Lesen. `.got` liegt jetzt im
geladenen Abschnitt, und das Programm hat bewusst keinen Haufen: die
Code-Seiten werden genau so gross abgebildet, wie das Segment Bytes hat, ein
beschreibbarer statischer Wert laege ausserhalb.

## Nachweis (QEMU, BIOS, Software-Emulation)

Alle Laeufe: `scripts/qemu_boot.sh` baut Kernel, Init und Fenster, das Image
enthaelt zwei Programme; Tastendruecke gehen als `sendkey` ueber die emulierte
PS/2-Tastatur, also ueber den echten Eingabepfad, nicht ueber einen Testhaken.

| Beobachtung | Ergebnis |
|---|---|
| Uebergabe der Konsole | `Konsole an Ring 3 uebergeben (PID=1003); Kernel schreibt nur noch seriell` |
| Fenster gezeichnet | 17.584 Nicht-Hintergrund-Pixel im Bereich (0,18)-(440,177), Hintergrund `(4,6,14)` |
| Tastatureingabe | nach `hello`: +560 Pixel im Benutzerfeld; `hello` + 6 Zeichen im Passwortfeld: +1.232 |
| Maskierung geprueft | zwei verschiedene Passwoerter gleicher Laenge (`aaaaaa`, `geheim`) ergeben ein **pixelgleiches** Bild |
| Feldwechsel/Loeschen | Tab wechselt die Markierung, sechsmal Backspace leert das Feld, neuer Text erscheint |
| Enter-Zustand | Statuszeile waechst von 272 auf 559 Pixel Breite (laengerer Text) |
| Prozesszustand | kein Seitenfehler, keine Panik, PID 1003 laeuft bis zum Timeout weiter |

Der Statuszeilen-Text nach Enter lautet:
`Keine Identitaetsablage im Image (kein Dateisystem) - Pruefung nicht moeglich.`

Das ist keine Fehlermeldung, sondern der Befund: `globus-identity` ist Host-Code
(argon2, bip39, Dateispeicher), und das Image hat kein Dateisystem. Ein
"Anmeldung erfolgreich" waere an dieser Stelle erfunden.

## Tests

| Ebene | Ergebnis |
|---|---|
| Workspace (Host, rustc 1.100.0) | 1778 bestanden, 0 fehlgeschlagen |
| ABI | 39 bestanden, davon 3 neu (Tastenlayout, Konsolen-Opcodes, Syscall-Nummer 0x0050) |
| Fenster-Crate | kein Testrahmen (bare metal); Nachweis ueber QEMU |
| rustfmt/clippy | eigene Aenderungen ohne Befund; die Datei `interrupts.rs` ist im Bestand nicht rustfmt-clean (Importreihenfolge, lange Signaturen) und wurde nicht umformatiert |

## Offen

1. **Identitaetspruefung.** Braucht eine Ablage im Image (Dateisystem oder
   eingebettete Kontenliste) und eine Entscheidung, wo das Geheimnis liegt.
2. **Anzeige im Betrieb.** Das Fenster zeichnet ueber den Kernel-Syscall. Ein
   Ring-3-Konsolendienst waere die Alternative; die Entscheidung ist offen, der
   Syscall ist so geschnitten, dass beide Wege darauf aufsetzen koennen.
3. **CI.** Die Workflows starten seit dem 3.11.2026 nicht (`startup_failure`),
   `CI_VERIFIED` ist damit weiterhin nicht vergeben.
4. **ABI-Dokumentation.** Der RBX-Befund gehoert in den Vertragstext: ein
   Programm darf ueber einen Syscall hinweg nichts in RBX halten.
