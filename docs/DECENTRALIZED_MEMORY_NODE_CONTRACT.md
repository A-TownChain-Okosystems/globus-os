# GlobusOS Decentralized Memory Node Contract

Status: specification / documented architecture baseline
Lifecycle: DEVELOPMENT / NOT_READY
Base: e28a05542993a9be205a4df9bdd4f56137dbf390
Date: 2026-09-27

## Purpose

GlobusOS is the policy and security boundary for device-local and federated Aurora memory. PCs, smartphones and other supported devices may become memory nodes only when explicitly registered and authorized.

A device joining GlobusOS or running Aurora does not automatically expose its private data to the federation.

## Security boundary

```text
Device / Hardware
      |
ShivaCore TCB
      |
Identity + Capabilities
      |
GlobusOS Memory Service
      |
Policy / Consent / Audit
      |
Aurora Memory Federation
      |
Authorized Peer Nodes
```

Required invariant:

request -> identity -> capability -> policy -> validation -> state change -> audit

## Device memory domains

Target domains:
- device state;
- personal memory;
- conversation memory;
- knowledge/RAG data;
- Genesis/game state;
- OS state;
- evidence/provenance;
- explicitly shared memory.

Sensitive credentials, private keys, wallet recovery material and other protected secrets remain behind their existing security boundaries and are not exposed as ordinary Aurora memory.

## Node lifecycle

```text
UNREGISTERED
   -> REGISTERED
   -> AUTHENTICATED
   -> POLICY_APPROVED
   -> MEMORY_ENABLED
   -> FEDERATION_ENABLED
   -> REVOKED / DISABLED
```

Federation enablement is a separate state from device registration.

## Memory API boundary

GlobusOS should expose a narrow service boundary rather than direct filesystem/kernel access:

```text
Aurora / Application
        -> Memory Request
        -> GlobusOS Identity
        -> Capability
        -> Policy / Consent
        -> Memory Service
        -> Local Store or Authorized Peer
        -> Audit
```

The UI, Aurora agent runtime and ordinary applications must not directly modify authoritative memory storage without passing the service boundary.

## Federation permissions

At minimum distinguish:
- MEMORY_READ_LOCAL
- MEMORY_WRITE_LOCAL
- MEMORY_READ_SHARED
- MEMORY_WRITE_SHARED
- MEMORY_EXPORT
- MEMORY_IMPORT
- MEMORY_REVOKE
- MEMORY_DELETE
- MEMORY_AUDIT

Capabilities must be scope-limited by owner, memory domain, operation and peer/node.

## Provenance

Every federated record must retain enough metadata to determine:
- memory identity;
- owner;
- source node;
- creation/update version;
- source/provenance;
- integrity hash;
- access policy;
- federation authorization;
- audit trail.

## Offline operation

Devices may operate without a federation connection. Local memory remains local until an explicit synchronization policy permits exchange.

Synchronization must preserve versions and provenance. Conflicts must be surfaced to a deterministic resolver rather than silently overwritten.

## A-TownChain boundary

A-TownChain is an optional integrity/provenance anchor, not the default bulk-memory store.

```text
Memory payload -> local/federated storage
Memory hash    -> provenance
Optional proof -> A-TownChain
```

## Current evidence

The GlobusOS repository already defines identity, capability, IPC, settings, storage and wallet boundaries at the architecture level. This document adds the missing decentralized-memory node contract.

This document does not claim that a production memory service, mobile client, federation transport, cross-device synchronization or E2E flow is implemented.

Current status remains:

SPECIFIED -> DOCUMENTED

Required next gates:
1. Canonical memory schema and serialization.
2. GlobusOS memory-service API.
3. Capability identifiers and policy rules.
4. Local persistence implementation.
5. Federation transport and node authentication.
6. Version/conflict resolver.
7. Revocation/deletion implementation.
8. Exact-SHA unit/integration CI.
9. Cross-device E2E.
10. Security audit.