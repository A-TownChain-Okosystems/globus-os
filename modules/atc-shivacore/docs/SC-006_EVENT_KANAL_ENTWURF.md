# Entwurf: Tastenereignisse zum Userspace, ohne neue Syscall-Nummer

Stand 06.11.2026. Dies ist ein **Entwurf**, keine Umsetzung im Bootpfad. Er
beantwortet die zwei offenen Punkte aus dem IRQ-Audit: wohin die Tastenereignisse
gehen, und wo ein ACK ansetzt, wenn ein Prozess eine Line abonniert.

## Ausgangslage

Die Tastatur haengt seit `9e0e629` an GSI 1 auf Vektor 0x30, laeuft durch die
rechtegepruefte Bruecke und wird im Handler gesperrt, gelesen, per EOI quittiert
und wieder freigegeben (`[Bruecke 2, Rueckfall 0, Haenger 0]` je Taste). Der
Scancode landet im seriellen Log und nirgends sonst.

Die ABI hat sechs Syscalls: `Yield`, `IpcSend`, `IpcReceive`, `CapabilityQuery`,
`HandleClose`, `MonotonicTime`. Es gibt **keinen** Syscall fuer IRQ-ACK und
keinen fuer Tastatureingaben.

## Vorschlag: der Kernel ist Absender, `IpcReceive` ist der Kanal

Fuer Tastenereignisse braucht es keine neue Nummer. `IpcReceive` existiert, ein
Channel mit Capability existiert, und eine Nachricht traegt bereits `sender`,
`data` und `timestamp`. Der Kernel legt beim Spawn einen Channel an, behaelt
`WRITE` und gibt dem Prozess nur `READ`.

Prototyp: `kernel/src/key_events.rs` (Lib, 8 Tests, **nicht verdrahtet**).

| Frage | Entscheidung im Entwurf | Begruendung |
|---|---|---|
| Wer besitzt den Kanal? | der Kernel (`Pid(0)`) | Ein Prozess mit `WRITE` koennte sich eigene Tastenereignisse einspeisen; die Eingabe waere nicht mehr die Eingabe. Test: `the_subscriber_cannot_write_into_its_own_queue` → `NoWriteCapability`. |
| Was steht in der Nachricht? | ein Byte, der rohe Scancode | Der Kernel kennt kein Tastaturlayout. |
| Was passiert bei vollem Puffer? | `ChannelFull` wird zurueckgegeben und gezaehlt | Im Interrupt-Pfad darf nichts warten. Test: `a_full_queue_refuses_instead_of_blocking_the_handler`. |
| Reihenfolge? | FIFO | Test: `scancodes_keep_their_order`. |
| Fremder Zugriff? | `NoReadCapability` | Test: `a_foreign_process_cannot_read_the_queue`. |

## Wo setzt das ACK an

Solange der Kernel die Line haelt, ist die Frage gegenstandslos: er quittiert im
Handler selbst, die Maske steht also nur waehrend seiner eigenen Arbeit.

Erst wenn ein Prozess eine Geraete-Line **selbst** abonniert, entsteht die Luecke:
SC-006 (REQ-SC006-03) verlangt "maskiert bis ACK", aber niemand sagt, wie ein
Userspace-ACK beim Kernel ankommt. Drei Wege, mit Empfehlung:

1. **Implizit beim Konsum (Empfohlen).** Der Kernel stellt zu und gibt die Line
   erst frei, wenn der Prozess die Nachricht abgeholt hat, also `IpcReceive`
   erfolgreich war. Kein neuer Syscall, keine stille Aenderung an REQ-03: die
   Maske haelt der Kernel, nicht der Prozess. Preis: solange der Prozess nicht
   liest, steht die Line — bei einem haengenden Prozess also Stillstand. Das ist
   die ehrliche Folge von "maskiert bis ACK" und waere zu dokumentieren.
2. **Eigener ACK-Syscall.** Sauber trennbar, aber eine ABI-Erweiterung: neue
   Nummer, neue Rechtepruefung, neue Spec-Zeile. Nicht ohne Owner-Entscheidung.
3. **Zeitlimit statt ACK.** Der Kernel gibt nach n Ticks selbst frei und zaehlt
   die verpassten Zustellungen. Robust gegen haengende Prozesse, weicht aber von
   REQ-03 ab.

## Was der Owner entscheiden muss

1. **Empfaenger:** genau ein Prozess, mehrere, oder Broadcast? Der Entwurf nimmt
   einen an.
2. **Puffergroesse** und Verhalten bei Ueberlauf: verwerfen und zaehlen (Entwurf)
   oder Line maskieren bis der Prozess liest.
3. **Wo wird das Layout uebersetzt?** Der Kernel hat heute `keymap.rs` (US);
   besser waere die Uebersetzung im Prozess, damit ein deutsches Layout keine
   Kernel-Aenderung braucht. Dann muss aber `keymap` aus dem Kernel heraus.
4. **ACK-Pfad** (siehe oben): 1, 2 oder 3.

## Grenzen dieses Entwurfs

- Getestet ist die Kanalseite gegen die echte `IpcSubsystem`- und
  `CapabilityTable`-Implementierung, nicht der Bootpfad.
- Der Ring-3-Payload ist heute ein Hello-World-Blob ohne Syscall-Schleife. Ein
  echter Konsument braucht ein Programm mit `IpcReceive`-Schleife; das ist ein
  eigener Schritt und nicht Teil dieses Entwurfs.
- Kein Layout, keine Tastenwiederholung, kein Puffer je Prozess.
