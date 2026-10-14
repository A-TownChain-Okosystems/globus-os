# Unified Security Model

## Trust chain
Identity -> Credential -> Capability -> Rights -> Policy -> Resource -> Audit.

## Principal model
User, Process, Thread, Service, Agent, Container, Contract, Validator, Device and System Component are distinct principal classes.

## Capability model
A capability identifies authority over a resource in a namespace. Rights are explicit and must be checked for every privileged operation.
Recommended rights: READ, WRITE, EXECUTE, GRANT, REVOKE, INSPECT, SIGNAL, WAIT, MAP, CONNECT, BIND and ADMIN.

## Handle validation
1. handle exists;
2. object is live;
3. caller owns or has transferred authority;
4. namespace is compatible;
5. resource type matches;
6. requested right is present;
7. capability has not been revoked.

## Revocation
Revocation MUST invalidate descendants or delegated authority according to the capability lineage policy and MUST emit an auditable event.

## IPC
IPC transfer MUST distinguish copy and move semantics. Capability transfer is explicit and cannot be smuggled through ordinary payload data.

## Object lifecycle
Objects use explicit lifecycle states such as Created -> Live -> Closing -> Closed. Waiting operations must define wakeup and cancellation behavior.

## Resource control
CPU, memory, storage, network, GPU, devices, IPC, VM execution, blockchain gas and agent budgets are quota-governed resources.

## Separation of duties
Coder != validator != auditor != release authority for critical release gates.

## Evidence
Security claims require reproducible tests, CI evidence and auditable records. Documentation alone never establishes implementation or production readiness.
