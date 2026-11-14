# CI-Status: keine Workflow-Laeufe seit 03.11.2026

Kurzfassung: Workflows starten nicht. Jeder Lauf endet mit `startup_failure`,
auch die von GitHub selbst verwaltete Code-Quality-Pipeline. Das ist kein
Problem der Workflow-Dateien und auch keines der Commits.

## Befund vom 14.11.2026

| Beobachtung | Wert |
|---|---|
| Laeufe seit 03.11.2026 | `startup_failure` (Stichprobe 40 Laeufe, alle) |
| Letzter erfolgreicher Repo-Lauf | 2026-11-03 12:49 UTC, `cargo-audit.yml`, `pull_request`, `6fb337f` |
| Eintrag in der Lauf-Liste | Name leer, Pfad `BuildFailed` |
| Manueller `workflow_dispatch` (`cargo-audit.yml`) | 2026-11-14 17:58 UTC, `startup_failure` |
| CodeQL (GitHub-verwaltet, `dynamic/github-code-quality/codeql`) | `startup_failure` |
| `actions/permissions` | `enabled: true`, `allowed_actions: all` |
| Workflow-Dateien | 11 Dateien, lokal alle parsebar, alle `ubuntu-latest` |

## Was damit ausgeschlossen ist

- **Kein YAML-Fehler:** alle Workflow-Dateien parsen; ein Syntaxfehler würde
  ausserdem nur den einen Workflow treffen, nicht alle und nicht CodeQL.
- **Kein Commit-Problem:** die Laeufe scheitern vor dem ersten Job, also vor
  jedem Checkout. Der Fehler tritt auch bei manuellem Auslösen ohne Push auf.
- **Keine fehlenden Runner:** keine der Dateien verlangt einen selbst
  gehosteten Runner.
- **Keine deaktivierten Actions:** laut API aktiv und ohne Einschränkung.

## Was bleibt

`startup_failure` vor dem ersten Job, bei allen Workflows und bei manuellem
Auslösen, deutet auf eine Bedingung auf Repository- oder Kontoebene. Zwei
Kandidaten sind von hier aus nicht einsehbar und nicht aenderbar:

1. **Abrechnung/Auslastung der Actions-Minuten** (privates Repository). Bei
   erschoepftem Kontingent oder fehlgeschlagener Zahlung startet GitHub keine
   Laeufe und meldet `startup_failure`.
2. **Organisations- oder Repository-Richtlinie** (Ruleset, erforderliche
   Workflows, Beschraenkung auf zulaessige Actions).

Beides ist in der GitHub-Oberflaeche zu pruefen: Repository → Settings →
Actions → General (Zugriff, Kontingent) und Organization → Settings → Billing.

## Folge fuer Aussagen zur Qualitaet

Bis die Laeufe wieder starten, gibt es **kein CI-Evidenzniveau**. Aussagen zu
Tests und Builds stuetzen sich auf lokale Laeufe und auf Gegenproben in einem
Frischklon, nicht auf die Pipeline. `CI_VERIFIED` wird nicht vergeben, solange
kein Lauf auf dem Head-Commit durchgelaufen ist.
