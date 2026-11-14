# Boot-Test bis zum Login-Fenster

**Auftrag:** das System bis zum Login-Fenster testen.
**Ergebnis in einem Satz:** Der Test erreicht das letzte vorhandene Stadium — laufender Ring-3-Betrieb mit Textkonsole und echter Tastatureingabe. Ein Login-Fenster gibt es nicht, und es gibt derzeit keinen Weg dorthin.
**Getesteter Stand:** `8c2fe5afc33e5a8b1267524e0c52c39dfbfdaca2` (main, 2026-11-14)
**Art des Belegs:** lokal aus dem Quelltext gebaut und in QEMU ausgeführt. Kein CI-Lauf, deshalb kein `CI_VERIFIED`.

## 1. Vorgehen

1. Kernel-ELF, Boot-Image und Init-Programm aus dem Quelltext bauen (`cargo +nightly`, Profil `kernel`).
2. In QEMU booten, BIOS und UEFI, serielle Ausgabe mitschreiben.
3. Bildschirminhalt über den QEMU-Monitor abgreifen (`screendump`), weil "Fenster" eine Aussage über den Bildschirm ist und nicht über das Log.
4. Echte Tasten über die PS/2-Schnittstelle senden (`sendkey`), um zu prüfen, ob Eingabe im Ring 3 ankommt.
5. Länger laufen lassen als der Boot dauert, um zu sehen, ob danach etwas passiert oder etwas abstürzt.

## 2. Messungen

| # | Modus | Laufzeit | Kontextwechsel | Logzeilen | Meilensteine | Besonderheit |
|---|---|---|---|---|---|---|
| 1 | BIOS | 45 s | 411 | 2.611 | 8 von 8 | Standardlauf |
| 2 | BIOS | 78 s | 873 | 5.406 | 8 von 8 | Bildschirmfoto bei 18 s und 78 s |
| 3 | UEFI (OVMF) | 45 s | 389 | 2.486 | 8 von 8 | gleicher Endpunkt |
| 4 | BIOS | 130 s | 1.264 | 7.773 | 8 von 8 | 0 Zeilen mit `panic`, `fault` oder `error` |
| 5 | BIOS | ~40 s | — | — | 8 von 8 | sechs echte Tasten gesendet |

Alle Läufe enden auf dem Timeout des Aufrufers, nicht auf einem Fehler des Systems. Erreicht werden die acht Meilensteine bis `USER-001 entering`.

## 3. Was der Lauf belegt

**Bootkette, IMPLEMENTED und lokal ausgeführt:**

- Firmware → Bootloader → Kernel → Ring 3. Der ELF aus der Ramdisk (`shivacore-init`, 6.736 Bytes) wird als PID 1000 gestartet, führt seine Syscalls aus und beendet sich mit Code 0.
- Nach dem Exit: Capabilities widerrufen, TLB geleert, IPC-Handles und Kanäle aufgeräumt, Zombie eingesammelt, 24 Frames zurückgegeben, Audit-Kette intakt (19 Ereignisse).
- Weiter laufen PID 1001 (Registerprüfung des Schedulers) und PID 1002 (Syscall-Schleife). Beide sind **im Kernel eingebauter Maschinencode**, keine ELF-Programme.

**Bildschirm:**

- Der Bootloader übergibt einen Framebuffer mit 1280 × 720, `Bgr`, 3 Byte je Pixel; der Kernel bildet ihn ab (`framebuffer::init`).
- Die Kernel-Konsole zeichnet echte Glyphen (`noto_sans_mono_bitmap`, Größe 16) und spiegelt jede `serial_println!`-Zeile auf den Bildschirm.
- Das Bildschirmfoto nach 78 s zeigt ausschließlich Kernel-Logzeilen der Form `SCHED-001 register checkpoint PID=1001 ...` und `scheduler preemption -> context switch ...`. Kein Rahmen, kein Feld, keine Eingabezeile, kein Anmeldebildschirm.

**Tastatur, Ende zu Ende belegt:**

Der Tastatur-IRQ legt in die Geräte-Warteschlange, der Tick pumpt sie in den IPC-Kanal, ein Ring-3-Prozess holt sie per `IpcReceive`. Mit sechs echten Tasten über die emulierte PS/2-Tastatur:

| Nr | Zeichen | Scancode | Empfänger |
|---|---|---|---|
| 2 | `h` | 0x23 | PID 1002 |
| 3 | `e` | 0x12 | PID 1002 |
| 4 | `l` | 0x26 | PID 1002 |
| 5 | `l` | 0x26 | PID 1002 |
| 6 | `o` | 0x18 | PID 1002 |
| 7 | — | 0x1C (Enter) | PID 1002, ohne Zeichen |

Ereignis 1 (`a`, Scancode 0x1E) stammt aus dem Selbsttest des Kernels, nicht aus der Hardware; die Ereignisse 2 bis 7 sind der echte Pfad. Damit ist die Eingabestrecke für ein Anmeldefeld vorhanden.

## 4. Was nicht da ist: das Login-Fenster

| Baustein | Stand | Beleg |
|---|---|---|
| Anzeige-Syscall für den Ring 3 | **fehlt** | `Syscall` kennt sieben Nummern: `Yield`, `Exit`, `IpcSend`, `IpcReceive`, `CapabilityQuery`, `HandleClose`, `MonotonicTime`. Keine Anzeige, keine Fläche, kein Fenster. |
| Zeichnen aus dem Ring 3 | **fehlt** | Die Konsole ist Kernel-Code (`kernel/src/framebuffer.rs`), erreichbar nur über `println!` im Kernel. |
| Anmeldebildschirm / Greeter | **fehlt** | Die Ramdisk enthält genau ein Programm (Init, `Yield` im Kreis, dann `Exit`). `init_program/src/bin/` enthält ausschließlich Testprogramme (`crash`, `crash_gp`, `crash_ud`, `peek`, `probe`). |
| Anmeldung selbst | **IMPLEMENTED, aber nicht im Image** | `system/identity/src/service.rs::login()` prüft das Geheimnis und öffnet eine Sitzung. Das ist Host-Code; kein Prozess im Image ruft ihn auf, und es gibt keine Quelle für die Eingabe. |
| Fenster, Kompositor, Shell | **Modell, nicht verdrahtet** | `system/graphics/` hat 2.182 Zeilen für Kompositor, Fensterverwaltung, Desktop-Sitzung, Shell und Eingaberouting. Es gibt keinen Renderer, keinen Syscall und keine Verbindung zum Bootpfad. `STATUS.md` führt das als Modell, nicht als laufendes System. |
| Dauerhafter Nutzerprozess nach dem Boot | **fehlt** | Nach dem Exit von PID 1000 läuft kein Ramdisk-Programm mehr. Es gibt keinen Supervisor, der Dienste startet oder neu startet. |

Kurz: Der Test scheitert nicht am Login-Fenster, er kann es nicht erreichen, weil hinter "Ring 3 läuft und kann tippen" nichts weiter implementiert ist.

## 5. Was ein Login-Fenster braucht

Die Reihenfolge ist nicht beliebig; jeder Schritt braucht eine Entscheidung, bevor Code entsteht.

1. **Anzeige-Autorität (Entscheidung nötig).** Entweder bekommt der Ring 3 eine Zeichenfläche per Capability (dann braucht die ABI einen Syscall und eine geteilte Fläche), oder ein Ring-3-Dienst übernimmt die Konsole. Beides zugleich wäre eine zweite Wahrheit über dieselbe Anzeige und ist nach der Architektur-Grundregel ausgeschlossen.
2. **Anmelde-Prozess im Image.** Ein Greeter, der `globus-identity` wirklich benutzt, statt es nur zu verlinken. Er braucht eine persistente Identitätsablage (heute Host-seitig) und eine Capability für die Anzeige.
3. **Zeilenbildung und Maskierung.** `IpcReceive` liefert einzelne Zeichen; für ein Passwortfeld fehlen Zeilenpuffer, Echounterdrückung und eine Obergrenze.
4. **Fokus und Fläche.** `wm.rs` und `compositor.rs` sind Modelle; ohne Renderer und Syscall bleibt ein Fenster eine Datenstruktur.
5. **Supervisor.** Nach dem Exit des Init läuft niemand mehr, der einen Greeter starten oder bei Absturz neu starten könnte.

## 6. Grenzen des Belegs

- QEMU ohne KVM: Software-Emulation, keine echte Hardware, kein echter Grafikchip, keine echte Tastatur.
- Kein CI-Lauf auf diesem Stand; Workflow-Läufe enden seit dem 03.11.2026 mit `startup_failure` (`docs/CI_STATUS.md`). Deshalb `CI_VERIFIED` nicht vergeben.
- Das Bildschirmfoto belegt den sichtbaren Inhalt dieses Laufs. Zum Zeitpunkt der Messung existierte kein Code fuer ein Fenster; dieses Blatt beschreibt den Stand VOR dem Anmeldefenster (siehe Nachtrag).

## 7. Nächster kleinster Schritt

Zuerst die Entscheidung aus Punkt 1: **wem gehört die Anzeige.** Ohne sie würde jeder Login-Versuch eine zweite Zeichenroute neben der Kernel-Konsole aufmachen — genau die parallele Implementierung, die die Architektur-Grundregel ausschließt. Danach ist der Greeter ein Ring-3-Programm mit Identity-Login, Tastatureingabe über den bestehenden Kanal und einer Anzeige-Capability; alles darunter ist bereits belegt.

## 8. Nachtrag 14.11.2026 — das Fenster existiert

Der Punkt 7 dieses Blattes ist erledigt, und die Entscheidung aus Punkt 1 ist
vorlaeufig gefallen: die Anzeige bleibt beim Kernel, der Ring 3 zeichnet ueber
den Syscall `Console` gegen ein eigenes Recht. Damit gibt es keine zweite
Zeichenroute, sondern genau eine — der Kernel besitzt die Flaeche und gibt sie
nur gegen die Capability frei. Das ist als Vorschlag umgesetzt, nicht als
ratifizierte Entscheidung; die Alternative (Ring-3-Konsolendienst) bleibt offen,
weil der Syscall so geschnitten ist, dass sie darauf aufsetzen kann.

Belegt ist ein bedienbares Anmeldefenster mit Tastatureingabe, Maskierung,
Feldwechsel und Enter-Zustand. Nachweis, Mechanik und die zwei dabei gefundenen
Fehler stehen in [LOGIN_WINDOW.md](LOGIN_WINDOW.md). Ausdruecklich offen bleibt
die Pruefung der Anmeldedaten: es gibt im Image keine Identitaetsablage.
