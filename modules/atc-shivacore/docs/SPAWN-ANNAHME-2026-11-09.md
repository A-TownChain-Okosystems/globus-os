# Spawn-Annahme: was steht, was fehlt

**Stand:** 09.11.2026, globus-os. **Keine ABI-Aenderung, keine Durchsetzung.**

## 1. Was jetzt existiert

`kernel/src/spawn.rs` (203 Zeilen, 11 Tests) prueft drei Dinge, bevor ein
Programmstart ueberhaupt diskutiert wird: den Namen gegen das Ramdisk-Verzeichnis,
das ELF gegen `ET_EXEC` und den Einstieg gegen das Nutzerfenster
`0x400000..0x500000`. Jede Ablehnung hat einen eigenen Fehlerwert; die letzte
Annahme wird als `Attempt` gemerkt, damit ein Kernel mit Ausgabe sie protokollieren
kann. Das ist die Vorstufe von Admission, nicht Admission: keine Rechtepruefung,
keine Ausfuehrung, kein neuer Syscall.

## 2. Blocker 1: Nutzerprogramme bekommen nur zwei Segmente

Beim Versuch, Init den Start zu ueberlassen, kam kein ABI-Problem, sondern ein
Abbildungsproblem: `map_user_binary` nimmt genau zwei Stuecke, das Codesegment
(`code_segment()` = erster ausfuehrbarer `PT_LOAD`) und das Datensegment
(`data_segment()` = erster beschreibbarer, nicht ausfuehrbarer `PT_LOAD`). Alles,
was das ELF sonst noch hat, wird nicht abgebildet.

Belegt mit zwei Laeufen, beide PID 1000, beide sofort nach dem Einstieg:

| Init-Fassung | Segmente im ELF | Folge |
|---|---|---|
| mit Stringliteral `"probe"` | 4 `LOAD` (R E, R, RW, .bss) | `Seitenfehler PID=Some(Pid(1000)) Adresse=0x0 (Seite nicht vorhanden, Lesen)` |
| Name byteweise auf den Stack | 2 `LOAD` (R E, RW) | `Seitenfehler PID=Some(Pid(1000)) Adresse=0x31 (Seite nicht vorhanden, Schreiben)` |

Die zweite Zeile ist der wichtigere Befund: auch ohne Stringliteral stirbt das
Programm, weil ein null-initialisiertes Feld im nicht abgebildeten `.bss` landet.
Damit gilt heute: **kein Nutzerprogramm darf Stringliterale, statische Tabellen
oder null-initialisierte Puffer verwenden.** Das ist deutlich enger als "Code und
Daten" und war bisher in keinem Dokument festgehalten.

## 3. Blocker 2: Die ABI kennt keinen Prozessstart (Kandidat, nicht beschlossen)

Ein Syscall `SpawnProcess` waere additiv, verlangt aber eine Entscheidung
des Owners, weil er die eingefrorene Syscall-ABI und das Capability-Modell beruehrt
(SHIVA-SYSCALL-ABI, AD-013). Vorschlag, bewusst nicht implementiert:

- zwei Argumentwoerter `arg0`/`arg1` tragen das 16-Byte-Namensfeld des
  SRAM-Verzeichnisses; kein Nutzerzeiger, also keine Kopie in den Adressraum
  noetig (die fehlt der ABI ohnehin noch).
- Recht: neue `ResourceType::Process` mit genau einem `Right::SPAWN`, ueber
  `create_typed` vergeben, damit der Kernel den Satz vergibt und nicht der Aufrufer.
- Ablehnungen: fehlende Capability, unbekannter Name, kein `ET_EXEC`, Einstieg
  ausserhalb des Fensters, fehlende Frames. Jeder Fall mit eigenem Status, keiner
  mit erfundenem PID.

## 4. Blocker 3: Ohne erreichbaren Frame-Allokator gibt es keinen Prozess

Der Bootpfad haelt den Allokator lokal. Selbst mit Capability und gueltigem Plan
kann ein Syscall heute keinen Adressraum bauen. Ein Spawn, der `ok(pid)`
zurueckgibt, waere eine Luege; ein Spawn, der `ResourceExhausted` sagt, waere die
Wahrheit.

## 5. Reihenfolge

1. Segmente abbilden (Blocker 1) — reine Kernel-Korrektheit, keine ABI-Aenderung,
   sofort pruefbar: Init mit Stringliteral muss laufen statt zu sterben.
2. Frame-Allokator erreichbar machen (Blocker 3).
3. Erst dann ABI 2.1 mit `SpawnProcess` (Blocker 2) — mit Owner-Entscheidung.

Solange 1 fehlt, ist jeder Spawn-Versuch ein Test der Abbildung, nicht des Spawns.

## 6. Nachtrag 14.11.2026 — die vorgeschlagene Nummer ist vergeben

Der Vorschlag nannte `SpawnProcess = 0x0050`. Diese Nummer ist inzwischen
belegt: das Anmeldefenster benutzt `Syscall::Console = 0x0050` (Clear/Write auf
die Flaeche, `docs/LOGIN_WINDOW.md`). Ein Prozessstart, wenn er kommt, braucht
deshalb eine andere Nummer; die Kollision ist hier festgehalten, damit sie nicht
spaeter als Ueberraschung auftaucht. Die Entscheidung ueber den Syscall selbst
ist weiterhin offen (Blocker 2), und Blocker 1 (nur zwei Segmente im Abbild)
besteht unveraendert.

## 7. Nachtrag 14.11.2026 — Blocker 1 ist geschlossen

`map_user_binary` nimmt weiterhin Code und Daten, aber der Bootpfad fuellt beide
jetzt aus dem ELF: `elf_loader::user_binary_from_elf` liest das ausfuehrbare
`PT_LOAD`, das beschreibbare und den `.bss`-Anteil (`p_memsz - p_filesz`) als
Nullauffuellung. Ein Programm darf damit Stringliterale, statische Tabellen und
null-initialisierte Puffer benutzen; die Regel aus Abschnitt 2 gilt nicht mehr.

Beleg: das Anmeldefenster haelt einen vorbelegten Wert (`.data`), einen Zaehler
in `.data` und einen 6 KiB grossen `.bss`-Puffer ueber zwei Seiten und liest
beide Enden zurueck; mit dem alten Kernel ergab derselbe Code einen Seitenfehler
bei `0x1d0180f`, mit dem neuen laeuft er (`docs/LOGIN_WINDOW.md`).

Damit bleiben als Voraussetzungen fuer einen echten Prozessstart: der
Frame-Allokator im Syscall-Pfad (Blocker 3, beim Boot schon als
`memory::FRAME_ALLOCATOR` veroeffentlicht) und die ABI-Entscheidung (Blocker 2,
Nummer nicht mehr 0x0050, siehe Abschnitt 6).
