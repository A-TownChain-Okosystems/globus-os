# Globus OS Diagnostics & Glass Neon Desktop

## Diagnostics

globus-diagnostics is an offline-first diagnostics contract shared by kernel-adjacent services and userspace. It provides deterministic metric validation, problem classification, crash-report retention, event logging, health scoring, and explicit recovery levels.

Security-sensitive repair actions require the corresponding confirmation level. The diagnostics engine does not execute privileged repair operations itself.

Primary API surface:
- DiagnosticsEngine::scan
- DiagnosticsEngine::add_problem
- DiagnosticsEngine::record_crash
- DiagnosticsEngine::health_score
- DiagnosticsEngine::health_state

## Desktop taskbar

The graphics shell exposes deterministic visual-state contracts for a floating glass/neon taskbar:
- Glass, Glass Neon, and HUD modes.
- Glass opacity, blur, border, glow, corner radius, reflection and parallax parameters.
- App, download, notification, AI, system-problem, critical, gaming and blockchain glow states.
- ShivaCore-compatible activity states: Idle, Thinking, Processing, Generating.

Rendering remains the responsibility of the compositor/UI layer; these contracts do not embed GPU-specific behavior or security-sensitive authority.