# Kernel-Image bauen

Der Weg vom Quelltext zum bootfähigen Image, und was dabei festgelegt ist.

## Zwei Stufen, zwei Crates

| Stufe | Befehl | Ergebnis |
|---|---|---|
| Kernel-ELF | `cargo +nightly build -p shivacore --profile kernel --target x86_64-unknown-none --features x86-boot` | `target/x86_64-unknown-none/kernel/shivacore` |
| Image | `boot <kernel-elf> <ausgabe>` (aus `modules/atc-shivacore/boot`) | BIOS- und UEFI-Disk-Images |

Der Builder kompiliert den Kernel **nicht**. Er liest den fertigen ELF und legt
ihn per `bootloader` 0.11 in ein Image. Die Panik-Strategie des Kernels hängt
deshalb am Profil, mit dem der ELF gebaut wird, und das ist im Root-Manifest
festgelegt, nicht beim Aufrufer:

```toml
[profile.kernel]
inherits = "release"
panic = "abort"
```

Damit gilt: im Image gibt es keine Entfaltung, eine Panik endet an der
Panikstelle. Test- und Host-Builds bleiben beim Standardprofil mit Entfaltung,
damit `should_panic`-Tests weiter funktionieren. `scripts/qemu_boot.sh` baut
den Kernel entsprechend mit `--profile kernel`.

Voraussetzungen auf dem Build-Rechner: Toolchain `nightly` und das Target
`x86_64-unknown-none` (`rustup target add x86_64-unknown-none --toolchain nightly`).

## Befund: das Image ließ sich nicht mehr bauen

Mit dem Nightly-Stand vom 14.11.2026 brach der Kernel-Build mit fünf Fehlern ab,
alle in `modules/atc-shivacore/kernel/src/interrupts.rs`:

- zwei `extern "C"`-Blöcke ohne `unsafe` (Edition 2024 verlangt
  `unsafe extern "C"`)
- drei `#[no_mangle]`-Attribute ohne `unsafe(...)` (Edition 2024 verlangt
  `#[unsafe(no_mangle)]`)

Dieselbe Ursache traf das Init-Programm in sechs Dateien. Alle elf Stellen sind
auf die Edition-2024-Schreibweise umgestellt; die Symbole und ihre Namen sind
unverändert, es ist eine Schreibweise und keine Verhaltensänderung.

## Nachweis

Auf dem Nightly-Stand vom 14.11.2026:

- `cargo +nightly build -p shivacore --profile kernel --target
  x86_64-unknown-none --features x86-boot` → 0 Fehler, ELF 519.072 Bytes
- Der rustc-Aufruf des Kernel-Crates trägt `-C opt-level=3 -C panic=abort`
- `nm` auf den ELF findet `rust_begin_unwind` (der eigene Panik-Handler) und
  **kein** `_Unwind_Resume`
- `cargo +nightly check --manifest-path modules/atc-shivacore/init_program/Cargo.toml
  --target x86_64-unknown-none` → sauber
- `cargo test -p shivacore` auf Stable 1.100.0: 781 Tests grün

## Offen

- Der Boot-Builder selbst braucht Nightly (Zwang des `bootloader`-Crates) und
  ist ein eigenes Crate außerhalb des Root-Workspace; er erbt deshalb keine
  Workspace-Lints.
- Ein QEMU-Lauf ist mit diesem Stand nicht belegt. Die Prüfung oben endet beim
  gebauten Image, nicht beim gebooteten System.
