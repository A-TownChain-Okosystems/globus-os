#!/usr/bin/env python3
"""Prueft, ob die Governance-Evidenz zum tatsaechlichen Stand passt.

Regel (ATC-STD-REPO-AUDIT-001): kein Status-Claim ohne Evidence-Bundle.
Genau das kippt still, wenn CI nicht laeuft: `bound_commit` bleibt auf einem
alten Commit stehen und die Datei behauptet weiter einen PASS.

Aufruf:
    python3 scripts/check_evidence_binding.py [--commit SHA]

Exit 0 = bindung ok, Exit 1 = veraltet/widerspruechlich, Exit 2 = Datei kaputt.
"""
import argparse
import os
import re
import subprocess
import sys

try:
    import yaml
except ImportError:  # CI-Images ohne PyYAML
    yaml = None

EVIDENCE = os.path.join(".atc", "evidence", "evidence.yaml")

CODE_PATHSPECS = [".", ":!.atc", ":!docs", ":!scripts", ":!*.md", ":!*.txt"]


def sh(cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, check=True).stdout
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None


def is_ancestor(ancestor, descendant):
    try:
        subprocess.run(
            ["git", "merge-base", "--is-ancestor", ancestor, descendant], check=True
        )
        return True
    except (subprocess.CalledProcessError, FileNotFoundError):
        return False


def commits_touching_code_since(bound, head):
    out = sh(["git", "rev-list", "--count", f"{bound}..{head}", "--"] + CODE_PATHSPECS)
    return int(out.strip()) if out and out.strip().isdigit() else 0



def git_head():
    try:
        out = subprocess.run(
            ["git", "rev-parse", "HEAD"], capture_output=True, text=True, check=True
        )
        return out.stdout.strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None


def load_yaml(text):
    if yaml is not None:
        return yaml.safe_load(text)
    data = {}
    for line in text.splitlines():
        m = re.match(r"^([a-z_]+):\s*(.*)$", line)
        if m and m.group(2):
            data[m.group(1)] = m.group(2).strip('"')
    return data


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--commit", help="zu pruefender Commit (Default: git HEAD)")
    args = ap.parse_args()

    if not os.path.exists(EVIDENCE):
        print(f"FEHLT: {EVIDENCE}", file=sys.stderr)
        return 2
    raw = open(EVIDENCE, encoding="utf-8").read()
    try:
        data = load_yaml(raw)
    except Exception as exc:  # noqa: BLE001 - Meldung statt Traceback
        print(f"KAPUTT: {EVIDENCE} ist kein gueltiges YAML: {exc}", file=sys.stderr)
        return 2

    problems = []
    notes = []
    head = args.commit or git_head()
    bound = str(data.get("bound_commit") or "")
    if not bound:
        problems.append("bound_commit fehlt")
    elif head:
        # Veraltet heisst: seit der Bindung sind Code-Commits dazugekommen.
        # Reine Evidenz-/Doku-Commits machen die Bindung nicht falsch, sonst
        # waere die Datei nie mit sich selbst konsistent.
        reachable = sh(["git", "cat-file", "-e", bound + "^{commit}"])
        if reachable is None:
            notes.append(
                f"Shallow-Clone: bound_commit {bound[:10]} nicht aufloesbar, "
                "Bindung nicht pruefbar"
            )
        elif not is_ancestor(bound, head):
            problems.append(
                f"bound_commit {bound[:10]} ist kein Vorfahr von HEAD {head[:10]}"
            )
        else:
            since = commits_touching_code_since(bound, head)
            if since > 0:
                problems.append(
                    f"{since} Code-Commit(s) seit bound_commit {bound[:10]} "
                    "(Evidenz ist veraltet)"
                )

    tests = data.get("tests") or {}
    status = str(tests.get("status") or "")
    test_run = data.get("test_run") or {}
    verif = data.get("verification") or {}
    run_commit = str(test_run.get("commit") or "")
    if status == "pass_with_evidence":
        if not run_commit:
            problems.append("tests.status=pass_with_evidence, aber test_run.commit fehlt")
        elif bound and not (run_commit.startswith(bound) or bound.startswith(run_commit)):
            problems.append(
                f"test_run.commit {run_commit[:10]} != bound_commit {bound[:10]}"
            )
        if not test_run.get("run_id") and not verif.get("results"):
            problems.append(
                "tests.status=pass_with_evidence, aber weder CI-run_id noch "
                "lokale verification.results vorhanden"
            )
    results = verif.get("results") or []
    for entry in results:
        # Artefakt-Nachweise (z. B. Bare-Metal-ELF) haben keine Testzahl und
        # sind trotzdem gueltige Evidenz; nur ausgewiesene Suiten muessen
        # bestandene Tests zeigen.
        if "passed" in entry and int(entry.get("passed") or 0) <= 0:
            problems.append(f"verification-Eintrag ohne bestandene Tests: {entry}")

    if problems:
        print("EVIDENZ NICHT KONFORM:", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1
    for note in notes:
        print(f"Hinweis: {note}", file=sys.stderr)
    print(
        f"EVIDENZ OK: bound_commit {bound[:10]}, tests={status}, "
        f"lokale Suiten={len(results)}, runner={test_run.get('runner')}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
