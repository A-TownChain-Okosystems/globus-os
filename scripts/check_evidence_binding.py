#!/usr/bin/env python3
"""Prueft, ob die Governance-Evidenz zum tatsaechlichen Stand passt.

Regel (ATC-STD-REPO-AUDIT-001): kein Status-Claim ohne Evidence-Bundle.
Genau das kippt still, wenn CI nicht laeuft: `bound_commit` bleibt auf einem
alten Commit stehen und die Datei behauptet weiter einen PASS.

Aufruf:
    python3 scripts/check_evidence_binding.py [--commit SHA]

Exit 0 = bindung ok, Exit 1 = veraltet/widerspruechlich, Exit 2 = Datei kaputt.

Zusaetzlich geprueft (15.11.2026):
  - das Dateiregister bildet den Baum an HEAD ab (Mengengleichheit), und
  - jedes Audit in docs/audits/ mit `audit-basis: <sha>` nennt einen aufloesbaren
    Vorfahren von HEAD. Der aufnehmende Commit ist nicht der untersuchte Stand.
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


AUDIT_DIR = os.path.join("docs", "audits")
REGISTER = "FILE_REGISTER.md"
AUDIT_BASIS_RE = re.compile(r"<!--\s*audit-basis:\s*([0-9a-f]{7,40})\s*-->")


def tracked_files(rev):
    """Alle Dateien des Baums an `rev`, wie sie committet sind."""
    out = sh(["git", "ls-tree", "-r", "--name-only", rev])
    if out is None:
        return None
    return {line for line in out.splitlines() if line}


def register_entries():
    """Pfade aus dem Dateiregister (Tabellenzeilen mit Pfad in Backticks)."""
    if not os.path.exists(REGISTER):
        return None
    entries = set()
    for line in open(REGISTER, encoding="utf-8").read().splitlines():
        m = re.match(r"^\|\s*`([^`]+)`\s*\|", line)
        if m:
            entries.add(m.group(1))
    return entries


def check_register(rev):
    """Das Register muss den Baum abbilden, sonst zaehlt niemand mehr mit.

    Ein Register, das eine Datei nicht nennt, ist keine Zaehlung; eine Zeile
    ohne Datei ist eine Behauptung ueber etwas, das nicht existiert.
    """
    if rev is None:
        return [], ["Register nicht pruefbar (kein HEAD)"]
    tracked = tracked_files(rev)
    entries = register_entries()
    if tracked is None:
        return [], ["git ls-tree fehlgeschlagen, Register nicht pruefbar"]
    if entries is None:
        return [f"{REGISTER} fehlt"], []
    problems = []
    missing = sorted(tracked - entries)
    extra = sorted(entries - tracked)
    if missing:
        problems.append(
            f"Register unvollstaendig: {len(missing)} Datei(en) im Baum ohne Zeile, "
            f"z. B. {missing[0]}"
        )
    if extra:
        problems.append(
            f"Register nennt {len(extra)} Pfad(e) ohne Baum-Eintrag, z. B. {extra[0]}"
        )
    return problems, []


def check_audit_basis(rev):
    """Ein Audit nennt den untersuchten Quellstand, nicht sich selbst.

    `audit-basis` muss ein aufloesbarer Vorfahr von HEAD sein. Damit kann ein
    Audit-Dokument nicht stillschweigend behaupten, der Commit, der es aufnimmt,
    sei der untersuchte Code-Stand.
    """
    problems = []
    if rev is None or not os.path.isdir(AUDIT_DIR):
        return problems
    for name in sorted(os.listdir(AUDIT_DIR)):
        if not name.endswith(".md"):
            continue
        path = os.path.join(AUDIT_DIR, name)
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        m = AUDIT_BASIS_RE.search(text)
        if not m:
            continue
        basis = m.group(1)
        if sh(["git", "cat-file", "-e", basis + "^{commit}"]) is None:
            problems.append(f"{path}: audit-basis {basis[:10]} ist nicht aufloesbar")
        elif not is_ancestor(basis, rev):
            problems.append(
                f"{path}: audit-basis {basis[:10]} ist kein Vorfahr von HEAD {rev[:10]}"
            )
    return problems


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

    register_problems, register_notes = check_register(head)
    problems.extend(register_problems)
    notes.extend(register_notes)
    problems.extend(check_audit_basis(head))

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
