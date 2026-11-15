# CI-Forensik: `startup_failure` ohne Jobs (B1, 15.11.2026)

Status: **INVESTIGATED / NICHT BEHOBEN**. Abnahmekriterium (Lauf mit Jobs auf festgeschriebenem HEAD)
bleibt **NOT_MET**. Kein Workflow geaendert, kein Gate abgeschwaecht, kein Run ohne Jobs als Erfolg gezaehlt.

## 1. Ausgangsbefund (unveraendert)

`globus-os`: 100 von 100 der letzten Laeufe `startup_failure`, `jobs: total_count 0`, Logs HTTP 404,
Push-Laeufe mit `path: "BuildFailed"` und leerem Namen. `workflow_dispatch` auf `test-suite.yml`
wird mit 204 angenommen, der Lauf heisst korrekt `ATC Test Suite`, erzeugt aber null Jobs.
Reproduziert 4 von 4 (15.11. 00:58 auf `6beb0cec`, 14.11. auf `2ee2d9fe` und `e70e6aa3`).

## 2. Kontrollierter Trigger-Vergleich (neu)

| Dimension | Vergleich | Ergebnis |
| --- | --- | --- |
| Gleicher Workflow, verschiedener Trigger | `push` gegen `workflow_dispatch` in `globus-os` | beide `startup_failure`, 0 Jobs |
| Gleicher Actor, verschiedener Event | `ShivaCoreDev` mit `pull_request` | **51/51, 54/54, 52/52, 25/25, 30/30, 32/32, 43/43, 18/18 gestartet** (je Repo) |
| Gleicher Actor, verschiedener Event | `ShivaCoreDev` mit `push` | 108x kein Start, 26x Start (alle vor dem Umschlag) |
| Gleicher Actor, verschiedener Event | `ShivaCoreDev` mit `workflow_dispatch` | 6/6 kein Start |
| Commit-Verifizierung | `dependabot[bot]`-Push `verified: true` gegen Agenten-Push `unsigned` | Dependabot startet am 09.11. 18:46 (`atc-sdk`), Agenten-Push am 08.11. 18:35 (`atc-shivacore`) nicht |
| Referenz-Repo | `atc-shivacore`, `atc-standards`, `.github`: `pull_request` und teils `schedule` | starten normal, waehrend `push` dort scheitert |
| Repo-/Org-Richtlinie | `actions/permissions`: `enabled true`, `allowed_actions all`; Repo oeffentlich, nicht `disabled` | als Ursache ausgeschlossen |
| Billing | `billing/usage` 11/2026: 805 Minuten, `netAmount 0.0`, alle Repos oeffentlich | ausgeschlossen |
| Runner / Toolchain | kein Job entsteht, also kein Runner- oder Toolchain-Zugriff | ausgeschlossen |
| Workflow-Definition | Dispatch wird angenommen und korrekt aufgeloest | ausgeschlossen |

**Umschlagpunkt org-weit:** letzter gestarteter `push` am **2026-11-03 12:46** (`atclang`),
erste fehlgeschlagene `push`-Laeufe am **2026-11-06 17:23** (`atc-standards`). Nach dem Umschlag
startet kein benutzerinitiierter Push mehr, wohl aber ein Dependabot-Push mit `verified: true`.

## 3. Hypothesenmatrix

| Hypothese | Status | Begruendung |
| --- | --- | --- |
| Trigger-Identitaet / Actor-Policy | **WIDERLEGT als Alleinursache** | derselbe Actor `ShivaCoreDev` startet 51/51 `pull_request`-Laeufe in genau den Repos, in denen `push` scheitert |
| Commit-Signatur / Verifizierung | **GESTUETZT** | nach dem Umschlag startet nur ein `verified: true`-Push (Dependabot); unverifizierte Commits starten nicht |
| GitHub-interne Initialisierung benutzerinitiierter Events | **BESTE VERBLEIBENDE ERKLAERUNG** | `push` und `workflow_dispatch` (beide vom Benutzer ausgeloest) scheitern, `pull_request` und teils `schedule` (plattformausgeloest) laufen; reproduzierbar ueber 33 Repos |
| Repository- oder Organisationsrichtlinie | nicht gestuetzt | `pull_request` laeuft in denselben Repos unter denselben Rechten |
| Workflow-Definition | widerlegt | Lauf wird benannt und aufgeloest, dann 0 Jobs |

## 4. Was das fuer die Abnahme bedeutet

Ein fehlender Job ist kein gruener Job. `CI_VERIFIED` bleibt unerreichbar, solange kein Lauf Jobs erzeugt.
Die Evidenzbindung an den Code-Commit `c75bb769` bleibt gueltig; diese Datei ist Doku und ersetzt ihn nicht.

## 5. Eskalation an den GitHub Support (Text zum Absenden)

> Repository: A-TownChain-Okosystems/globus-os (public). Account: ShivaCoreDev.
> Since 2026-11-06, every workflow run in this repository completes with `conclusion: startup_failure`
> and zero jobs. `GET /repos/{repo}/actions/runs/{id}/jobs` returns `total_count: 0`, run logs return 404,
> and push-triggered runs report `path: "BuildFailed"` with an empty name.
> `workflow_dispatch` on `.github/workflows/test-suite.yml` is accepted with 204 and creates a correctly
> named run, which also produces zero jobs (run 5721993594, head 6beb0cec, 2026-11-15T00:58Z).
> Actions is enabled (`enabled: true`, `allowed_actions: all`), the repository is public and not disabled,
> billing shows 805 included minutes with 0.00 net usage, and the same organization's `pull_request`-triggered
> runs start normally in the same repositories (e.g. 51/51 in a-townchain-os, 18/18 in atc-standards).
> Org-wide, the last successfully started push run is from 2026-11-03T12:46Z; after 2026-11-06T17:23Z only a
> Dependabot push with a verified commit started. Please investigate why user-initiated push and
> workflow_dispatch events no longer initialize jobs for this account.

## 6. Nicht getan

Keine Workflow-Aenderung, keine Abschwaechung von Gates, keine Signaturpruefung deaktiviert, kein
Zweitkonto zum Erzeugen eines gruenen Laufs benutzt, kein HTTP 204 als CI-Erfolg gewertet.
