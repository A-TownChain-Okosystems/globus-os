# Doc-Gate: Zaehler und Compiler

Dieses Dokument haelt fest, wie die Dokumentationspflicht der oeffentlichen API
in GlobusOS geprueft wird, was dabei gefunden wurde und was offen ist.

## Zwei Stufen

| Stufe | Werkzeug | Rolle |
|---|---|---|
| Vorpruefung | `scripts/doc_debt.py` + `governance/doc-debt-baseline.json` | schneller Proxy, Ratchet gegen Regressionen, laeuft in `.github/workflows/doc-debt.yml` |
| Gate | `missing_docs = "warn"` in `[workspace.lints.rust]` | Compiler ist die Wahrheit; alle Crates erben die Workspace-Lints |

Die Baseline darf nur sinken. Die Gate-Aussage haengt seit dem 14.11.2026 am
Compiler, nicht am Zaehler.

## Befund D1: der Zaehler stand auf 0, rustdoc nicht

`cargo doc --workspace --no-deps` meldete 40 Warnungen: 36 nicht aufloesbare
Intra-Doc-Links und zwei unfertige HTML-Tags. Ursache waren Byte-Layouts in
Prosa, die rustdoc als Link- oder Tag-Syntax liest, etwa `type[1]`,
`[0x42;32]` und `did:shivacore:<hex-public-key>`. Behoben durch
Code-Auszeichnung ohne Wortlautaenderung in `service_space` (genesis_bridge,
net, p2p, p2p_secure, did) und `system/identity/src/transaction_journal.rs`.

## Befund D2: der Zaehler war zu optimistisch

`missing_docs` meldete 170 Stellen, waehrend der Zaehler 0 meldete. Zwei Fehler
im Zaehler waren die Ursache:

1. Enum-Varianten mit Wert (`PeerList = 0x02,`) wurden nicht erkannt, weil das
   Muster nur `(`, `{` und `,` als Folgezeichen zuliess.
2. Eine `//!`-Moduldoku wurde als Doku des Folge-Items gewertet. `//!`
   dokumentiert das Modul, nicht das Item.

Zusaetzlich zaehlte der Zaehler `pub mod x;`-Deklarationen, deren Doku in
`x.rs` liegt. Alle drei Punkte sind behoben; Zaehler und Compiler melden mit
demselben Bestand beide 0.

## Ausnahme beim Kernel

`[lints] workspace = true` laesst sich in Cargo nicht mit einer Abweichung
kombinieren. Im Kernel stehen die Workspace-Werte deshalb ausgeschrieben, mit
`unsafe_code = "allow"` als begruendete Ausnahme: die Hardwarezugriffe tragen
an jeder Stelle eine Safety-Begruendung, die Warnung wuerde sie nur
wiederholen. `missing_docs` gilt dort unveraendert.

## Offene Punkte

- **Panik-Strategie des Kernel-Builds.** Die `[profile.*]`-Abschnitte im
  Kernel-Manifest wurden von Cargo ignoriert, weil sie in einem
  Nicht-Wurzel-Paket standen; sie sind entfernt. Der Image-Builder
  (`modules/atc-shivacore/boot`) fuehrt kein Profil, dort gilt also der
  Cargo-Standard `unwind`. Eine gewollte `panic = "abort"`-Strategie ist damit
  nirgends gesetzt. Offen, nicht geaendert.
- **Manifest-Warnungen von Cargo.** `unused field repository` in
  `[workspace.package]`, weil kein Mitglied es erbt, und mehrere ungenutzte
  Abhaengigkeiten. Beides ist Metadaten-Hygiene, kein Doku-Inhalt.
- **Flake** `syscall::tests::kernel_input_channel_hands_out_the_next_event`
  (1 bis 2 Fehlschlaege aus 15 bis 20 Laeufen, reproduzierbar seit T43).
- **Aeltere rustfmt-Abweichungen** in mehreren Kernel-Dateien. Die
  normalisierten Diffs sind vor und nach jeder Aenderung zeilengleich belegt;
  ein `cargo fmt --all` wuerde ueber 100 fremde Dateien umformatieren.

## Nachweise

Auf rustc 1.100.0, Stand 14.11.2026:

- `cargo check --workspace`: 0 Fehler, 0 `missing_docs`-Warnungen, keine
  `missing [lints]`-Warnung mehr
- `cargo doc --workspace --no-deps`: keine Warnung zum Doku-Inhalt
- `cargo test --workspace`: 1778 Tests gruen
- `python3 scripts/doc_debt.py --root .`: 0 Items, Baseline 0, keine
  Regression
- Gegenprobe im Frischklon von `origin/main` (Commit `dd92d9b`): Kernel 781
  Tests, `shivacore-service-space` 730, `libshivacore` 36, alle gruen
