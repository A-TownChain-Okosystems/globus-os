# ABI 2.0.0 — Argumentblock statt loser Register

Stand 07.11.2026, umgesetzt und im Bootlauf belegt.

## Warum die Major-Version

In ABI 1.0.0 uebergab ein Syscall Aufruf sechs Register: Cap-Slot, Opcode, vier
Argumentwoerter. Die Nutzlaenge lag in `R8` — demselben Register, das laut
Konvention auch das dritte Argumentwort war. `R9` trug gleichzeitig das vierte
Argumentwort und die ABI-Version. Einen Pufferzeiger gab es nicht.

Folge: `IpcReceive` konnte keine Daten liefern und meldete dennoch Erfolg, und
die Konvention hatte keinen Platz fuer ein siebtes Feld. Ein Aufruf, der nicht
wachsen kann, ist keine ABI, sondern eine Zusage auf Zeit.

## Aufruf

| Register | Bedeutung |
| --- | --- |
| RDI | Zeiger auf den Argumentblock im Userspace |
| RSI | Laenge des Blocks, den der Aufrufer geschrieben hat |
| RDX | ABI-Version (Major << 16 \| Minor), muss zu 2.x passen |
| RAX | Resultat 1 (Wert) |
| RBX | Resultat 2 (0 = Erfolg, sonst AbiError) |

Die Version steht bewusst VOR dem Block: der Kernel muss sie kennen, bevor er
einen Zeiger aus dem Userspace liest.

## Block (alles little endian, 64 Bit)

| Offset | Feld |
| --- | --- |
| 0 | size — Gesamtgroesse, mindestens 56 |
| 8 | opcode (u16) \| flags (u16) \| reserviert (u32) |
| 16 | capability (0 = keine) |
| 24 | arg0 |
| 32 | arg1 |
| 40 | buffer_ptr (0 = keine Nutzdaten) |
| 48 | buffer_len |

## Erweiterbarkeit

- **Anhaengen statt umdeuten.** Ein spaeterer Minor-Stand haengt Felder ab
  Offset 56 an und erhoeht `size`. Der Kernel liest `min(size, bekannt)` und
  ignoriert den Rest. Ein alter Kernel versteht einen neuen Block weiterhin.
- **Ein neuer Kernel versteht alte Bloecke**, weil `size` bei 56 bleiben darf.
- **Unbekannte Flags werden abgelehnt**, nicht ignoriert: Ein Aufruf darf nicht
  stillschweigend anders gemeint sein, als der Aufrufer denkt.
- **Luegen werden vor dem Lesen entdeckt**: `size` groesser als der gelieferte
  Puffer, `buffer_len` ohne `buffer_ptr`, Laenge ueber `MAX_SYSCALL_PAYLOAD` oder
  ein unbekannter Opcode enden als `InvalidPayload`/`InvalidSyscall`, bevor
  irgendetwas interpretiert wird.

## Was noch fehlt

Die Kopie der Nutzdaten in den bzw. aus dem Userspace-Adressraum macht das
Trampolin, nachdem es den Zeiger gegen die Prozessabbildung geprueft hat. Bis
das verdrahtet ist, gilt im Kernel weiter: Nutzdaten in `IpcSend` enden als
`EndpointUnavailable` statt als vorgetaeuschter Erfolg. Der Empfang des
Tastaturereignisses laeuft derzeit ueber den Rueckgabewert (64 Bit reichen fuer
Scancode und Zeichen), nicht ueber den Puffer.

## Umsetzung: Der Kernel liest den Block wirklich

Stand 07.11.2026, im Bootlauf belegt. Das Trampolin uebersetzt keine losen
Register mehr, sondern liest den Block in dieser Reihenfolge:

1. Version aus RDX. Stimmt die Major-Version nicht, endet der Aufruf mit
   `AbiVersionMismatch` — ohne dass ein Zeiger aus dem Userspace gelesen wurde.
2. Laenge aus RSI gegen `SYSCALL_BLOCK_MIN_SIZE..=SYSCALL_BLOCK_MAX_SIZE`.
3. Zeiger aus RDI gegen das Abbild des Aufrufers. `UserAddressSpace::range_writable`
   verlangt, dass der ganze Bereich in Daten-, Heap- oder Stackbereich liegt, und
   lehnt Ueberlauf ab. Code wird abgelehnt: lesbar, aber ein Puffer dort ist ein
   Fehler des Aufrufers. Dafuer traegt der Scheduler-Eintrag jetzt das Abbild
   (`SchedEntry::addr_space`), sonst waere jede Pruefung nur eine Behauptung.
4. Erst dann die Kopie in einen Kernpuffer (max. 256 Byte) und das Parsen des
   Blocks. Ab hier liest der Kernel nur noch eigenen Speicher.

Der Ring-3-Prozess legt den Block in seine Datenseite (0x0190_0000), nicht auf
den Stack.

### Offener Befund: ein Block auf dem Stack faultet

Die erste Fassung baute den Block auf dem Stack. Ergebnis: 5746 Seitenfehler in
einem 30-Sekunden-Lauf, jeder bei RIP 0x1800020 (der Anweisung
`mov [rsp+0x10], rdi`) mit CR2 0x4810247c. Die Messung im Timer-Tick zeigt
gleichzeitig einen korrekten RSP (0x7FDFFC0) und einen korrekten RAX/RBX/R12 —
die Zuordnung von Frame und Kontext ist also nicht offensichtlich falsch, und
die Fehleradresse passt nicht zu [rsp+0x10]. Mit der Datenseite tritt kein
einziger Seitenfehler auf. Der Stack-Pfad bleibt damit unerklaert und offen;
solange er offen ist, taugt ein Ring-3-Programm mit Stack nicht als Beweis.
