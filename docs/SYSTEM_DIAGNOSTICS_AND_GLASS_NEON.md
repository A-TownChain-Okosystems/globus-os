# GlobusOS System Diagnostics & Glass Neon Desktop

**Status:** IMPLEMENTED FOUNDATION
**Scope:** Userspace diagnostics/recovery contract and desktop taskbar presentation model.

## 1. System Diagnostics & Recovery

The canonical diagnostics crate is \`globus-diagnostics\` at \`system/diagnostics/\`. It is a userspace contract and does not move diagnostics policy into ShivaCore.

### Domains

Hardware, Driver, Kernel, Memory, Storage, Network, User, Application, Update, Security, AI Service and Blockchain.

### Pipeline

\`problem -> diagnosis -> cause -> risk/policy -> repair proposal -> authorization -> repair -> verification\`

Diagnosis and repair execution are separate. A repair is successful only after an external capability-authorized executor returns \`RepairResult::Applied\` and the diagnostics engine marks the problem verified.

### Repair authorization

| Level | Meaning |
|---|---|
| 0 | Information |
| 1 | Automatic safe fix |
| 2 | User confirmation |
| 3 | Administrator confirmation |
| 4 | Recovery environment |
| 5 | Emergency recovery |

AI may consume diagnostic data through an explicit API but receives no implicit repair authority.

### Crash Center

\`CrashReport\` captures crash class, timestamp, component, stack trace, kernel/driver versions, hardware, recent changes and logs. History is bounded for deterministic offline operation; persistent evidence storage remains a separate implementation milestone.

### Health

\`HealthSnapshot\` provides per-domain state and a deterministic 0..100 aggregate score. This score is an observability metric and is not a release/readiness claim.

### Core API

- \`DiagnosticsEngine::scan\`
- \`DiagnosticsEngine::health\`
- \`DiagnosticsEngine::problems\`
- \`DiagnosticsEngine::explain\`
- \`DiagnosticsEngine::propose_repair\`
- \`DiagnosticsEngine::verify_repair\`
- \`DiagnosticsEngine::report\`

Rollback and recovery are represented by explicit \`RepairAction\` variants and require the corresponding executor/policy boundary.

## 2. Glass Neon Taskbar

The graphics crate now exposes a renderer-neutral \`TaskbarState\` model in \`system/graphics/src/taskbar.rs\`.

### Modes

- \`Glass\` — maximum transparency.
- \`GlassNeon\` — default futuristic presentation profile.
- \`Hud\` — denser status-oriented presentation.

### Glass layer

\`GlassStyle\` defines opacity, backdrop blur radius, border width, corner radius and reflection strength.

### Dynamic glow

\`GlowState\` covers normal, active app, download, notification, AI work, system problem, critical error, gaming and blockchain states.

### Aurora/ShivaCore activity

\`ActivityState\` supports idle, thinking, processing and generating. Animation remains a renderer concern; the state model is deterministic.

## 3. Architecture

\`\`\`text
ShivaCore
   |
GlobusOS IPC/capability boundary
   +-- Diagnostics
   +-- Recovery
   +-- Desktop / compositor
   |      +-- Glass Neon Taskbar
   +-- Aurora AI
\`\`\`

This keeps diagnostics, recovery, desktop policy and AI outside the ShivaCore TCB.

## 4. Validation

The new modules include unit tests for deterministic health calculation, bounded problem history, repair verification, crash state, task activation, taskbar modes and explicit critical-error glow state.

Repository validation remains:

\`\`\`text
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
\`\`\`

## Authorized repair lifecycle

Repair execution is capability-gated. Diagnostics never performs privileged work directly:

1. diagnostics.propose_repair() creates a typed request.
2. The OS policy layer evaluates RepairAuthorization against the required repair level.
3. RepairPolicy derives the exact capability from the requested action.
4. The injected RepairExecutor performs the privileged operation.
5. Only an Applied result is accepted for verification.
6. The diagnostics engine marks the problem resolved only after the verification boundary succeeds.

AI services may supply diagnosis and repair proposals, but they do not receive implicit repair capabilities.
