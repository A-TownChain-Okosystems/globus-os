# CI-Forensik: `startup_failure` ohne Jobs (B1, 15.11.2026)

Status: **BLOCKED**, Ursache eingegrenzt, Abhilfe nur durch den Owner.
Kein Workflow wurde geaendert, kein Gate abgeschwaecht.

## Befund

| Frage | Antwort | Beleg |
| --- | --- | --- |
| Welcher Workflow? | jeder im Repo, nicht ein einzelner | 100 von 100 der letzten Laeufe `startup_failure`, darunter `ATC Test Suite`, `Rust CI`, CodeQL und die synthetischen Push-Laeufe |
| Welcher Job? | keiner | `GET /actions/runs/{id}/jobs` → `total_count: 0` |
| Welcher Step? | keiner, es gibt keine Logs | `GET /actions/runs/{id}/logs` → HTTP 404 |
| Runner? | ausgeschlossen | GitHub-hosted, andere Repos derselben Organisation starten im selben Zeitraum |
| Toolchain? | ausgeschlossen | kein Job, damit kein Toolchain-Zugriff; Fehler liegt vor dem Job-Start |
| Environment/Repo-Setting? | ausgeschlossen | `actions/permissions`: `enabled: true`, `allowed_actions: all`, `sha_pinning_required: false`; Repo oeffentlich, nicht archiviert, nicht `disabled` |
| Reproduzierbar? | ja, 4 von 4 | `workflow_dispatch` auf `test-suite.yml` am 15.11. 00:58 UTC auf `6beb0cec`: HTTP 204 accepted, Lauf angelegt, `conclusion: startup_failure`, Jobs 0. Ebenso 14.11. 21:24 auf `2ee2d9fe` und 17:58 auf `e70e6aa3` |

## Eingrenzung

- Push-Laeufe tragen `path: "BuildFailed"` und einen leeren Namen: der Lauf entsteht, bevor ein Workflow aufgeloest wird.
- Ein `workflow_dispatch` auf `test-suite.yml` wird angenommen und korrekt benannt (`ATC Test Suite`, `path: .github/workflows/test-suite.yml`), startet aber ebenfalls keine Jobs. Die Workflow-Datei ist also nicht die Ursache.
- Org-weiter Vergleich (15 Laeufe je Repo): `push` 76x `success`, `push` 84x `startup_failure`, `schedule` 50x `success`, `dynamic` 107x `success`.
- Die Trennlinie ist die Identitaet des Ausloesers, nicht das Repo: Am 09.11. startet ein `push` auf `atc-sdk` mit Dependabot-Commit (`verification.verified: true`) erfolgreich; am 08.11. scheitern `atc-node` und `atc-shivacore` mit unverifizierten Commits. Am 02.11. starten unverifizierte Commits (`atc-contracts`, `atc-engineering`) noch.
- Das ausloesende Konto `ShivaCoreDev` liefert `email: null` (unbestaetigt); alle Commits dieses Agenten sind `verified: false`, `reason: unsigned`.
- Billing ist ausgeschlossen: alle betroffenen Repos sind oeffentlich, `billing/usage` 11/2026 zeigt 805 Actions-Minuten mit `netAmount 0.0`.

## Schlussfolgerung

Der Fehler sitzt bei der Erzeugung des Laufs, nicht in Job, Step, Runner, Toolchain oder Repo-Konfiguration.
Er trifft jeden Lauf, den das Konto `ShivaCoreDev` ausloest, unabhaengig vom Trigger und vom Repo-Inhalt; Laeufe
einer verifizierten Identitaet starten im selben Org im selben Zeitraum.

## Abhilfe (Owner, keine Code-Aenderung)

1. E-Mail des Kontos `ShivaCoreDev` unter `github.com/settings/emails` bestaetigen. Danach erneut einen Lauf auf dem
   aktuellen HEAD ausloesen. Erwartung: Jobs entstehen, damit ist die Diagnose zugleich widerlegt oder bestaetigt.
2. Falls die Bestaetigung nicht ausreicht: GitHub Support mit dieser Datei als Fallbeschreibung (Konto, Repo,
   Run-IDs) oder Laeufe ueber eine verifizierte Identitaet ausloesen.

## Nicht getan

Keine Workflow-Aenderung, kein Abschalten von Gates, keine lokale Erfolgsmeldung als CI-Ersatz.
`CI_VERIFIED` bleibt unerreicht, solange kein Lauf auf einem festgeschriebenen HEAD Jobs erzeugt.
