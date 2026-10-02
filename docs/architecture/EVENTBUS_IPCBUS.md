# EventBus / IPCBus Architecture Contract

Status: IMPLEMENTED — exact-SHA CI verification pending.

## Decision

GlobusOS uses a strict hybrid boundary.

- EventBus (P1): bounded, in-process publish/subscribe for decoupled service events.
- IPCBus (P0): capability-scoped message transport for process and service boundaries.
- EventBus is not an authorization mechanism and cannot establish canonical state or finality.
- IPCBus is not a consensus mechanism and cannot establish blockchain finality.
- Cross-process event delivery must use IPCBus or another explicitly authorized transport.

## P0 — IPCBus

Every IPC operation resolves:

principal -> capability -> operation -> endpoint/resource -> policy -> dispatch

Required properties:

1. Endpoint access is explicit.
2. Send and receive require explicit credentials.
3. Protocol version is validated.
4. Payload size and queue capacity are bounded.
5. FIFO delivery is deterministic per endpoint.
6. Invalid payload metadata is rejected.
7. Unknown or mismatched access fails closed.
8. IPC failure cannot mutate unrelated canonical state.

Implementation SSOT:
system/ipc

## P1 — EventBus

Every event contains:

- stable event ID;
- event type;
- semantic schema version;
- source and domain;
- correlation/causation metadata;
- monotonic local sequence;
- caller-supplied timestamp;
- payload;
- authoritative/derived classification.

Required properties:

1. Subscriber queues are bounded.
2. Publication order is deterministic.
3. Backpressure is explicit; events are never silently dropped.
4. History is bounded.
5. Unsubscribe is explicit.
6. EventBus never authorizes privileged operations.
7. EventBus never establishes finality or canonical state ownership.
8. Replay is possible from bounded history without wall-clock dependence.

Implementation SSOT:
system/event-bus

## Existing-first audit

At source SHA e28a05542993a9be205a4df9bdd4f56137dbf390, globus-os already contained a bounded ChannelRegistry, Endpoint, Message and payload validation in system/ipc. That implementation is reused.

The existing repository did not contain a current standalone EventBus crate. Earlier EventBus and IPCBus material was found in documentation/archive content and is not treated as the Rust implementation SSOT.

The new runtime surfaces therefore extend system/ipc and add system/event-bus rather than copying legacy documentation code into the ecosystem repository.

## Verification

This contract records implementation intent and existing-first evidence. It does not claim CI PASS until an exact commit SHA has completed the relevant GitHub Actions jobs.
