# Runtime Host ABI

The Host ABI is the boundary used by ATC-VM, WASM, agents, plugins and container runtimes to request privileged platform services.

Runtime code MUST NOT call kernel internals directly.

Runtime -> Host ABI -> Syscall ABI -> ShivaCore.

## Host ABI domains
memory, time, randomness, IPC, storage, networking, identity, capabilities, audit, resource/quota, VM execution and device access through explicitly authorized services.

## Security
A Host ABI operation maps to a concrete capability and syscall contract. Runtime type or contract identity never implies authority.

## ATC-VM boundary
ATCLang -> compiler -> ATC bytecode -> verifier -> ATC-VM -> Host ABI.
The verifier guarantees bytecode validity and deterministic execution rules. The Host ABI guarantees that privileged effects remain outside the VM and are capability/policy controlled.

## Determinism
Consensus-critical ATC-VM execution MUST use deterministic Host ABI calls. Wall-clock time, nondeterministic randomness, local filesystem state and ambient network access are forbidden unless explicitly modeled as deterministic protocol inputs.

## Agents
Aurora agents use the same Host ABI security boundary as other runtimes. Agent intent, model output or tool selection MUST NOT grant capabilities implicitly.
