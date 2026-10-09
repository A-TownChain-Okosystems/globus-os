# GlobusOS / ShivaCore Reference Architecture v3

Status: integration baseline / implementation contract
Version: 1.0.0
Date: 2026-09-22

## 1. Purpose
This document consolidates the architecture established for GlobusOS, ShivaCore, ATC-VM, ATCLang, Aurora AI and the A-TownChain platform. It is an architectural contract, not a claim that every subsystem is production-ready.

## 2. Vertical domains
| Domain | Responsibility |
|---|---|
| D1 Experience | Aurora, desktop, shell, UI, CLI, applications |
| D2 Application | OS applications, wallet, developer tools, Genesis-facing applications |
| D3 Platform | APIs, SDKs, services, middleware, identity, policy, governance |
| D4 Execution | ATC-VM, ATCLang runtime, WASM, agents, containers, plugins |
| D5 System | Syscall ABI, ShivaCore kernel, HAL, drivers, hardware |

## 3. Horizontal control planes
Security, Identity, Policy, Governance, Audit, Observability/Telemetry, Resource/Quota and Configuration apply across all domains.

## 4. Contract boundaries
1. Experience -> Application
2. Application -> Platform
3. Platform -> Runtime
4. Runtime -> Host ABI
5. Host ABI -> Syscall ABI
6. Syscall ABI -> ShivaCore kernel
7. Kernel -> HAL/Drivers
8. HAL/Drivers -> Hardware

The kernel MUST NOT absorb application, AI or blockchain semantics.

## 5. Runtime boundary
ATCLang -> compiler -> ATC bytecode -> verifier -> ATC-VM -> Host ABI -> ShivaCore.
The VM is the explicit boundary between deterministic chain execution and the Rust/system track.

## 6. Blockchain boundary
Wallet/SDK -> API -> transaction builder -> transaction -> mempool -> proposer -> block -> validation -> consensus/finality -> state transition -> ATC-VM -> state commit -> durable persistence -> restart recovery.
A production-readiness claim requires CI evidence for the complete path.

## 7. AI boundary
Model -> inference engine -> agent runtime -> planner -> tool invocation -> policy -> capability request -> Host ABI/syscall -> ShivaCore.
Aurora AI is outside the kernel TCB.

## 8. Unified identity model
Principal -> Identity -> Credentials -> Capabilities -> Rights -> Policy.
Principal classes include User, Process, Thread, Service, Agent, Container, Contract, Validator, Device and System Component.

## 9. Unified resource model
Every controlled resource is modeled by Resource ID, Owner, Namespace, Capability, Quota, Usage, Limit, Policy, Lifecycle and Audit record.
Resource classes include CPU, memory, storage, network, GPU, device, IPC, VM execution, blockchain gas and agent budget.

## 10. Privileged operation pipeline
Request -> caller identity -> namespace -> handle validation -> capability validation -> rights -> policy -> quota -> resource operation -> audit event.
Failure MUST be fail-closed.

## 11. Container model
Container = namespace set + capability set + resource domain + VFS namespace + network namespace + process tree + device policy + security policy.
Containers are not a bypass around ShivaCore authorization.

## 12. IPC primitives
IPC_SEND, IPC_RECEIVE, IPC_CALL, IPC_REPLY, IPC_NOTIFY, IPC_SIGNAL, IPC_SHARE_MEMORY, IPC_TRANSFER_CAPABILITY, IPC_CANCEL and IPC_CLOSE.
Only operations backed by a tested kernel implementation are considered implemented.

## 13. Security invariants
- No ambient authority.
- Capability ownership is explicit.
- Rights are checked before resource access.
- User pointers and lengths are validated before dereference.
- Namespace boundaries are enforced.
- Quotas prevent uncontrolled resource consumption.
- Revocation is explicit and auditable.
- Kernel TCB excludes AI and blockchain application semantics.
- Audit records carry actor, subject, resource, action, result, policy decision and correlation identity.

## 14. Repository ownership
- globus-os: canonical integrated OS implementation and active ShivaCore kernel tree.
- atc-shivacore: reusable ShivaCore contracts, specifications and supporting material.
- atc-standards: normative governance and standards SSOT; standards changes require SCR.
- a-townchain-ecosystem: integration/release orchestrator; it does not replace source-repository ownership.
- a-townchain-os-docs: documentation hub and historical/wiki material.

## 15. Readiness rule
APPROVED != IMPLEMENTED != AUDITED != PRODUCTION_READY.
Each state requires its own evidence.

## 16. Required implementation sequence
1. Freeze ABI identifiers and error semantics.
2. Implement syscall dispatcher and capability checks.
3. Implement IPC/handle/object lifecycle semantics.
4. Implement memory, process/thread and resource/quota contracts.
5. Connect Host ABI for ATC-VM/WASM/agents/containers.
6. Connect identity/policy/audit planes.
7. Add integration tests and fault/race/stress tests.
8. Run CI and record evidence.
