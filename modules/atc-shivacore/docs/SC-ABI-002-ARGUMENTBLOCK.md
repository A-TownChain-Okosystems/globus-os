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
