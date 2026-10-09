# ShivaCore Syscall ABI v1.0

Status: contract baseline
ABI: major 1, minor 0
Wire version: 0x0001_0000

## ABI rules
- Syscall IDs are architecture-neutral u16.
- ABI version is u32: high 16 bits major, low 16 bits minor.
- Major mismatch is rejected.
- Unknown syscall IDs are rejected.
- Maximum payload is 1 MiB unless a stricter subsystem limit applies.
- Handles/capabilities are opaque u64 values.
- User pointers and lengths MUST be validated before kernel access.
- Privileged operations require explicit capabilities and rights.
- Errors are stable numeric values.

## Current ABI matrix
| ID | Name | Arguments | Required authority | Blocking | Current code |
|---:|---|---|---|---|---|
| 0x0001 | Yield | none | none | no | implemented |
| 0x0010 | IpcSend | capability, endpoint/resource, payload | IPC + WRITE | no* | boundary implemented |
| 0x0011 | IpcReceive | capability, endpoint/resource, max payload | IPC + READ | no* | boundary implemented |
| 0x0020 | CapabilityQuery | capability | INSPECT | no | implemented |
| 0x0030 | HandleClose | capability/handle | ownership | no | boundary implemented |
| 0x0040 | MonotonicTime | none | none | no | implemented |

*The current dispatcher validates the boundary; full queueing/blocking semantics remain an implementation milestone.

## Reserved ID ranges
| Range | Domain |
|---|---|
| 0x0000-0x000F | scheduler/process control |
| 0x0010-0x001F | IPC |
| 0x0020-0x002F | capabilities |
| 0x0030-0x003F | handles/objects |
| 0x0040-0x004F | time/randomness |
| 0x0050-0x005F | memory |
| 0x0060-0x006F | process/thread |
| 0x0070-0x007F | filesystem/VFS |
| 0x0080-0x008F | networking |
| 0x0090-0x009F | devices |
| 0x00A0-0x00AF | containers/namespaces |
| 0x00B0-0x00BF | runtime/VM |
| 0x00C0-0x00CF | audit/telemetry |

## Canonical syscall contract
Each syscall specification MUST define ID, mnemonic, ABI version, argument encoding, argument types, return type, required capability, required rights, namespace requirements, memory effects, blocking behavior, synchronization, error codes, audit event, quota/resource effects and security invariants.

## Dispatch pipeline
Userspace stub -> ABI decode -> version validation -> syscall ID validation -> argument/pointer validation -> handle validation -> capability/rights validation -> policy -> quota -> kernel subsystem -> audit -> response.

## Stable errors
| Value | Error |
|---:|---|
| 1 | InvalidSyscall |
| 2 | InvalidHandle |
| 3 | PermissionDenied |
| 4 | InvalidPayload |
| 5 | EndpointUnavailable |
| 6 | ResourceExhausted |
| 7 | AbiVersionMismatch |

## Verification requirements
The implementation MUST have tests for unknown IDs, major-version mismatch, oversized payload, invalid handles, wrong owner, wrong resource type, insufficient rights, revoked capability, namespace mismatch, quota exhaustion, concurrent close/transfer, cancellation, malformed user memory, audit emission and restart/recovery where persistent objects are involved.
