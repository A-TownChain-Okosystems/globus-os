# Warum das Init-ELF abgelehnt wurde (09.11.2026)

**Status:** Analyse abgeschlossen, Ursache behoben. Belege: QEMU-BIOS-Laeufe und
frischer Klon. Betroffene Grenze: Kernel → Ramdisk-Programm (Bridge B4).

## 1. Was im Log stand

Drei verschiedene Meldungen, die wie ein Fehler aussehen, aber drei Stufen sind:

1. `Programm 'init' ist kein lesbares ELF: InvalidType`
2. `KERNEL PANIC ... USER-001 user page mapping failed: InvalidAddress`
3. `Programm 'init' aus Ramdisk: 6608 Bytes ELF, Einstieg 0x400000, Code 121 Bytes`
   mit `USER-001 mapped PID=1000 RIP=0x400000 (Quelle: Ramdisk)` — der Zielzustand.

## 2. Stufe 1 — PIE ist kein ausfuehrbares ELF fuer den Kernel

`kernel/src/elf_loader.rs:150` verlangt `e_type == ET_EXEC` (Konstante `ET_EXEC`
in Zeile 23). Der Bare-Metal-Build ohne Linkerskript erzeugt aber
`Type: DYN (Position-Independent Executable file)` mit Einstieg `0x12b0`.
Der Parser antwortet `InvalidType`, dessen Display-Text `not executable` lautet.

## 3. Stufe 2 — richtige ELF-Art, falsche Ladeadresse

Erzwungenes `-C relocation-model=static` liefert `Type: EXEC`, aber ohne
Linkerskript einen Einstieg bei `0x2011c0` (2 MiB). Das Nutzercode-Fenster des
Kernels ist fest: `code_base = 0x0040_0000`, `code_size = 0x0010_0000`
(`kernel/src/userspace.rs:66-67`), also `0x400000..0x500000`. Der Einstieg liegt
darunter, die Abbildung verweigert ihn: `userspace.rs:253-254` gibt
`UserspaceError::InvalidAddress` zurueck, `main.rs:789` macht daraus einen Panic.

## 4. Die eigentliche Ursache — RUSTFLAGS verdraengt die Crate-Konfiguration

`modules/atc-shivacore/init_program/.cargo/config.toml` setzt genau das Richtige:

```toml
[target.x86_64-unknown-none]
rustflags = ["-C", "link-arg=-Tlinker.ld", "-C", "relocation-model=static"]
```

`scripts/qemu_boot.sh` exportiert aber global
`RUSTFLAGS='-C target-cpu=x86-64 --cfg curve25519_dalek_backend="serial"'`
(noetig fuer den Kernel). **Cargo-Regel:** die Umgebungsvariable `RUSTFLAGS`
ersetzt `target.*.rustflags` vollstaendig. Damit fielen `-Tlinker.ld` und
`relocation-model=static` still weg, waehrend `RUSTFLAGS` selbst weiterwirkte.
Die Init-Crate bekam also nie ihr Linkerskript.

Nachweis, beide Varianten am selben Baum gebaut:

| Build | `Type` | Einstieg |
|---|---|---|
| mit `RUSTFLAGS` | `DYN` | `0x12b0` |
| ohne `RUSTFLAGS` | `EXEC` | `0x400000` |

## 5. Die stille Falle dabei

Scheitert das Laden, **bricht der Boot nicht ab.** Der Kernel faellt auf das
eingebaute Platzhalter-Programm zurueck und schreibt
`USER-001 mapped ... (Quelle: eingebaut)`. Das Log sieht gesund aus, die Ramdisk
ist trotzdem wirkungslos. Ohne eine Zusicherung auf die Herkunft haelt man die
Bruecke fuer geschlossen, obwohl nichts aus dem Image laeuft.

## 6. Was jetzt anders ist

- `scripts/qemu_boot.sh`: baut den Init mit `env -u RUSTFLAGS cargo build
  --release` und uebergibt `init=<elf>` an den Boot-Builder.
- `.github/workflows/shivacore-kernel.yml`: eigener Init-Build-Schritt (ohne
  `RUSTFLAGS`), `init=` beim Image-Bau, und zwei Zusicherungen im Gast-Log:
  `Programm 'init' aus Ramdisk` sowie `(Quelle: Ramdisk)`. Fehlt eines, ist der
  Job rot — der stille Rueckfall kann nicht mehr als Erfolg durchgehen.
- `tools/integration_graph.py` (a-townchain): Probe B4 verlangt Bau **und**
  Uebergabe des Init, nicht mehr die blosse Erwaehnung.

## 7. Merkregel

Wer `RUSTFLAGS` global setzt, schaltet die `rustflags` jeder Crate in diesem
Baum ab. Fuer Crates, die eine feste Ladeadresse brauchen, gilt: `env -u
RUSTFLAGS` beim Bauen, und im Boot-Log immer die `Quelle:`-Zeile pruefen, nicht
nur „kein Panic".
