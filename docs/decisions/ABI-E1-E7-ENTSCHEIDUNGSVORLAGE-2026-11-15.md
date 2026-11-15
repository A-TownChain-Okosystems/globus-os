# Entscheidungsvorlage E-1 bis E-7 zum Syscall-ABI-Contract

**Status:** VORLAGE, nichts entschieden. Grundlage: `docs/architecture/SHIVACORE-SYSCALL-ABI-001.md` auf `ea84fae0`.
**Untersuchter Quellstand der Inventur:** `ea84fae0` (560 Dateien), frischer Klon.
**Regel dieses Dokuments:** Jede Entscheidung ist eine Auswahl zwischen benannten Alternativen mit ihren Folgen. Pauschale Zustimmung zu diesem Dokument entscheidet nichts. Eine Entscheidung ist keine Implementierungsfreigabe.

---

## A. Vollständige Inventur der Rechtevokabulare (Grundlage für E-1)

Es sind **vier** Definitionen im Baum, nicht drei. Die vierte liegt in `system/security` und enthält ein pauschales Administrationsrecht.

| Nr | Ort | Typ | Breite | Werte | Verwendung | Konvertierung |
| --- | --- | --- | --- | --- | --- | --- |
| V1 | `modules/atc-shivacore/kernel/src/capability.rs:69` | `struct Rights(pub u16)` | 16 Bit | `NONE` 0, `READ` 1, `WRITE` 2, `EXEC` 4, `DELEGATE` 8, `INSPECT` 16, `SUBSCRIBE` 32, `ACK` 64, `SET_DEADLINE` 128, `ACCESS` 256, `ALL` 0x1FF | Kernel-Tabelle, `create_typed`, `required_rights`, `dma_auth`, `irq` | maßgeblich im Kernel |
| V2 | `system/shivacore-api/src/lib.rs:355` | `struct CapabilityRights(pub u8)` | 8 Bit | `NONE` 0, `READ` 1, `WRITE` 2, `EXECUTE` 4, `DELEGATE` 8, `INSPECT` 16 | ABI-Crate, öffentliche Sicht für Dienste | **verlustbehaftet**: `SUBSCRIBE`/`ACK`/`SET_DEADLINE`/`ACCESS` nicht darstellbar |
| V3 | `system/shivacore-api/src/lib.rs:98` | `enum CapabilityRight` | keine Bits | `Read`, `Write`, `Execute`, `Grant`, `Revoke`, `Inspect` | Namensvokabular ohne Bitabbildung | keine; `Revoke` hat kein Gegenstück |
| V4 | `system/security/src/lib.rs:14` | `enum Right` | Einzelwert, kein Satz | `Read`, `Write`, `Execute`, `Map`, `Sign`, `Admin` | `capability_registry::authorize`: `object.right == requested \|\| object.right == Right::Admin` | keine; `Admin` ist eine Vollmacht |

**Transport- und Abfragepfade, die alle vier berühren:**

| Stelle | Verhalten heute | Folge |
| --- | --- | --- |
| `syscall.rs` `CapabilityQuery` | `ok(c.rights.bits() as u64)` aus V1 | Wert 256 kommt korrekt als `u64` an, ist aber in V2 nicht ablegbar |
| `service_space/remote_caps.rs:122` | `rights.bits().to_le_bytes()` aus V1, 2 Byte LE im signierten Ticket | Bindung an 16 Bit. Eine Verbreiterung auf 32 Bit macht alte Tickets ungültig |
| `remote_caps.rs:346` | `child.rights.0 & !parent.rights.0 != 0` ergibt `ChainRightsExpanded` | Vererbungsprüfung arbeitet auf V1-Bits |
| `system/security/capability_registry.rs:66` | Gleichheit oder `Admin` | **pauschale Vollmacht**, widerspricht „keine impliziten Root-Rechte“ |
| `sdk/src/lib.rs`, `system/runtime/src/core_api.rs` | keine Rechteverwendung | kein Konflikt, aber auch keine Absicherung |

**Messbare Verluste, nicht Vermutungen:** `ACCESS` ist der einzige Wert über 255 und damit der einzige, der an der u8-Grenze scheitert. `SUBSCRIBE`, `ACK` und `SET_DEADLINE` liegen unter 256 und überleben die Breite, haben aber in V2 keine benannte Konstante. V3 kann `ACCESS` nicht benennen. V4 kann neun der zehn V1-Rechte nicht ausdrücken und ersetzt sie durch `Admin`.

---

## B. E-1: Kanonisches Rechtevokabular

**Frage:** Welche Definition ist ab der Ratifizierung die einzige normative Quelle, und wie werden die anderen darauf abgebildet?

| Alt | Vorgehen | Auswirkung | Kosten / Risiko |
| --- | --- | --- | --- |
| **A1** | V1 (u16) wird nach `libshivacore` verschoben und ist die Wahrheit; `CapabilityRights` wird auf `u16` verbreitert; `CapabilityRight` sind benannte Konstanten darauf; `Revoke` fliegt aus V3 oder bekommt ein Bit; V4 wird ein Adapter, der auf V1 abbildet und `Admin` verliert | verlustfreie Abbildung in beide Richtungen, `ACCESS` bleibt 256, Ticket-Transport bleibt bei 2 Byte gültig | klein: Umschreiben von V2/V3, `capability.rs` re-exportiert; V4 braucht eine Zuordnungstabelle und einen Ersatz für `Admin` |
| A2 | wie A1, aber 32 Bit breit | Platz für künftige Rechte, sauberes Alignment im Block | Transport `remote_caps` bricht: alle Remote-Tickets müssen neu ausgestellt werden |
| A3 | V2 (u8) bleibt und `ACCESS` wird auf ein freies Bit unter 256 gelegt | keine Breitenänderung | **abgelehnt**: Bedeutungsumkehr bestehender Bits, alte Tickets werden falsch gedeutet, stiller ABI-Bruch |
| A4 | Kernel liefert `rights_low` und `rights_high` getrennt | u8 bleibt unverändert | zwei Felder, zwei Wahrheiten, jeder Aufrufer muss zusammensetzen |
| A5 | typisiertes `Rights`-Struct mit Bitset und Methoden statt Rohzahl, `u16` als Speicherform | Compiler verhindert Verwechslung von Vokabularen | größerer Umbau, aber keine Semantikänderung |

**Empfehlung: A1 jetzt, A5 als Zielform nach der Ratifizierung.** Begründung: A1 ist die einzige Variante, die verlustfrei ist, die bestehende Ticketkodierung nicht bricht und keine Neunummerierung vornimmt. A2 wäre architektonisch sauberer, kostet aber die Neuausstellung aller signierten Remote-Capability-Tickets und braucht dafür ein eigenes Verfahren.

**Verbindliche Regeln, unabhängig von der Alternative:**

1. Unbekannte Bits werden **zurückgewiesen**, nicht abgeschnitten: `try_from_bits(u64) -> Result<Rights, RightsError::UnknownBits>`. Ein `truncate` ist unzulässig.
2. Keine implizite Erweiterung: Rechte werden nur durch Vergabe größer, nie durch Konversion.
3. `Revoke` ist eine **Verwaltungsoperation**, kein Zugriffsrecht. Wenn es ein Bit bekommt, darf es nicht über `DELEGATE` weitergegeben werden. Wenn es kein Bit bekommt, gehört es nicht ins Rechtevokabular, sondern in die Capability-Tabelle als Operation.
4. `Admin`, `ALL` und ähnliche Sammelrechte sind außerhalb der Kernel-Tabelle unzulässig. Ein Dienst, der „alles darf“, ist die Umgehung des Modells.
5. Die Zuordnungstabelle V4→V1 ist endlich und vollständig zu dokumentieren: `Read`→`READ`, `Write`→`WRITE`, `Execute`→`EXEC`, `Map`→`WRITE|INSPECT`, `Sign`→ keine Entsprechung (offene Lücke), `Admin`→ **entfällt**.

**Negative Tests je Konvertierungsgrenze (vor der Ratifizierung nachzuweisen):**

| Nr | Eingabe | Erwartung |
| --- | --- | --- |
| K-1 | `0` | gültig, leerer Satz |
| K-2 | `1` | `READ` |
| K-3 | `255` (`u8::MAX`) | gültig, alle fünf alten Bits |
| K-4 | `256` | `ACCESS`, **nicht** 0 |
| K-5 | `511` | alle neun Bits, gleich `ALL` |
| K-6 | `512` | `Err(UnknownBits)` |
| K-7 | `u16::MAX` | `Err(UnknownBits)` |
| K-8 | `u64::MAX` | `Err(UnknownBits)`, kein Überlauf |
| K-9 | V4 `Admin` in V1 | kein Rechtebit, `Err(NoMapping)` |
| K-10 | `Revoke` über `DELEGATE` weitergegeben | `ChainRightsExpanded` oder `PermissionDenied` |

---

## C. E-2 bis E-7

### E-2: Reihenfolge der SpawnProcess-Voraussetzungen

| Alt | Vorgehen | Auswirkung |
| --- | --- | --- |
| **A** | (a) alle `PT_LOAD`-Segmente abbilden, dann (b) Frame-Allokator prozessübergreifend, dann (c) `AddressSpaceCreate`, dann (d) Process-Cap `SPAWN`, dann Syscall | jeder Schritt für sich prüfbar; `SpawnProcess` bleibt bis (d) gesperrt |
| B | (b) zuerst, dann (a) | ohne (a) startet kein reales Programm, der Nutzen von (b) ist nicht sichtbar |
| C | beides parallel, Freigabe erst wenn beide grün | kürzere Kalenderzeit, aber zwei offene Baustellen gleichzeitig |

**Empfehlung A.** Belegt: heute scheitert jedes Nutzerprogramm mit Stringliteral an `Seitenfehler Adresse=0x0 (Lesen)`, mit null-initialisiertem Feld an `Seitenfehler Adresse=0x31 (Schreiben)`. Ohne (a) ist jeder Spawn-Test ein Test der falschen Sache. Zusätzlich vor Freigabe nachzuweisen, wie in der Owner-Vorgabe genannt: Isolation der neuen Adressräume, korrekte Rechtevergabe, keine Übernahme des aktiven Vorgänger-Stacks, kontrollierte Thread-Kontext-Initialisierung, Rollback bei fehlgeschlagener Erstellung.

### E-3: `WaitProcess` als Syscall oder als Dienstnachricht

| Alt | Vorgehen | Auswirkung |
| --- | --- | --- |
| **A** | Syscall `0x00A0` | Exit-Code ohne laufenden Dienst verfügbar; eine Nummer mehr im TCB; kein Zyklus |
| B | Nachricht an einen Prozessdienst | TCB bleibt kleiner; verlangt einen laufenden Dienst und damit funktionierendes IPC |
| C | gar nicht, Zombies werden nur abgeräumt | Exit-Codes gehen verloren, kein Elternprozess kann warten |

**Empfehlung A**, solange der IPC-Transport fehlt (B ist heute nicht baubar). Wenn der Transport existiert, ist B die sauberere Microkernel-Lösung; die Nummer bleibt dann `RETIRED`.

### E-4: Abrufweg für Auditdaten

| Alt | Vorgehen | Auswirkung |
| --- | --- | --- |
| A | neuer Block `0x00B0` `TraceRead` | abfragbar ohne Transport; eine weitere privilegierte Nummer |
| B | Diagnosekanal über IPC an einen Auditdienst | architektonisch sauber; setzt Transport voraus |
| C | kein Abruf, nur Zähler im Boot-Log | Auditierbarkeit bleibt Behauptung, nicht prüfbar |

**Empfehlung B als Ziel, A als Übergang**, weil die Ratifizierungsbedingung „jedes `PermissionDenied` und jeder Rechteentzug ist zählbar und lesbar“ heute nur über A erreichbar ist. C ist keine Option, sie erfüllt die eigene Bedingung nicht.

### E-5: IRQ-ACK implizit beim Konsum oder als Syscall

| Alt | Vorgehen | Auswirkung |
| --- | --- | --- |
| A | implizit: Line wird erst frei, wenn `IpcReceive` erfolgreich war | keine neue Nummer; ein hängender Prozess blockiert die Line (im Betrieb als `STUCK_DELIVERIES` sichtbar) |
| **B** | eigener Syscall `0x0090` `IrqAck` mit `ACK`-Recht | Line kann nicht dauerhaft hängen; eine Nummer mehr; ein Prozess kann ACK missbrauchen, wenn die Prüfung schwach ist |
| C | Zeitlimit statt ACK | weicht von „maskiert bis ACK“ ab, macht das Verhalten zeitabhängig und damit schlechter prüfbar |

**Empfehlung B.** A hat den Mangel bereits gezeigt. C ist bei einem deterministischen System die falsche Richtung.

### E-6: Fehlercodes additiv erweitern oder neuer Major

| Alt | Vorgehen | Auswirkung |
| --- | --- | --- |
| **A** | neue `AbiError`-Werte anhängen (8, 9, …), bestehende 1 bis 7 unverändert | Minor-Erweiterung, alte Aufrufer verstehen alte Codes weiter |
| B | neuer Major bei jedem neuen Fehler | jeder Fehler wird ein Bruch, der Major steigt schnell |
| C | Fehler in Text umwandeln | Nicht-ABI, nicht prüfbar, keine stabile Kennung |

**Empfehlung A.** `AbiError` ist `repr(u32)`, die Werte sind stabil, Lücken sind zulässig. Ein Major-Wechsel ist nur bei Bedeutungswandel gerechtfertigt.

### E-7: `syscall_supported()` in `system/runtime` als gepflegte Liste oder abgeleitet

| Alt | Vorgehen | Auswirkung |
| --- | --- | --- |
| A | Liste bleibt handgepflegt | veraltet still, sobald der Kernel wächst; die Laufzeit meldet einen Syscall als nicht unterstützt, den der Kernel längst kann |
| **B** | Liste wird abgeleitet: das ABI-Crate führt `Syscall::implemented()` als einzige Quelle, `system/runtime` liest sie | eine Wahrheit, kein Nachpflegen |
| C | Laufzeit fragt den Kernel beim Start | ehrlich, kostet einen Syscall und braucht eine Antwortform für „nicht unterstützt“ |

**Empfehlung B**, C als spätere Verfeinerung. A ist eine zweite Definition und damit genau das, was der Contract verbietet.

---

## D. Abhängigkeiten zwischen den Entscheidungen

1. **E-1 blockiert alles Weitere.** Ohne verlustfreie Rechteabbildung ist jede neue privilegierte Nummer auf unsicherer Grundlage (`ACCESS`-Verlust betrifft `Device`-Caps und damit DMA und Geräteanspruch).
2. **E-6 muss vor E-2/E-3/E-5 entschieden sein**, weil jede dieser Nummern neue Fehlerfälle erzeugt, die sonst keinen stabilen Code bekommen.
3. **E-3 und E-4 hängen am IPC-Transport.** Solange der Transport fehlt, sind nur die Übergangsvarianten baubar; die Endvarianten bleiben dokumentiert und gesperrt.
4. **E-7 ist Voraussetzung für jede Nutzerkante**, weil die Laufzeit sonst Aufrufe verweigert, die der Kernel erlaubt.
5. `SpawnProcess` (`0x0060`) bleibt in jedem Fall gesperrt, bis E-2 abgearbeitet und die fünf Nachweise aus Abschnitt C erbracht sind.

## E. Was eine Entscheidung hier nicht ist

Keine Implementierungsfreigabe. Kein `CI_VERIFIED`. Kein Nachweis, dass eine Variante bereits funktioniert. Wer eine Alternative wählt, legt die Richtung fest; die Umsetzung ist ein eigener Schritt mit eigenen negativen Tests und eigener Freigabe.
