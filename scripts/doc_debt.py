#!/usr/bin/env python3
"""Rustdoc-Debt-Ratchet fuer das ATC-Oekosystem.

Zaehlt oeffentliche Items ohne Doc-Kommentar je Crate und vergleicht sie mit der
Baseline. Die Baseline darf nur sinken: jede Erhoehung ist ein Regressionsfehler.

Aufruf:  python3 scripts/doc_debt.py [--base <ref-baseline.json>] [--root <pfad>]
Exit:    0 = Baseline gehalten oder gesenkt, 1 = Regression, 2 = Baseline fehlt
"""
import argparse, json, os, re, sys
from collections import Counter

ITEM = re.compile(r'^\s*pub\s+(?:async\s+)?(?:unsafe\s+)?'
                  r'(?:extern\s+"C"\s+)?(fn|struct|enum|trait|const|static|type|mod|union)\b')
ATTR = re.compile(r'^\s*#\[')
DOC = re.compile(r'^\s*(///|//!|\*|/\*)')
SKIP = ('target', '.git', 'node_modules', 'legacy', 'archive')

def crates(root):
    out = []
    for dp, dn, fn in os.walk(root):
        dn[:] = [d for d in dn if d not in SKIP]
        if 'Cargo.toml' in fn:
            out.append(os.path.normpath(dp))
    return out

root_holder = ['.']

def crate_of(path, known):
    d = os.path.normpath(os.path.dirname(path)); best = None
    for c in known:
        if d == c or d.startswith(c + os.sep):
            if best is None or len(c) > len(best):
                best = c
    res = best or '(kein Cargo.toml)'
    return os.path.relpath(res, root_holder[0]) if res != '(kein Cargo.toml)' else res

def undocumented(path):
    lines = open(path, encoding='utf-8', errors='replace').read().split('\n')
    if any('allow(missing_docs)' in l for l in lines[:40]):
        return 0
    n = 0
    for i, l in enumerate(lines):
        if not ITEM.match(l) or 'allow(missing_docs)' in l:
            continue
        j = i - 1; documented = False
        while j >= 0:
            s = lines[j].strip()
            if s == '':
                j -= 1; continue
            if DOC.match(lines[j]):
                documented = True; break
            if ATTR.match(lines[j]):
                j -= 1; continue
            break
        if not documented:
            n += 1
    return n

def count(root):
    root_holder[0] = root
    known = crates(root); per_crate = Counter()
    for dp, dn, fn in os.walk(root):
        dn[:] = [d for d in dn if d not in SKIP]
        for f in fn:
            if f.endswith('.rs'):
                p = os.path.join(dp, f)
                n = undocumented(p)
                if n:
                    per_crate[crate_of(p, known)] += n
    return per_crate

def main():
    ap = argparse.ArgumentParser()
    here = os.path.dirname(os.path.abspath(__file__))
    ap.add_argument('--root', default=os.path.dirname(here))
    ap.add_argument('--base', default=os.path.join(here, '..', 'governance', 'doc-debt-baseline.json'))
    ap.add_argument('--write-baseline', action='store_true')
    a = ap.parse_args()
    cur = count(a.root)
    if a.write_baseline:
        json.dump({'total': sum(cur.values()), 'per_crate': dict(sorted(cur.items()))},
                  open(a.base, 'w'), indent=2)
        print(f"Baseline geschrieben: {sum(cur.values())} Items in {len(cur)} Crates -> {a.base}")
        return 0
    if not os.path.exists(a.base):
        print(f"Baseline fehlt: {a.base}"); return 2
    base = json.load(open(a.base)); old = base.get('per_crate', {})
    worse = []; better = []
    for c in sorted(set(cur) | set(old)):
        now, was = cur.get(c, 0), old.get(c, 0)
        if now > was: worse.append((c, was, now))
        elif now < was: better.append((c, was, now))
    print(f"Rustdoc-Debt: {sum(cur.values())} Items (Baseline {base.get('total')})")
    for c, was, now in better:
        print(f"  besser  {c}: {was} -> {now}")
    if worse:
        print("  REGRESSION:")
        for c, was, now in worse:
            print(f"    {c}: {was} -> {now} (+{now - was})")
        print("  Neue oeffentliche Items brauchen einen Doc-Kommentar;")
        print("  die Baseline wird nur beim Abarbeiten gesenkt, nie erhoeht.")
        return 1
    print("  keine Regression.")
    return 0

if __name__ == '__main__':
    sys.exit(main())
