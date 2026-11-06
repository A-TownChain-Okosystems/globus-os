// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! Stable ShivaCore syscall boundary.
//!
//! The kernel owns dispatch and capability validation. Userspace only sees the
//! versioned libshivacore ABI; unknown syscall IDs and malformed requests
//! fail closed before any kernel operation is reached.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::ats1000::Pid;

/// Kanal-ID des Kernel-Eingabekanals. Der Wert ist ein Default, kein
/// ABI-Zusatz: `IpcReceive` mit dieser ID und READ-Recht liefert das naechste
/// Tastenereignis im Rueckgabewert.
pub const KERNEL_INPUT_CHANNEL: u64 = 1;
use crate::capability::{CapId, CapabilityTable, ResourceType, Rights};
use libshivacore::{
    validate_abi_version, AbiError, CapabilityHandle, Syscall, MAX_SYSCALL_PAYLOAD,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyscallRequest {
    pub abi_version: u32,
    pub syscall_id: u16,
    pub capability: Option<CapabilityHandle>,
    pub arg0: u64,
    pub arg1: u64,
    /// Zeiger auf die Nutzdaten im Userspace-Speicher (0 = keine).
    pub buffer_ptr: u64,
    /// Laenge der Nutzdaten. ABI 2.0.0 trennt Zeiger und Laenge; in 1.x gab es
    /// nur eine Laenge, die sich ein Register mit einem Argumentwort teilte.
    pub buffer_len: usize,
}

impl SyscallRequest {
    /// Uebersetzt einen Argumentblock in die interne Anfrage. Struktur und
    /// Version sind zu diesem Zeitpunkt bereits geprueft.
    pub fn from_block(block: &libshivacore::SyscallBlock) -> Self {
        Self {
            abi_version: libshivacore::ABI_VERSION,
            syscall_id: block.opcode.id(),
            capability: block.capability,
            arg0: block.arg0,
            arg1: block.arg1,
            buffer_ptr: block.buffer_ptr,
            buffer_len: block.buffer_len,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyscallResponse {
    pub error: Option<AbiError>,
    pub value: u64,
}

impl SyscallResponse {
    const fn ok(value: u64) -> Self {
        Self { error: None, value }
    }
    const fn err(error: AbiError) -> Self {
        Self {
            error: Some(error),
            value: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchError {
    Abi(AbiError),
}

impl From<AbiError> for DispatchError {
    fn from(error: AbiError) -> Self {
        Self::Abi(error)
    }
}

/// Einzige Zeitquelle des Systems: ein Hardware-Tick ist ein Schritt.
/// Bewusst lock-frei — ein belegtes Gate darf keinen Tick verschlucken, sonst
/// verliert `MonotonicTime` Zeit, sobald Syscalls laufen.
static MONOTONIC_TICKS: AtomicU64 = AtomicU64::new(0);

/// Vom Timer-Interrupt aufzurufen. Ohne Sperre, ohne Bedingung.
pub fn advance_monotonic_tick() {
    MONOTONIC_TICKS.fetch_add(1, Ordering::Relaxed);
}

/// Aktueller Tick-Stand.
pub fn monotonic_ticks() -> u64 {
    MONOTONIC_TICKS.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyscallDispatcher {}

impl Default for SyscallDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl SyscallDispatcher {
    pub const fn new() -> Self {
        Self {}
    }

    /// Vom Timer-Interrupt aufzurufen: genau ein Tick pro Hardware-IRQ.
    /// Die Zeit fortschreiben darf nur die Hardware, nicht der Leser.
    pub fn on_tick(&mut self) {
        advance_monotonic_tick();
    }

    /// Aktueller Tick-Stand (fuer Diagnose und Tests).
    pub fn ticks(&self) -> u64 {
        monotonic_ticks()
    }

    pub fn dispatch(
        &mut self,
        pid: Pid,
        request: SyscallRequest,
        capabilities: &mut CapabilityTable,
        input: &mut crate::input_queue::InputQueue,
    ) -> SyscallResponse {
        if !validate_abi_version(request.abi_version) {
            return SyscallResponse::err(AbiError::AbiVersionMismatch);
        }
        if request.buffer_len > MAX_SYSCALL_PAYLOAD {
            return SyscallResponse::err(AbiError::InvalidPayload);
        }

        let syscall = match request.syscall_id {
            x if x == Syscall::Yield.id() => Syscall::Yield,
            x if x == Syscall::IpcSend.id() => Syscall::IpcSend,
            x if x == Syscall::IpcReceive.id() => Syscall::IpcReceive,
            x if x == Syscall::CapabilityQuery.id() => Syscall::CapabilityQuery,
            x if x == Syscall::HandleClose.id() => Syscall::HandleClose,
            x if x == Syscall::MonotonicTime.id() => Syscall::MonotonicTime,
            _ => return SyscallResponse::err(AbiError::InvalidSyscall),
        };

        match syscall {
            Syscall::Yield => SyscallResponse::ok(0),
            Syscall::IpcSend => {
                if !self.check_capability(
                    pid,
                    request.capability,
                    capabilities,
                    ResourceType::IpcChannel,
                    request.arg0,
                    Rights::WRITE,
                ) {
                    return SyscallResponse::err(AbiError::PermissionDenied);
                }
                // Die eingefrorene ABI uebertraegt keine Bytes: `SyscallRequest`
                // ABI 2.0.0 traegt Pufferzeiger und Laenge getrennt, der Aufruf
                // ist also nicht mehr strukturell blind. Was noch fehlt, ist die
                // Kopie in den bzw. aus dem Userspace-Adressraum: die macht der
                // Trampolin, nachdem er den Zeiger gegen die Prozessabbildung
                // geprueft hat. Bis dahin bleibt ein `ok` bei Nutzdaten die
                // Behauptung, Daten seien unterwegs.
                if request.buffer_len > 0 {
                    return SyscallResponse::err(AbiError::EndpointUnavailable);
                }
                SyscallResponse::ok(0)
            }
            Syscall::IpcReceive => {
                if request.arg1 as usize > MAX_SYSCALL_PAYLOAD {
                    return SyscallResponse::err(AbiError::InvalidPayload);
                }
                if !self.check_capability(
                    pid,
                    request.capability,
                    capabilities,
                    ResourceType::IpcChannel,
                    request.arg0,
                    Rights::READ,
                ) {
                    return SyscallResponse::err(AbiError::PermissionDenied);
                }
                // Frueher: `ok(request.arg1)`. Das meldete dem Prozess
                // "arg1 Bytes liegen fuer dich bereit", obwohl weder ein Puffer
                // noch eine Nachricht existiert — der Aufrufer haette in einen
                // ungeschriebenen Puffer gelesen und den Inhalt fuer Eingabe
                // gehalten. Ohne Pufferzeiger in der ABI gibt es keine Bytes.
                //
                // Ein Tastenereignis braucht aber keine Bytes: Scancode und
                // Zeichen passen in das 64-Bit-Ergebnis (siehe
                // `InputEvent::packed`). Fuer den Kernel-Eingabekanal liefert
                // der Syscall deshalb das naechste Ereignis als Wert zurueck,
                // 0 bedeutet "nichts anliegend". Fuer jeden anderen Kanal
                // bleibt es beim ehrlichen "kein Transport".
                if request.buffer_len > 0 {
                    return SyscallResponse::err(AbiError::EndpointUnavailable);
                }
                if request.arg0 == KERNEL_INPUT_CHANNEL {
                    return SyscallResponse::ok(match input.pop() {
                        Some(event) => event.packed(),
                        None => 0,
                    });
                }
                SyscallResponse::err(AbiError::EndpointUnavailable)
            }
            Syscall::CapabilityQuery => {
                let cap = match request.capability {
                    Some(cap) => cap,
                    None => return SyscallResponse::err(AbiError::InvalidHandle),
                };
                if !capabilities.check_any(pid, cap.0, Rights::INSPECT) {
                    return SyscallResponse::err(AbiError::PermissionDenied);
                }
                match capabilities.get(CapId(cap.0)) {
                    Some(c) => SyscallResponse::ok(c.rights.bits() as u64),
                    None => SyscallResponse::err(AbiError::InvalidHandle),
                }
            }
            Syscall::HandleClose => {
                // SC-005: Schliessen widerruft die Capability wirklich (inkl. Nachfahren).
                let Some(handle) = request.capability else {
                    return SyscallResponse::err(AbiError::InvalidHandle);
                };
                let cap_id = CapId(handle.0);
                match capabilities.get(cap_id) {
                    Some(cap) if cap.owner == pid => {
                        capabilities.revoke(cap_id);
                        SyscallResponse::ok(1)
                    }
                    Some(_) => SyscallResponse::err(AbiError::PermissionDenied),
                    None => SyscallResponse::err(AbiError::InvalidHandle),
                }
            }
            Syscall::MonotonicTime => SyscallResponse::ok(monotonic_ticks()),
        }
    }

    fn check_capability(
        &self,
        pid: Pid,
        handle: Option<CapabilityHandle>,
        capabilities: &CapabilityTable,
        resource_type: ResourceType,
        resource_id: u64,
        rights: Rights,
    ) -> bool {
        let Some(handle) = handle else { return false };
        capabilities
            .get(CapId(handle.0))
            .map(|cap| {
                cap.owner == pid
                    && cap.resource_type == resource_type
                    && cap.resource_id == resource_id
                    && cap.rights.has(rights)
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(n: u32) -> Pid {
        Pid(n)
    }

    fn request(syscall: Syscall) -> SyscallRequest {
        SyscallRequest {
            abi_version: libshivacore::ABI_VERSION,
            syscall_id: syscall.id(),
            capability: None,
            arg0: 0,
            arg1: 0,
            buffer_ptr: 0,
            buffer_len: 0,
        }
    }

    #[test]
    fn rejects_unknown_syscall() {
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                syscall_id: 0xffff,
                ..request(Syscall::Yield)
            },
            &mut CapabilityTable::new(),
            &mut crate::input_queue::InputQueue::new(),
        );
        assert_eq!(r.error, Some(AbiError::InvalidSyscall));
    }

    #[test]
    fn rejects_major_abi_mismatch() {
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                abi_version: 0x0003_0000,
                ..request(Syscall::Yield)
            },
            &mut CapabilityTable::new(),
            &mut crate::input_queue::InputQueue::new(),
        );
        assert_eq!(r.error, Some(AbiError::AbiVersionMismatch));
    }

    #[test]
    fn rejects_oversized_payload() {
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                buffer_len: MAX_SYSCALL_PAYLOAD + 1,
                ..request(Syscall::Yield)
            },
            &mut CapabilityTable::new(),
            &mut crate::input_queue::InputQueue::new(),
        );
        assert_eq!(r.error, Some(AbiError::InvalidPayload));
    }

    #[test]
    fn ipc_requires_matching_capability_and_right() {
        let mut table = CapabilityTable::new();
        let mut input = crate::input_queue::InputQueue::new();
        let cap = table.create(pid(1), ResourceType::IpcChannel, 42, Rights::WRITE);
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: 42,
                ..request(Syscall::IpcSend)
            },
            &mut table,
            &mut crate::input_queue::InputQueue::new(),
        );
        assert_eq!(r.error, None);
        let denied = d.dispatch(
            pid(2),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: 42,
                ..request(Syscall::IpcSend)
            },
            &mut table,
            &mut crate::input_queue::InputQueue::new(),
        );
        assert_eq!(denied.error, Some(AbiError::PermissionDenied));
    }

    #[test]
    fn ipc_receive_does_not_claim_bytes_it_cannot_deliver() {
        let mut table = CapabilityTable::new();
        let mut input = crate::input_queue::InputQueue::new();
        let cap = table.create(pid(1), ResourceType::IpcChannel, 7, Rights::READ);
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: 7,
                arg1: 64,
                ..request(Syscall::IpcReceive)
            },
            &mut table,
            &mut crate::input_queue::InputQueue::new(),
        );
        // Kein Erfolg, kein Wert: ohne Pufferzeiger kann nichts angekommen sein.
        assert_eq!(r.error, Some(AbiError::EndpointUnavailable));
        assert_eq!(r.value, 0, "kein Byte darf als geliefert gelten");
    }

    #[test]
    fn ipc_send_with_payload_reports_the_missing_transport() {
        let mut table = CapabilityTable::new();
        let mut input = crate::input_queue::InputQueue::new();
        let cap = table.create(pid(1), ResourceType::IpcChannel, 8, Rights::WRITE);
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: 8,
                buffer_len: 16,
                buffer_ptr: 0x400000,
                ..request(Syscall::IpcSend)
            },
            &mut table,
            &mut crate::input_queue::InputQueue::new(),
        );
        assert_eq!(r.error, Some(AbiError::EndpointUnavailable));
    }

    #[test]
    fn ipc_send_without_payload_is_a_truthful_signal() {
        let mut table = CapabilityTable::new();
        let mut input = crate::input_queue::InputQueue::new();
        let cap = table.create(pid(1), ResourceType::IpcChannel, 9, Rights::WRITE);
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: 9,
                ..request(Syscall::IpcSend)
            },
            &mut table,
            &mut crate::input_queue::InputQueue::new(),
        );
        assert_eq!(r.error, None, "ein Signal ohne Nutzdaten ist zulaessig");
    }

    #[test]
    fn ipc_checks_the_right_before_the_transport() {
        // Ein Unbefugter darf nichts ueber den Kanalzustand erfahren — auch
        // nicht, dass es keinen Transport gibt. Die Rechtepruefung bleibt
        // zuerst.
        let mut table = CapabilityTable::new();
        let mut input = crate::input_queue::InputQueue::new();
        let cap = table.create(pid(1), ResourceType::IpcChannel, 10, Rights::READ);
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(2),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: 10,
                arg1: 32,
                ..request(Syscall::IpcReceive)
            },
            &mut table,
            &mut crate::input_queue::InputQueue::new(),
        );
        assert_eq!(r.error, Some(AbiError::PermissionDenied));
    }

    #[test]
    fn a_parsed_block_becomes_a_request_with_pointer_and_length() {
        let mut block = libshivacore::SyscallBlock::new(
            libshivacore::Syscall::IpcSend,
            Some(libshivacore::CapabilityHandle(9)),
        );
        block.arg0 = 3;
        block.buffer_ptr = 0x0040_1000;
        block.buffer_len = 32;
        let request = SyscallRequest::from_block(&block);
        assert_eq!(request.syscall_id, libshivacore::Syscall::IpcSend.id());
        assert_eq!(request.capability, Some(libshivacore::CapabilityHandle(9)));
        assert_eq!(request.buffer_ptr, 0x0040_1000);
        assert_eq!(request.buffer_len, 32);
        assert!(libshivacore::validate_abi_version(request.abi_version));
    }

    #[test]
    fn a_block_round_trips_through_the_byte_parser() {
        let mut block = libshivacore::SyscallBlock::new(
            libshivacore::Syscall::IpcReceive,
            Some(libshivacore::CapabilityHandle(1)),
        );
        block.arg0 = crate::syscall::KERNEL_INPUT_CHANNEL;
        let mut bytes = [0u8; libshivacore::SYSCALL_BLOCK_MAX_SIZE];
        let written = block.write_to(&mut bytes).expect("Block schreiben");
        let parsed = libshivacore::SyscallBlock::parse(&bytes[..written]).expect("Block lesen");
        let request = SyscallRequest::from_block(&parsed);
        assert_eq!(request.arg0, crate::syscall::KERNEL_INPUT_CHANNEL);
        assert_eq!(request.buffer_len, 0);
    }

    #[test]
    fn kernel_input_channel_hands_out_the_next_event() {
        use crate::input_queue::InputEvent;
        let mut d = SyscallDispatcher::new();
        let mut table = CapabilityTable::new();
        let cap = table.create(
            pid(1),
            ResourceType::IpcChannel,
            KERNEL_INPUT_CHANNEL,
            Rights::READ,
        );
        let mut input = crate::input_queue::InputQueue::new();
        input.push(InputEvent::new(0x1E, Some('a')));
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: KERNEL_INPUT_CHANNEL,
                ..request(Syscall::IpcReceive)
            },
            &mut table,
            &mut input,
        );
        assert_eq!(r.error, None);
        assert_eq!(
            InputEvent::unpacked(r.value),
            Some(InputEvent::new(0x1E, Some('a')))
        );
        assert_eq!(input.len(), 0, "das Ereignis ist verbraucht");
    }

    #[test]
    fn empty_kernel_input_channel_reports_nothing() {
        let mut d = SyscallDispatcher::new();
        let mut table = CapabilityTable::new();
        let cap = table.create(
            pid(1),
            ResourceType::IpcChannel,
            KERNEL_INPUT_CHANNEL,
            Rights::READ,
        );
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: KERNEL_INPUT_CHANNEL,
                ..request(Syscall::IpcReceive)
            },
            &mut table,
            &mut crate::input_queue::InputQueue::new(),
        );
        assert_eq!(r.error, None);
        assert_eq!(
            r.value, 0,
            "0 heisst: nichts anliegend, nicht 'kein Transport'"
        );
    }

    #[test]
    fn receive_checks_the_right_before_handing_out_events() {
        use crate::input_queue::InputEvent;
        let mut d = SyscallDispatcher::new();
        let mut table = CapabilityTable::new();
        // Kanal 1 gibt es, aber der Prozess hat kein Recht darauf.
        let cap = table.create(
            pid(2),
            ResourceType::IpcChannel,
            KERNEL_INPUT_CHANNEL,
            Rights::READ,
        );
        let mut input = crate::input_queue::InputQueue::new();
        input.push(InputEvent::new(0x1E, Some('a')));
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: KERNEL_INPUT_CHANNEL,
                ..request(Syscall::IpcReceive)
            },
            &mut table,
            &mut input,
        );
        assert_eq!(r.error, Some(AbiError::PermissionDenied));
        assert_eq!(input.len(), 1, "ohne Recht bleibt das Ereignis liegen");
    }

    #[test]
    fn foreign_channel_still_has_no_transport() {
        let mut d = SyscallDispatcher::new();
        let mut table = CapabilityTable::new();
        let cap = table.create(pid(1), ResourceType::IpcChannel, 42, Rights::READ);
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: 42,
                ..request(Syscall::IpcReceive)
            },
            &mut table,
            &mut crate::input_queue::InputQueue::new(),
        );
        assert_eq!(r.error, Some(AbiError::EndpointUnavailable));
    }

    #[test]
    fn monotonic_time_only_advances_on_tick() {
        let mut d = SyscallDispatcher::new();
        let mut table = CapabilityTable::new();
        let mut input = crate::input_queue::InputQueue::new();
        let a = d.dispatch(
            pid(1),
            request(Syscall::MonotonicTime),
            &mut table,
            &mut input,
        );
        let b = d.dispatch(
            pid(1),
            request(Syscall::MonotonicTime),
            &mut table,
            &mut input,
        );
        assert_eq!(a.value, b.value, "Lesen darf die Zeit nicht veraendern");
        advance_monotonic_tick();
        let c = d.dispatch(
            pid(1),
            request(Syscall::MonotonicTime),
            &mut table,
            &mut input,
        );
        assert_eq!(c.value, a.value + 1, "genau ein Tick pro Hardware-IRQ");
    }

    #[test]
    fn handle_close_revokes_the_capability() {
        let mut table = CapabilityTable::new();
        let mut input = crate::input_queue::InputQueue::new();
        let cap = table.create(pid(1), ResourceType::IpcChannel, 42, Rights::WRITE);
        assert_eq!(table.count(), 1);
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                ..request(Syscall::HandleClose)
            },
            &mut table,
            &mut crate::input_queue::InputQueue::new(),
        );
        assert_eq!(r.error, None);
        assert_eq!(r.value, 1);
        assert_eq!(table.count(), 0, "HandleClose muss widerrufen");
    }

    #[test]
    fn handle_close_denies_foreign_owner() {
        let mut table = CapabilityTable::new();
        let mut input = crate::input_queue::InputQueue::new();
        let cap = table.create(pid(1), ResourceType::IpcChannel, 42, Rights::WRITE);
        let mut d = SyscallDispatcher::new();
        let denied = d.dispatch(
            pid(2),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                ..request(Syscall::HandleClose)
            },
            &mut table,
            &mut crate::input_queue::InputQueue::new(),
        );
        assert_eq!(denied.error, Some(AbiError::PermissionDenied));
        assert_eq!(table.count(), 1, "fremdes Handle bleibt bestehen");
    }

    #[test]
    fn handle_close_rejects_unknown_handle() {
        let mut table = CapabilityTable::new();
        let mut input = crate::input_queue::InputQueue::new();
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(9999)),
                ..request(Syscall::HandleClose)
            },
            &mut table,
            &mut crate::input_queue::InputQueue::new(),
        );
        assert_eq!(r.error, Some(AbiError::InvalidHandle));
    }

    #[test]
    fn capability_query_returns_rights_mask() {
        let mut table = CapabilityTable::new();
        let mut input = crate::input_queue::InputQueue::new();
        let cap = table.create(
            pid(1),
            ResourceType::IpcChannel,
            7,
            Rights::READ | Rights::INSPECT,
        );
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                ..request(Syscall::CapabilityQuery)
            },
            &mut table,
            &mut crate::input_queue::InputQueue::new(),
        );
        assert_eq!(r.error, None);
        assert_eq!(r.value, (Rights::READ | Rights::INSPECT).bits() as u64);
    }

    #[test]
    fn capability_query_requires_inspect_right() {
        let mut table = CapabilityTable::new();
        let mut input = crate::input_queue::InputQueue::new();
        let cap = table.create(pid(1), ResourceType::IpcChannel, 7, Rights::READ);
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                ..request(Syscall::CapabilityQuery)
            },
            &mut table,
            &mut crate::input_queue::InputQueue::new(),
        );
        assert_eq!(r.error, Some(AbiError::PermissionDenied));
    }
}
