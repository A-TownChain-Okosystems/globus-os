#!/usr/bin/env python3
"""Rustdoc-Debt-Ratchet fuer das ATC-Oekosystem (Version 2).

Zaehlt oeffentliche API ohne Doc-Kommentar je Crate:
  - Top-Level-Items (fn, struct, enum, trait, const, static, type, mod, union)
  - oeffentliche Felder in oeffentlichen Structs
  - Varianten oeffentlicher Enums
  - Items oeffentlicher Traits

Damit deckt der Zaehler die Flaechen ab, die `missing_docs` tatsaechlich
meldet (Makro-generierte Items ausgenommen). Er bleibt ein Proxy, weil
Proc-Makros und `#[doc(hidden)]` nicht ausgewertet werden.

Die Baseline darf nur sinken: jede Erhoehung ist ein Regressionsfehler.
Exit: 0 = Baseline gehalten oder gesenkt, 1 = Regression, 2 = Baseline fehlt
"""
import argparse, json, os, re, sys
from collections import Counter

SKIP = ('target', '.git', 'node_modules', 'legacy', 'archive')

ITEM = re.compile(r'^(\s*)pub\s+(?:async\s+)?(?:unsafe\s+)?(?:extern\s+"C"\s+)?'
                  r'(fn|struct|enum|trait|const|static|type|mod|union)\b')
FIELD = re.compile(r'^(\s*)pub\s+[A-Za-z_][A-Za-z0-9_]*\s*:')
VARIANT = re.compile(r'^(\s*)([A-Z][A-Za-z0-9_]*)\s*(?:[({,])')
TRAIT_ITEM = re.compile(r'^(\s*)(?:fn|type|const)\s+[A-Za-z_]')
ATTR = re.compile(r'^\s*#\[')
DOC = re.compile(r'^\s*(///|//!|/\*\*|\*)')
BLANK = re.compile(r'^\s*$')

root_holder = ['.']

def documented(lines, i):
    """Prueft rueckwaerts, ob das Item bei Zeile i einen Doc-Kommentar hat."""
    j = i - 1
    while j >= 0:
        if BLANK.match(lines[j]):
            j -= 1; continue
        if DOC.match(lines[j]):
            return True
        if ATTR.match(lines[j]):
            j -= 1; continue
        return False
    return False

def scan_file(path):
    lines = open(path, encoding='utf-8', errors='replace').read().split('\n')
    if any('allow(missing_docs)' in l for l in lines[:40]):
        return 0
    n = 0
    stack = []           # (kind, indent) offener oeffentlicher Container
    for i, l in enumerate(lines):
        if 'allow(missing_docs)' in l or not l.strip():
            continue
        indent = len(l) - len(l.lstrip())
        while stack and indent <= stack[-1][1]:
            stack.pop()
        if l.strip().startswith('}'):
            continue
        m = ITEM.match(l)
        if m:
            kind = m.group(2)
            if not documented(lines, i):
                n += 1
            if kind in ('struct', 'enum', 'trait') and '{' in l:
                stack.append((kind, indent))
            continue
        if not stack:
            continue
        kind, _base = stack[-1]
        if kind == 'struct' and FIELD.match(l) and not documented(lines, i):
            n += 1
        elif kind == 'enum' and VARIANT.match(l) and not documented(lines, i):
            n += 1
        elif kind == 'trait' and TRAIT_ITEM.match(l) and not documented(lines, i):
            n += 1
    return n

def crates(root):
    out = []
    for dp, dn, fn in os.walk(root):
        dn[:] = [d for d in dn if d not in SKIP]
        if 'Cargo.toml' in fn:
            out.append(os.path.normpath(dp))
    return out

def crate_of(path, known, root):
    d = os.path.normpath(os.path.dirname(path)); best = None
    for c in known:
        if d == c or d.startswith(c + os.sep):
            if best is None or len(c) > len(best):
                best = c
    if best is None:
        return '(kein Cargo.toml)'
    return os.path.relpath(best, root).replace(os.sep, '/')

def count(root):
    known = crates(root); per_crate = Counter(); per_file = {}
    for dp, dn, fn in os.walk(root):
        dn[:] = [d for d in dn if d not in SKIP]
        for f in fn:
            if not f.endswith('.rs'):
                continue
            p = os.path.join(dp, f)
            n = scan_file(p)
            if n:
                per_file[p] = n
                per_crate[crate_of(p, known, root)] += n
    return per_crate, per_file

def main():
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser()
    ap.add_argument('--root', default=os.path.dirname(here) or '.')
    ap.add_argument('--base', default=os.path.join(here, '..', 'governance', 'doc-debt-baseline.json'))
    ap.add_argument('--write-baseline', action='store_true')
    ap.add_argument('--detail', action='store_true')
    a = ap.parse_args()
    cur, per_file = count(a.root)
    if a.write_baseline:
        json.dump({'total': sum(cur.values()), 'per_crate': dict(sorted(cur.items()))},
                  open(a.base, 'w'), indent=2)
        print(f"Baseline: {sum(cur.values())} Items in {len(cur)} Crates -> {a.base}")
        return 0
    if not os.path.exists(a.base):
        print(f"Baseline fehlt: {a.base}"); return 2
    base = json.load(open(a.base)); old = base.get('per_crate', {})
    worse = [(c, old.get(c, 0), cur.get(c, 0)) for c in sorted(set(cur) | set(old))
             if cur.get(c, 0) > old.get(c, 0)]
    better = [(c, old.get(c, 0), cur.get(c, 0)) for c in sorted(set(cur) | set(old))
              if cur.get(c, 0) < old.get(c, 0)]
    print(f"Rustdoc-Debt: {sum(cur.values())} Items (Baseline {base.get('total')})")
    for c, was, now in better:
        print(f"  besser  {c}: {was} -> {now}")
    if a.detail:
        for p, n in sorted(per_file.items(), key=lambda x: -x[1])[:10]:
            print(f"  {n:>4}  {p}")
    if worse:
        print("  REGRESSION:")
        for c, was, now in worse:
            print(f"    {c}: {was} -> {now} (+{now - was})")
        print("  Neue oeffentliche API braucht einen Doc-Kommentar;")
        print("  die Baseline wird nur beim Abarbeiten gesenkt, nie erhoeht.")
        return 1
    print("  keine Regression.")
    return 0

if __name__ == '__main__':
    sys.exit(main())
