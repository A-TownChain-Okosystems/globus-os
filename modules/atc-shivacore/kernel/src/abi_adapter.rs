//! Kernel-side adapters for the stable service-facing ABI.
//!
//! Adapters bind ABI calls to trusted kernel context. They do not expose the
//! concrete kernel data structures through the ABI crate.
use crate::ats1000::Pid;
use crate::capability::{CapId, CapabilityTable};
use crate::kernel_init::{BootPhase, InitStatus, KernelState};
use libshivacore::contracts::{
    CapabilityAuthority, CapabilityCheckRequest, CapabilityCheckResult, KernelBootState,
    KernelBootStatus, KernelBootStatusProvider,
};
use libshivacore::{AbiError, CapabilityRights};

/// Capability authority bound to a caller identity supplied by trusted syscall
/// dispatch, never from untrusted request payload data.
pub struct KernelCapabilityAuthority<'a> {
    table: &'a CapabilityTable,
    caller: Pid,
}

impl<'a> KernelCapabilityAuthority<'a> {
    pub const fn new(table: &'a CapabilityTable, caller: Pid) -> Self {
        Self { table, caller }
    }
}

impl CapabilityAuthority for KernelCapabilityAuthority<'_> {
    fn check(&self, request: CapabilityCheckRequest) -> Result<CapabilityCheckResult, AbiError> {
        const VALID_RIGHTS: u8 = 0x1f;
        let requested = request.required.bits();
        if requested & !VALID_RIGHTS != 0 {
            return Err(AbiError::InvalidPayload);
        }

        let cap = self
            .table
            .get(CapId(request.handle.0))
            .ok_or(AbiError::InvalidHandle)?;

        if cap.owner != self.caller {
            return Err(AbiError::PermissionDenied);
        }

        let effective = cap.rights.bits() & VALID_RIGHTS;
        Ok(CapabilityCheckResult {
            granted: effective & requested == requested,
            effective_rights: CapabilityRights(effective),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{ResourceType, Rights};

    #[test]
    fn adapter_grants_only_owned_capabilities_with_requested_rights() {
        let mut table = CapabilityTable::new();
        let caller = Pid(7);
        let handle = table.create(
            caller,
            ResourceType::Network,
            99,
            Rights::READ | Rights::INSPECT,
        );
        let authority = KernelCapabilityAuthority::new(&table, caller);
        let result = authority
            .check(CapabilityCheckRequest {
                handle: libshivacore::CapabilityHandle(handle.0),
                required: CapabilityRights::READ,
            })
            .unwrap();
        assert!(result.granted);
        assert_eq!(
            result.effective_rights.bits(),
            Rights::READ.bits() | Rights::INSPECT.bits()
        );
    }

    #[test]
    fn adapter_denies_foreign_and_unknown_handles() {
        let mut table = CapabilityTable::new();
        let handle = table.create(Pid(1), ResourceType::Memory, 5, Rights::READ);
        let foreign = KernelCapabilityAuthority::new(&table, Pid(2));
        assert_eq!(
            foreign.check(CapabilityCheckRequest {
                handle: libshivacore::CapabilityHandle(handle.0),
                required: CapabilityRights::READ,
            }),
            Err(AbiError::PermissionDenied)
        );
        assert_eq!(
            foreign.check(CapabilityCheckRequest {
                handle: libshivacore::CapabilityHandle(999),
                required: CapabilityRights::READ,
            }),
            Err(AbiError::InvalidHandle)
        );
    }

    #[test]
    fn adapter_rejects_unknown_right_bits() {
        let table = CapabilityTable::new();
        let authority = KernelCapabilityAuthority::new(&table, Pid(1));
        assert_eq!(
            authority.check(CapabilityCheckRequest {
                handle: libshivacore::CapabilityHandle(1),
                required: CapabilityRights(0x80),
            }),
            Err(AbiError::InvalidPayload)
        );
    }

}

/// Adapter from the legacy ATS-1000 network boundary to the stable ABI.
///
/// This adapter normalizes legacy `Option`/`bool` failures to `AbiError` and
/// enforces the ABI payload ceiling before calling the underlying stack. It
/// does not provide a transport implementation; the supplied stack owns that.
pub struct KernelNetworkAdapter<'a, N: crate::ats1000::NetworkStack> {
    stack: &'a mut N,
    max_message_size: usize,
}

impl<'a, N: crate::ats1000::NetworkStack> KernelNetworkAdapter<'a, N> {
    pub const fn new(stack: &'a mut N) -> Self {
        Self {
            stack,
            max_message_size: libshivacore::MAX_SYSCALL_PAYLOAD,
        }
    }

    /// Test/configuration seam that can only tighten, never widen, the ABI limit.
    pub const fn with_max_message_size(stack: &'a mut N, requested_max: usize) -> Self {
        Self {
            stack,
            max_message_size: if requested_max < libshivacore::MAX_SYSCALL_PAYLOAD {
                requested_max
            } else {
                libshivacore::MAX_SYSCALL_PAYLOAD
            },
        }
    }
}

impl<N: crate::ats1000::NetworkStack> libshivacore::contracts::NetworkStack
    for KernelNetworkAdapter<'_, N>
{
    fn connect(&mut self, peer_node_id: &[u8; 32]) -> Result<u64, AbiError> {
        self.stack
            .connect(peer_node_id)
            .ok_or(AbiError::EndpointUnavailable)
    }

    fn send(&mut self, connection: u64, message: &[u8]) -> Result<(), AbiError> {
        if message.len() > self.max_message_size {
            return Err(AbiError::InvalidPayload);
        }
        if self.stack.send(connection, message) {
            Ok(())
        } else {
            Err(AbiError::EndpointUnavailable)
        }
    }

    fn recv(&mut self, connection: u64, buffer: &mut [u8]) -> Result<usize, AbiError> {
        if buffer.len() > self.max_message_size {
            return Err(AbiError::InvalidPayload);
        }
        let received = self.stack.recv(connection, buffer);
        let received = usize::try_from(received).map_err(|_| AbiError::InvalidPayload)?;
        if received > buffer.len() {
            return Err(AbiError::InvalidPayload);
        }
        Ok(received)
    }
}

#[cfg(test)]
mod network_adapter_tests {
    use super::*;
    use crate::ats1000::NetworkStack as LegacyNetworkStack;

    struct FakeNetwork {
        connected: bool,
        send_ok: bool,
        received: u64,
    }

    impl LegacyNetworkStack for FakeNetwork {
        fn connect(&mut self, _peer_node_id: &[u8; 32]) -> Option<u64> {
            self.connected.then_some(17)
        }

        fn send(&mut self, _conn: u64, _msg: &[u8]) -> bool {
            self.send_ok
        }

        fn recv(&mut self, _conn: u64, buf: &mut [u8]) -> u64 {
            let count = core::cmp::min(self.received as usize, buf.len());
            buf[..count].fill(0x5a);
            self.received
        }
    }

    #[test]
    fn network_adapter_normalizes_legacy_failures() {
        let mut fake = FakeNetwork {
            connected: false,
            send_ok: false,
            received: 0,
        };
        let mut adapter = KernelNetworkAdapter::new(&mut fake);
        assert_eq!(
            libshivacore::contracts::NetworkStack::connect(&mut adapter, &[0; 32]),
            Err(AbiError::EndpointUnavailable)
        );
        assert_eq!(
            libshivacore::contracts::NetworkStack::send(&mut adapter, 1, b"x"),
            Err(AbiError::EndpointUnavailable)
        );
    }

    #[test]
    fn network_adapter_returns_connection_and_received_length() {
        let mut fake = FakeNetwork {
            connected: true,
            send_ok: true,
            received: 3,
        };
        let mut adapter = KernelNetworkAdapter::new(&mut fake);
        assert_eq!(
            libshivacore::contracts::NetworkStack::connect(&mut adapter, &[1; 32]),
            Ok(17)
        );
        assert_eq!(
            libshivacore::contracts::NetworkStack::send(&mut adapter, 17, b"abc"),
            Ok(())
        );
        let mut buffer = [0; 8];
        assert_eq!(
            libshivacore::contracts::NetworkStack::recv(&mut adapter, 17, &mut buffer),
            Ok(3)
        );
        assert_eq!(&buffer[..3], &[0x5a; 3]);
    }

    #[test]
    fn network_adapter_rejects_oversized_payloads_before_dispatch() {
        let mut fake = FakeNetwork {
            connected: true,
            send_ok: true,
            received: 0,
        };
        let mut adapter = KernelNetworkAdapter::with_max_message_size(&mut fake, 2);
        assert_eq!(
            libshivacore::contracts::NetworkStack::send(&mut adapter, 17, b"abc"),
            Err(AbiError::InvalidPayload)
        );
        let mut buffer = [0; 3];
        assert_eq!(
            libshivacore::contracts::NetworkStack::recv(&mut adapter, 17, &mut buffer),
            Err(AbiError::InvalidPayload)
        );
    }

    #[test]
    fn network_adapter_rejects_lying_transport_length() {
        let mut fake = FakeNetwork {
            connected: true,
            send_ok: true,
            received: 99,
        };
        let mut adapter = KernelNetworkAdapter::new(&mut fake);
        let mut buffer = [0; 4];
        assert_eq!(
            libshivacore::contracts::NetworkStack::recv(&mut adapter, 17, &mut buffer),
            Err(AbiError::InvalidPayload)
        );
    }
}


/// Read-only view of the lifecycle log. It cannot start, stop, or mutate boot.
pub struct KernelBootStatusAdapter<'a> {
    state: &'a KernelState,
}

impl<'a> KernelBootStatusAdapter<'a> {
    pub const fn new(state: &'a KernelState) -> Self {
        Self { state }
    }
}

fn abi_boot_phase(phase: BootPhase) -> libshivacore::KernelBootPhase {
    match phase {
        BootPhase::Heap => libshivacore::KernelBootPhase::Heap,
        BootPhase::Memory => libshivacore::KernelBootPhase::Memory,
        BootPhase::Capabilities => libshivacore::KernelBootPhase::Capabilities,
        BootPhase::Processes => libshivacore::KernelBootPhase::Processes,
        BootPhase::Scheduler => libshivacore::KernelBootPhase::Scheduler,
        BootPhase::Ipc => libshivacore::KernelBootPhase::Ipc,
        BootPhase::FileSystem => libshivacore::KernelBootPhase::FileSystem,
        BootPhase::Security => libshivacore::KernelBootPhase::Security,
        BootPhase::Done => libshivacore::KernelBootPhase::Ready,
    }
}

fn abi_boot_state(phase: BootPhase, status: InitStatus) -> KernelBootState {
    match status {
        InitStatus::Failed => KernelBootState::Failed,
        InitStatus::Ready if phase == BootPhase::Done => KernelBootState::Ready,
        InitStatus::NotStarted | InitStatus::Initializing | InitStatus::Ready => {
            KernelBootState::Initializing
        }
    }
}

impl KernelBootStatusProvider for KernelBootStatusAdapter<'_> {
    fn status(&self) -> KernelBootStatus {
        match self.state.init_log.last().copied() {
            Some((phase, state)) => KernelBootStatus {
                phase: abi_boot_phase(phase),
                state: abi_boot_state(phase, state),
                // KernelState::boot returns Err instead of a KernelState on failure,
                // and the legacy init log does not retain a typed BootError.
                error: None,
            },
            None => KernelBootStatus {
                phase: libshivacore::KernelBootPhase::Heap,
                state: KernelBootState::Initializing,
                error: None,
            },
        }
    }
}

#[cfg(test)]
mod boot_status_adapter_tests {
    use super::*;

    #[test]
    fn boot_phase_mapping_preserves_memory_and_done_semantics() {
        assert_eq!(abi_boot_phase(BootPhase::Memory) as u8, 8);
        assert_eq!(
            abi_boot_phase(BootPhase::Done),
            libshivacore::KernelBootPhase::Ready
        );
    }

    #[test]
    fn failed_boot_phase_is_not_reported_as_ready() {
        assert_eq!(
            abi_boot_state(BootPhase::FileSystem, InitStatus::Failed),
            KernelBootState::Failed
        );
        assert_eq!(
            abi_boot_state(BootPhase::Security, InitStatus::Initializing),
            KernelBootState::Initializing
        );
        assert_eq!(
            abi_boot_state(BootPhase::Done, InitStatus::Ready),
            KernelBootState::Ready
        );
    }
}
