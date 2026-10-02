# P1 LKM Dependency API Repair

## Finding

The historical finding concerned a placeholder `DependencyGraph::dependencies()` API whose `&[String]` return type was incompatible with the graph's `BTreeSet<String>` storage.

## Current source state

At source SHA `e28a05542993a9be205a4df9bdd4f56137dbf390`, `modules/atc-shivacore/kernel/src/lkm.rs` uses the owned deterministic `dependencies()` representation and contains regression coverage for lexical ordering and missing-module behavior.

The same source snapshot also contains regression coverage for the related LKM findings:
- dependency-first topological ordering;
- required unresolved imports failing closed without optional dependencies;
- exports not being recorded as imports.

## Verification status

- Source implementation: **FIXED / RE-READ**
- Regression tests: **PRESENT IN SOURCE**
- `cargo fmt --all -- --check`: **CI VERIFICATION PENDING**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: **CI VERIFICATION PENDING**
- `cargo test --workspace --all-features`: **CI VERIFICATION PENDING**
- GitHub Actions evidence for source SHA: **PENDING**

This document records the current implementation state; it does not claim CI closure until a workflow run is associated with the exact source SHA.

## Closure rule

Issue #18 must remain open until current GitHub Actions evidence verifies the exact source SHA. Historical audit text is not used as verification evidence.
