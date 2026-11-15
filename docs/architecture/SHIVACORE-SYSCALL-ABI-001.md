# SHIVACORE-SYSCALL-ABI-001 — System Call ABI Contract

**Status:** ENTWURF, nicht ratifiziert. Reine Spezifikation, keine Implementierung.
**Untersuchter Quellstand:** `globus-os` `main` = `1fc8c169` (15.11.2026).
**Normative Grundlage im Code:** `system/shivacore-api/src/lib.rs` (Paket `libshivacore`) ist die einzige ABI-Definition. Dieses Dokument beschreibt sie, es ersetzt sie nicht.
**Verbindliche Vorgaben:** AD-012 (Microkernel-Grenze), AD-EXEC-KERNEL-ENFORCEMENT, SHIVA-SYSCALL-ABI, SHIVA-KERNEL-REUSE.
**Regel dieses Dokuments:** Keine zweite Syscall-Implementierung, keine parallele ABI-Definition. Wo Code und Text abweichen, gilt der Code als Ist-Stand und der Text als Soll-Stand; die Abweichung wird ausdrücklich benannt, nicht stillschweigend aufgelöst.

---

## 1. ABI-Versionierung

| Feld | Wert im Stand `1fc8c169` |
| --- | --- |
| `ABI_MAJOR` | 2 |
| `ABI_MINOR` | 0 |
| `ABI_VERSION` | `0x0002_0000` (MAJOR << 16 \| MINOR) |
| Übergeben in | `RDX` (vor dem ersten Lesen eines Nutzerzeigers) |
| Handshake | `AbiHandshake { major, minor }`, Prüfung über `major == ABI_MAJOR` |
| Blockgröße | `SYSCALL_BLOCK_MIN_SIZE` = 56, `SYSCALL_BLOCK_MAX_SIZE` = 256 |
| Nutzlastgrenze | `MAX_SYSCALL_PAYLOAD` = 1 MiB |

**Versionsregeln (verbindlich):**

1. Ein Major-Wechsel ist nur zulässig, wenn bestehende Aufrufe ihre Bedeutung verlieren oder Register/Speicherlayout nicht erweiterbar sind. Bisheriger einziger Grund: ABI 1.0.0 hatte `R8` doppelt belegt (Argumentwort und Längenfeld) und `R9` doppelt (Argumentwort und Versionsnummer), ein Pufferzeiger hatte keinen Platz.
2. Ein Minor-Wechsel darf **nur** additiv sein: neue Felder ab Offset 56, `size` wächst, der Kernel liest `min(size, bekannt)` und ignoriert den Rest. Neue Opcodes sind ebenfalls additiv.
3. Ein alter Kernel akzeptiert neue Blöcke, ein neuer Kernel akzeptiert alte Blöcke. Diese beiden Richtungen sind die eigentliche Kompatibilitätszusage.
4. Unbekannte Flags werden **abgelehnt** (`InvalidPayload`), nicht ignoriert. Unbekannte Opcodes werden abgelehnt (`InvalidSyscall`).
5. Eine falsche ABI-Version führt zu `AbiVersionMismatch`, **bevor** ein Nutzerzeiger gelesen wird. Zählung und Logzeile unterbleiben in diesem Pfad bewusst (ein Prozess mit falscher Version ist kein ABI-Fehler des Kernels).
6. Nummern werden **nie** neu belegt. Ein zurückgezogener Syscall bleibt als Lücke mit dem Vermerk `RETIRED` stehen. Eine Umwidmung ist nur durch eine formale Architekturentscheidung mit Major-Wechsel möglich.
7. Es gibt keinen stillen Bruch: keine Änderung der Argumentbedeutung, der Rückgabeform oder der Fehlerpriorität innerhalb eines Major-Stands.

## 2. Nummernraum

Die neun implementierten Syscalls sind vollständig inventarisiert. Die Werte stammen aus dem Enum `Syscall` in `libshivacore`; die Zuordnung ist Vertragsbestandteil.

### 2.1 Implementiert (ABI 2.0.0)

| ID | Name | Ressource | Capability | Im Dispatcher erreichbar |
| --- | --- | --- | --- | --- |
| `0x0001` | `Yield` | keine | keine | ja |
| `0x0002` | `Exit` | Process | keine (Selbstbeendigung) | ja, verlangt Exit-Code in `arg0` |
| `0x0010` | `IpcSend` | IpcChannel | `WRITE` auf den Kanal | ja, liefert heute `EndpointUnavailable` bei Nutzlast |
| `0x0011` | `IpcReceive` | IpcChannel | `READ` auf den Kanal | ja, liefert `ok(0)` bei leerem Kanal |
| `0x0020` | `CapabilityQuery` | Capability selbst | keine (Abfrage der eigenen Cap) | ja |
| `0x0030` | `HandleClose` | Capability selbst | Eigentümerschaft | ja |
| `0x0040` | `MonotonicTime` | keine | keine | ja |
| `0x0050` | `Console` | Console | `WRITE` auf die Console-Cap | ja, `arg0` = `CONSOLE_OP_CLEAR` oder `CONSOLE_OP_WRITE` |
| `0x0070` | `IdentityCheck` | keine | Konsoleneigentümer (IDENT-003) | ja |

### 2.2 Reserviert (vom Owner festgelegt, ohne Implementierung)

| ID | Name | Begründung der Reservierung |
| --- | --- | --- |
| `0x0060` | `SpawnProcess` | Vorgabe des Owners. **Keine Umwidmung ohne formale Architekturentscheidung.** `IdentityCheck` liegt deshalb auf dem nächsten Schritt der Konvention (`0x0070`) und nicht auf `0x0060`. |

### 2.3 In diesem Dokument zugeordnet (`PROPOSED`, Implementierung braucht eigene Freigabe)

| ID | Name | Zweck | Blockiert heute |
| --- | --- | --- | --- |
| `0x0080` | `MemoryMap` | Abbildung eines Bereichs in den eigenen Adressraum | Speicher-Nutzerkante |
| `0x0081` | `MemoryUnmap` | Aufheben einer Abbildung | Speicher-Nutzerkante |
| `0x0082` | `MemoryProtect` | Rechte einer Abbildung ändern (W^X-Gate) | Code-Admission |
| `0x0083` | `AddressSpaceCreate` | Neuen Adressraum anlegen (Voraussetzung für Spawn) | `SpawnProcess` |
| `0x0090` | `IrqAck` | Bestätigen einer zugestellten Interrupt-Line | IRQ-Zustellung an Dienste |
| `0x0091` | `TimerSetDeadline` | Deadline auf einer Timer-Cap setzen | zeitgesteuerte Dienste |
| `0x00A0` | `WaitProcess` | Auf Beendigung eines Kindprozesses warten, Exit-Code lesen | Prozessverwaltung als Dienst |

### 2.4 Dauerhaft gesperrt (`FORBIDDEN`)

| Bereich | Grund |
| --- | --- |
| Dateioperationen (`Open`, `Read`, `Write`, `Close`, `Seek`) | Der Kernel implementiert kein Dateisystem (Abschnitt 9). Dateizugriff ist eine Nachricht an den VFS-Dienst. |
| Netzwerkoperationen (`Socket`, `Bind`, `Connect`, `Send`, `Receive`) | Kein Netzwerkstack im Kernel (AD-012). |
| Dienstverwaltung (`Start`, `Stop`, `Restart`) | Der Service Manager ist ein Nutzerraumdienst. |
| Code-Admission als eigener Syscall | Admission wird beim Ausführungsmapping durchgesetzt, nicht auf Zuruf eines Prozesses. |

**Regeln für den Nummernraum:**

1. Blöcke zu `0x10`: die niedere Stelle gehört zu einem Bereich, die obere zu einer Variante.
2. `0x0000` bleibt unbenutzt. Ein Nullwert ist kein gültiger Opcode und dient als Fehlerindikator in Tests.
3. `0x00F0` bis `0x00FF` bleiben frei für experimentelle Aufrufe, die nie in ein Release gehen.
4. Kollisionsfreiheit wird gegen das Enum geprüft, nicht gegen Prosa. Der Prüfer dafür ist Abschnitt 12.

## 3. Calling Convention

### 3.1 Register beim Eintritt

| Register | Bedeutung |
| --- | --- |
| `RDI` | Zeiger auf den `SyscallBlock` im Userspace |
| `RSI` | Länge des Blocks in Bytes |
| `RDX` | ABI-Version, `0x0002_0000` |
| `RAX` | Rückgabewert (`value`), `0` bei Fehler |
| `RBX` | ABI-Status: `0` = Erfolg, sonst `AbiError`-Wert |

`rbx` darf in Inline-Assembly nicht als Operand stehen (LLVM-intern reserviert); die Clobber-Liste ist `clobber_abi("sysv64")`.

### 3.2 Blocklayout (little endian, 64 Bit)

| Offset | Feld | Typ | Bedeutung |
| --- | --- | --- | --- |
| 0 | `size` | u64 | Gesamtgröße, mindestens 56 |
| 8 | `opcode` \| `flags` \| reserviert | u16 \| u16 \| u32 | Flags heute `SYSCALL_FLAG_NONE` = 0 |
| 16 | `capability` | u64 | Cap-Handle, `0` = keine |
| 24 | `arg0` | u64 | Bedeutung je Opcode |
| 32 | `arg1` | u64 | Bedeutung je Opcode |
| 40 | `buffer_ptr` | u64 | Zeiger auf Nutzdaten, `0` = keine |
| 48 | `buffer_len` | u64 | Länge der Nutzdaten |

### 3.3 Prüfreihenfolge (bindend)

1. `RDX` gegen `ABI_MAJOR` prüfen, sonst `AbiVersionMismatch`.
2. Blocklänge `RSI` gegen 8 und `SYSCALL_BLOCK_MAX_SIZE` prüfen.
3. `size` gegen `SYSCALL_BLOCK_MIN_SIZE` und gegen die gelieferte Länge prüfen (Größe größer als der Puffer ist unzulässig).
4. `flags` gegen bekannt prüfen.
5. `buffer_len` gegen `MAX_SYSCALL_PAYLOAD` prüfen; Länge ohne Zeiger und Zeiger ohne Länge sind unzulässig.
6. Opcode auflösen, sonst `InvalidSyscall`.
7. Capability prüfen (Abschnitt 5).
8. Erst danach Nutzdaten kopieren.

Die Reihenfolge ist selbst Sicherheitsmechanik: ein Prozess ohne Recht darf nicht erfahren, ob ein Opcode existiert, ob ein Handle gültig ist oder ob ein Endpunkt offen ist. Rechteprüfung steht **vor** Inhaltsprüfung.

## 4. Fehlercodes

`AbiError` ist `#[repr(u32)]`, die Zahlen sind stabil und werden nie neu belegt.

| Wert | Name | Bedeutung | Typische Ursache |
| --- | --- | --- | --- |
| 0 | (Erfolg) | `RBX` = 0, `RAX` = `value` |  |
| 1 | `InvalidSyscall` | Opcode unbekannt | Tippfehler, neue Nummer auf altem Kernel |
| 2 | `InvalidHandle` | Handle unbekannt oder freigegeben | doppeltes `HandleClose`, fremdes Handle |
| 3 | `PermissionDenied` | Capability deckt die Operation nicht | fehlendes Recht, fremder Eigentümer |
| 4 | `InvalidPayload` | Argumente oder Größen unzulässig | `size` zu klein, Länge ohne Zeiger, unbekanntes Flag |
| 5 | `EndpointUnavailable` | Endpunkt existiert nicht oder ist geschlossen | IPC ohne Transport, geschlossener Kanal |
| 6 | `ResourceExhausted` | Ressourcengrenze erreicht | Kanal voll, Frame-Vorrat leer |
| 7 | `AbiVersionMismatch` | Aufrufer erwartet andere ABI-Version | falsche Immediate in `RDX` |

**Fehlersemantik (verbindlich):**

1. `value` ist bei jedem Fehler `0`. Ein Fehler darf nie als Teilerfolg gelesen werden. Beispiel aus dem Ist-Stand: `IpcReceive` liefert bei fehlendem Transport `EndpointUnavailable` mit `value = 0`, damit kein Prozess „0 Bytes empfangen“ als Erfolg deutet.
2. Es gibt keinen Sammelfehler. Ein Aufruf meldet genau einen Grund.
3. Kein Fehlercode wird für zwei Bedeutungen verwendet. Wächst der Bedarf, kommt ein neuer Wert hinzu (Minor-Erweiterung von `AbiError` ist zulässig, weil die Werte stabil bleiben).
4. Interne Zustände werden nicht nach außen gespiegelt. `LiveIpcError::NotParticipant` wird zu `PermissionDenied`, `Full`/`Empty` zu `ResourceExhausted`; die Übersetzung ist Vertragsbestandteil.

## 5. Capability Enforcement

**Grundsätze:**

1. Jeder privilegierte Aufruf setzt eine benannte Capability mit benanntem Recht voraus. Keine impliziten Root-Rechte.
2. Keine pauschalen `EXEC`- oder `DELEGATE`-Rechte. `DELEGATE` wird nur dort verlangt, wo eine Capability tatsächlich weitergegeben wird (`HandleDuplicate`, nicht in ABI 2.0.0).
3. Deny-by-default: fehlt das Recht, ist die Antwort `PermissionDenied`, nicht „teilweise erlaubt“.
4. Eine Capability autorisiert genau ihr Objekt. Eine Device-Cap öffnet kein zweites Gerät, eine Kanal-Cap keinen zweiten Kanal. Wer beim Anspruch nur „ist die Cap gültig?“ fragt, baut einen Generalschlüssel.
5. Der Eigentümer einer Capability ist Teil der Prüfung. `HandleClose` auf eine fremde Cap ist `PermissionDenied`, nicht `InvalidHandle`.

**Rechtesatz (Ist-Stand, zwei Darstellungen im Code):**

| Bit | Kernel `capability::Rights` (u16) | ABI `CapabilityRights` (u8) | ABI-Enum `CapabilityRight` |
| --- | --- | --- | --- |
| 1 | `READ` | `READ` | `Read` |
| 2 | `WRITE` | `WRITE` | `Write` |
| 4 | `EXEC` | `EXECUTE` | `Execute` |
| 8 | `DELEGATE` | `DELEGATE` | `Grant` |
| 16 | `INSPECT` | `INSPECT` | `Inspect` |
| 32 | `SUBSCRIBE` | fehlt | fehlt |
| 64 | `ACK` | fehlt | fehlt |
| 128 | `SET_DEADLINE` | fehlt | fehlt |
| 256 | `ACCESS` | **nicht darstellbar** (u8) | fehlt |
|  |  |  | `Revoke` (ohne Bit) |

**Befund, der vor der Implementierung entschieden werden muss (P0 Capability-SSOT):** Es existieren drei Rechtsvokabulare. `CapabilityQuery` liefert `rights.bits()` der Kernel-Tabelle als `u64`; der höchste Wert `ACCESS` = 256 passt in kein `u8`. Ein Dienst, der die Antwort in `CapabilityRights` (u8) ablegt, liest für `ACCESS` den Wert `0`, also „kein Recht“. Ebenso hat das ABI-Enum `Revoke` ohne entsprechendes Bit und kennt `SUBSCRIBE`, `ACK`, `SET_DEADLINE`, `ACCESS` nicht. Der Contract verlangt **eine** Darstellung, bevor ein einziger neuer Syscall implementiert wird: `u16`-Bitfeld als Wahrheit, `CapabilityRights` auf `u16` erweitert, `CapabilityRight` als benannte Konstanten darauf, `Revoke` entweder mit Bit oder aus dem Enum entfernt.

**Capability-Anforderung je Syscall:**

| Syscall | Capability | Recht | Prüfpunkt |
| --- | --- | --- | --- |
| `Yield` | keine |  |  |
| `Exit` | keine |  | Aufräumen widerruft alle Caps des Prozesses |
| `IpcSend` | Kanal | `WRITE` | vor dem Lesen der Nutzdaten |
| `IpcReceive` | Kanal | `READ` | vor dem Lesen der Nutzdaten |
| `CapabilityQuery` | eigene Cap | keine | Handle muss existieren |
| `HandleClose` | eigene Cap | Eigentümerschaft | vor dem Widerruf |
| `MonotonicTime` | keine |  |  |
| `Console` | Console | `WRITE` | vor jeder Ausgabe |
| `IdentityCheck` | keine | Konsoleneigentümer | vor dem Vergleich |
| `MemoryMap` (proposed) | Memory oder Adressraum | `WRITE`, bei `EXEC` zusätzlich Admission | vor dem Setzen der Seitentabellen |
| `MemoryProtect` (proposed) | Memory | `WRITE`, bei `EXEC` zusätzlich Admission | vor `update_flags` |
| `SpawnProcess` (reserved) | Process | `SPAWN` (neu, `Rights`-Erweiterung) | vor dem Anlegen des Adressraums |
| `IrqAck` (proposed) | Irq | `ACK` | vor dem Unmaskieren |
| `TimerSetDeadline` (proposed) | Timer | `SET_DEADLINE` | vor dem Eintrag in die Queue |

## 6. Memory Safety

1. **Kein Kernelzeiger aus Nutzersicht.** `buffer_ptr` wird ausschließlich als Nutzeradresse interpretiert und gegen die Prozessabbildung geprüft. Ein Zeiger in den Kernelbereich wird abgelehnt (`InvalidPayload`), nicht umgebogen.
2. **Länge ohne Zeiger und Zeiger ohne Länge sind unzulässig.** `buffer_len > 0` mit `buffer_ptr == 0` und umgekehrt ergeben `InvalidPayload`.
3. **Grenzen:** `buffer_len` ≤ `MAX_SYSCALL_PAYLOAD` (1 MiB). Der Console-Pfad hat eine eigene, kleinere Grenze (`CONSOLE_MAX_BYTES` = 1024), die zur Übersetzungszeit gegen die globale Grenze geprüft wird.
4. **Copy-in vor Copy-out:** Nutzdaten werden nach der Rechteprüfung vollständig in den Kernel kopiert, dort geprüft, und erst dann verarbeitet. Kein doppeltes Lesen derselben Nutzeradresse (TOCTOU). Der Kernel glaubt keinen vom Aufrufer gelieferten Hash.
5. **Kein Überlauf:** alle Längen- und Offsetrechnungen mit geprüften Typen (checked add/mul); ein Überlauf ist `InvalidPayload`.
6. **Rückweg:** Ergebnisse werden erst nach vollständiger Prüfung zurückgeschrieben; bei Fehler wird nichts in den Nutzerpuffer geschrieben.
7. **Der Status ist nicht Teil des Nutzerpuffers.** `RAX`/`RBX` tragen das Ergebnis, damit ein fehlerhafter Pufferzeiger den Status nicht verfälschen kann.

## 7. IPC

**Ist-Stand, ehrlich benannt:** Das IPC-Subsystem wird gebaut, aber es gibt keinen Transport. `IpcSend` mit Nutzlast liefert `EndpointUnavailable`, `IpcReceive` liefert `ok(0)`. Das ist kein Mangel der Spezifikation, sondern eine offene Transportkante; der Contract beschreibt die Semantik, die gelten muss, sobald sie existiert.

**Soll-Semantik:**

| Begriff | Definition |
| --- | --- |
| Kanal | Objekt mit Eigentümer, Richtung und Kapazität; durch eine Capability vom Typ `IpcChannel` referenziert |
| Nachricht | endliche Bytefolge bis `MAX_SYSCALL_PAYLOAD`, mit Absender-PID und Zeitstempel |
| Endpunkt | die dem Prozess zugewiesene Seite eines Kanals; ein Handle je Prozess |
| Handle | `CapabilityHandle(u64)`; `0` bedeutet „keine“ |
| Zustellung | vollständig oder gar nicht. Kein Teilzustellen, kein Abschneiden |

**Verbindliche Regeln:** FIFO je Kanal; ein voller Kanal ist `ResourceExhausted` und **kein** Warten im Kernel; eine leere Empfangsseite ist `ok(0)` mit `value = 0` und **nicht** `EndpointUnavailable` (leer ist kein Fehler, fehlend ist einer); nach `HandleClose` ist der Kanal für den Halter `EndpointUnavailable`; ein Prozess ohne `READ` erfährt nicht, dass der Kanal existiert (`PermissionDenied` vor jeder Inhaltsprüfung).

## 8. Process

| Syscall | Status | Semantik |
| --- | --- | --- |
| `Exit` `0x0002` | implementiert | `arg0` = Exit-Code. Der Kernel widerruft alle Capabilities, setzt den Prozess auf Zombie, gibt den Adressraum verzögert frei, leert die TLB. Reihenfolge bindend: Capabilities widerrufen, TLB leeren, Prozess parken. |
| `SpawnProcess` `0x0060` | reserviert | `arg0` = Zeiger auf Namens-/Argumentpuffer, `arg1` = Länge, Rückgabe = neue PID. Voraussetzungen: (a) alle `PT_LOAD`-Segmente werden abgebildet, nicht nur Code und Daten; (b) der Frame-Allokator ist prozessübergreifend erreichbar; (c) `AddressSpaceCreate` existiert; (d) eine Process-Cap mit `SPAWN`. Ohne (a) bis (d) wäre `ok(pid)` gelogen und `ResourceExhausted` die ehrliche Antwort. |
| `WaitProcess` `0x00A0` | vorgeschlagen | `arg0` = PID, Rückgabe = Exit-Code. Ohne diesen Aufruf kann kein Dienst einen Kindprozess einsammeln; die Alternative ist ein Warten über IPC an einen Prozessdienst. |

`SpawnProcess` bleibt gesperrt, solange (a) und (b) fehlen. Das ist keine Formalie: heute sterben Nutzerprogramme an einem Stringliteral oder einer null-initialisierten Tabelle, weil `.bss` und weitere Segmente nicht abgebildet werden.

## 9. Memory

| Syscall | Status | Semantik |
| --- | --- | --- |
| `MemoryMap` `0x0080` | vorgeschlagen | `arg0` = Wunschadresse oder 0 (Kernel wählt), `arg1` = Länge, `buffer_ptr` = Rechtebeschreibung, Rückgabe = Basisadresse. Rechte als Bitfeld (read/write/exec); `exec` löst den Admission-Pfad aus und ist ohne gültige ExecutionPolicy `PermissionDenied`. |
| `MemoryUnmap` `0x0081` | vorgeschlagen | `arg0` = Basisadresse, `arg1` = Länge. Nur eigene Abbildungen; fremde sind `InvalidHandle`, nicht `PermissionDenied` (die Existenz fremder Abbildungen wird nicht bestätigt). |
| `MemoryProtect` `0x0082` | vorgeschlagen | Ändert Rechte bestehender Abbildungen. Muss unter dasselbe Gate wie `MemoryMap` mit `exec`: sonst entsteht ein Weg, Daten ausführbar zu machen und die Admission zu umgehen. |
| `AddressSpaceCreate` `0x0083` | vorgeschlagen | Legt einen leeren Adressraum an, Rückgabe = Handle. Voraussetzung für `SpawnProcess`. |

Verbindlich: `W^X` wird im Kernel erzwungen, nicht vom Aufrufer zugesichert. Eine Abbildung mit `write` und `exec` zugleich ist unzulässig, solange kein ausdrücklich befristeter Installationszustand spezifiziert ist. Daten und Stack sind nicht ausführbar (`NO_EXECUTE`), unabhängig davon, ob ein Loader das anders wünscht.

## 10. VFS

**Architekturentscheidung C dieses Contracts:** Es gibt **keine** Datei-Syscalls. `Open`, `Read`, `Write`, `Close`, `Seek` sind dauerhaft gesperrte Nummern (Abschnitt 2.4).

Begründung: Der Kernel implementiert kein Dateisystem (AD-012, siehe Dependency-Matrix vom 15.11.2026). Der Kernel stellt für Dateizugriff ausschließlich Mechanismen bereit: eine Capability vom Typ `FileSystem` (bereits im Enum vorhanden), IPC zum VFS-Dienst, und ab `0x0080` die Speicherabbildung für Puffer. Der VFS-Dienst lebt im Nutzerraum; er ist der einzige Ort, an dem Pfade, Journale und Blöcke existieren dürfen.

Was der Contract deshalb statt Datei-Syscalls festlegt:

1. Ein Prozess erhält beim Start eine `FileSystem`-Capability für seinen Namensraum, nicht mehr.
2. Jede Dateioperation ist eine Nachricht an den VFS-Dienst über einen Kanal, dessen Endpunkt im Startvertrag steht.
3. Puffer werden über `MemoryMap`/`MemoryUnmap` bereitgestellt, nicht über Kernel-Kopien. Ein Dateiinhalt von 100 MiB darf nicht durch den Syscall-Puffer laufen.
4. Der Kernel prüft die Capability beim Kanalzugriff, nicht die Pfadsemantik. Pfadauflösung ist Dienstsache.

**Abweichung, die ausdrücklich benannt wird:** Die Anforderungsliste nennt „VFS: Open, Read, Write, Close“ als ABI-Abschnitt. Das widerspricht der gleichzeitig verbindlichen Entscheidung C („VFS bleibt User-Space-Dienst“) und der Microkernel-Grenze. Dieser Contract löst den Widerspruch zugunsten von C auf und liefert stattdessen die Mechanismen. Sollte der Owner Datei-Syscalls im Kernel wollen, ist das eine Änderung von AD-012 und keine Detailfrage der ABI.

## 11. Sicherheit: Auditierbarkeit und Rechteentzug

1. **Jeder Syscall ist prüfbar.** Der Dispatcher ist die einzige Nutzer-Kernel-Kante. Es gibt keinen zweiten Weg (kein `int 0x80` neben dem Gate, kein Modulpfad zur Laufzeit: `lkm.rs` ist ausschließlich im Testbuild eingebunden).
2. **Rechteentzug ist vollständig.** `Exit` und Dienstneustart widerrufen alle Capabilities des Prozesses; die TLB wird danach geleert, nicht davor. Ein Neustart behält ausdrücklich die Capabilities, weil der Startvertrag neu geschrieben wird.
3. **Auditierbarkeit** ist heute teilweise: ein Auditlog existiert im Kernel (`security.rs`), ist aber nicht exportierbar und hat keinen Syscall. Der Contract verlangt für die Ratifizierung: jedes `PermissionDenied` und jeder Rechteentzug ist zählbar, und die Zähler sind über einen definierten Weg lesbar (`0x00A0`-Block oder ein Diagnosekanal). Loggen ohne Abrufweg ist keine Auditierung.
4. **Kein Vertrauen durch Anwesenheit.** Eine gültige Signatur erteilt keine Capabilities. Rechte entstehen nur durch Vergabe, nicht durch Herkunft.
5. **Speicherung von Geheimnissen ist nicht Teil der ABI.** Kein Syscall transportiert Schlüssel; `IdentityCheck` vergleicht gegen einen Anmeldedatensatz und gibt `0/1/2/3` zurück, mehr nicht.

## 12. Versionierung und Kompatibilität

**Kompatibilitätsprüfung gegen die drei genannten Verbraucher, Stand `1fc8c169`:**

| Verbraucher | Befund | Folge für den Contract |
| --- | --- | --- |
| `libshivacore` (ABI-Crate) | enthält Enum, Block, Fehler, Grenzen; 14 eigene Tests | Einzige Definition. Neue Nummern gehören ausschließlich hierher. |
| Kernel-Dispatcher | `match` über alle neun Nummern, unbekannte Nummer = `InvalidSyscall` | Neue Nummer ohne Dispatcher-Arm ist ein toter Vertrag. Nummer und Arm werden im selben Schritt beschlossen. |
| `system/runtime` (`core_api.rs`) | nutzt `libshivacore::{ABI_VERSION, AbiError, Syscall}`, führt eine `syscall_supported()`-Liste | Muss bei jeder neuen Nummer mitgeführt werden, sonst meldet die Laufzeit einen Syscall als „nicht unterstützt“, den der Kernel bereits kann. |
| `atc-sdk` (`sdk/src/lib.rs`) | **keine** ABI-Symbole, keine Syscall-Nummern | Keine parallele ABI-Definition vorhanden, gut. Das SDK darf keine eigenen Nummern einführen; es re-exportiert `libshivacore`. |
| ATCLang | keine Syscall-Nummern im Repo | Sprache bindet die ABI später über eine Runtime, nicht über hartkodierte Zahlen. Offene Anforderung: die Runtime muss die Version über `RDX` führen, nicht raten. |
| Service Space | importiert `libshivacore` (MADT/ACPI-Typen), keine eigenen Nummern | Kein Konflikt. |

**Verbot:** Keine Kopie der Nummerntabelle in einem anderen Crate, kein Nachbau in SDK, Shell oder Editionen. Wer die Zahlen braucht, hängt an `libshivacore`. Eine zweite Tabelle ist ein stiller ABI-Bruch auf Zeit.

**Prüfregel (Vorschlag für `scripts/check_evidence_binding.py` oder einen eigenen Prüfer):** Die Nummern in diesem Dokument werden gegen das Enum `Syscall` verglichen; jede Differenz ist Exit 1. Dieselbe Prüfung vergleicht die Rechtebits in Abschnitt 5 gegen `capability::Rights`. Damit kann dieses Dokument nicht veralten, ohne dass es auffällt.

## 13. Negative Sicherheitsfälle (vor Implementierung zu spezifizieren, je mit erwartetem Fehler)

| Nr | Fall | Erwartung |
| --- | --- | --- |
| N-01 | Unbekannter Opcode, z. B. `0x0061` | `InvalidSyscall`, keine Zustandsänderung |
| N-02 | Falsche ABI-Version in `RDX` | `AbiVersionMismatch`, kein Zeigerlesen |
| N-03 | `size` kleiner als 56 | `InvalidPayload` |
| N-04 | `size` größer als gelieferte Länge | `InvalidPayload` |
| N-05 | `buffer_len` > 1 MiB | `InvalidPayload` |
| N-06 | `buffer_ptr` = 0 bei `buffer_len` > 0 | `InvalidPayload` |
| N-07 | `buffer_ptr` im Kernelbereich | `InvalidPayload` |
| N-08 | Unbekanntes Flag gesetzt | `InvalidPayload` |
| N-09 | `IpcSend` ohne `WRITE` auf dem Kanal | `PermissionDenied`, kein Hinweis auf Existenz des Kanals |
| N-10 | `HandleClose` auf fremde Cap | `PermissionDenied` |
| N-11 | `HandleClose` zweimal | `InvalidHandle` |
| N-12 | `IpcReceive` auf geschlossenen Kanal | `EndpointUnavailable`, `value = 0` |
| N-13 | `IpcReceive` auf leeren Kanal | `ok(0)`, `value = 0` |
| N-14 | `MemoryMap` mit `write` und `exec` zugleich | `PermissionDenied` (W^X) |
| N-15 | `MemoryProtect` auf `exec` ohne Admission | `PermissionDenied`, Abbildung unverändert |
| N-16 | `SpawnProcess` ohne Process-Cap mit `SPAWN` | `PermissionDenied`, kein Adressraum angelegt |
| N-17 | `SpawnProcess` auf ein ELF mit mehr als einem `PT_LOAD`-Segment, solange (a) offen ist | `ResourceExhausted` oder ehrlicher Fehler, **nicht** `ok(pid)` |
| N-18 | `IrqAck` auf fremde Line | `PermissionDenied`, Line bleibt maskiert |
| N-19 | Prozess beendet sich mit offenen Caps | alle Caps widerrufen, TLB geleert, danach Zombie |
| N-20 | `CapabilityQuery` auf `ACCESS`-Cap über `CapabilityRights` (u8) gelesen | heute 0 (Fehler), muss nach SSOT-Entscheid 256 sein |

## 14. Offene Entscheidungen vor der Ratifizierung

| Nr | Frage | Warum sie blockiert |
| --- | --- | --- |
| E-1 | Rechte-SSOT: `u16`-Bitfeld als einzige Wahrheit, `CapabilityRights` erweitern oder ersetzen, `Revoke` mit Bit oder aus dem Enum entfernen | Ohne das ist `ACCESS` für Dienste nicht lesbar (Befund Abschnitt 5) |
| E-2 | `SpawnProcess`-Voraussetzungen (a)-(d) in welcher Reihenfolge | Ohne (a)/(b) ist die Nummer nicht ehrlich implementierbar |
| E-3 | Wird `WaitProcess` ein Syscall oder eine Dienstnachricht | Bestimmt, ob `0x00A0` belegt wird |
| E-4 | Wo lebt der Audit-Abrufweg: neuer Block oder Diagnosekanal | Auditierbarkeit ist Ratifizierungsbedingung |
| E-5 | `0x0090`/`0x0091`: ACK als Syscall oder implizit beim Konsum | Ein hängender Prozess blockiert sonst die Line |
| E-6 | Minor-Erweiterung von `AbiError` erlaubt oder neuer Major bei neuen Fehlern | Bestimmt die Vertragsdauer |
| E-7 | Gilt die `syscall_supported()`-Liste in `system/runtime` als zweite Wahrheit oder wird sie abgeleitet | Eine gepflegte Kopie ist eine zweite Definition |

## 15. Abnahmekriterien (Stand dieses Entwurfs)

| Kriterium | Stand |
| --- | --- |
| Alle neun bestehenden Syscalls inventarisiert | erfüllt (Abschnitt 2.1) |
| Reservierte IDs dokumentiert | erfüllt (`0x0060`, Abschnitt 2.2) |
| Neue IDs kollisionsfrei zugeordnet | erfüllt, Kollisionsfreiheit gegen das Enum geprüft (Abschnitt 2.3) |
| Argument- und Rückgabesemantik definiert | erfüllt für die neun; `PROPOSED` teilweise offen (Abschnitte 3, 8, 9) |
| Capability-Anforderung je Syscall dokumentiert | erfüllt (Abschnitt 5) |
| Fehlercodes vereinheitlicht | definiert; ein Ist-Befund offen (Abschnitt 5, E-1) |
| ABI-Versionierungsstrategie festgelegt | erfüllt (Abschnitt 1) |
| Kompatibilität mit ATCLang, GlobusOS, atc-sdk geprüft | erfüllt (Abschnitt 12) |
| Negative Sicherheitsfälle spezifiziert | erfüllt (20 Fälle, Abschnitt 13) |
| Dokumentations- und Evidence-Prüfer bestanden | erfüllt beim Aufnahme-Commit |
| Keine zweite Syscall-Definition | erfüllt, `libshivacore` bleibt einzige Quelle |

**Nicht Teil dieses Schritts:** Implementierung, Änderung von `libshivacore`, Änderung des Dispatchers, neue Capability-Typen. Jede dieser Änderungen braucht eine eigene Freigabe nach Ratifizierung dieses Contracts.
