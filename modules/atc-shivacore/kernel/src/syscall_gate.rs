// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! Syscall-Gate — der einzige Uebergang von Userspace in den Kernel.
//!
//! Vorher stand in `interrupts.rs` am Vektor 0x80 nur eine serielle Debug-Zeile:
//! der `SyscallDispatcher` war zwar vorhanden und getestet, aber von keinem
//! Interrupt-Pfad erreichbar. Dieses Modul ist die fehlende Klammer: es haelt
//! Dispatcher und Capability-Tabelle zusammen und ist die einzige Stelle, die
//! ein Gate aufruft.
//!
//! Der Zustand liegt bewusst hier (nicht in der Bin), damit die Verdrahtung
//! testbar bleibt — die Bin haelt nur eine `Mutex<Option<SyscallGate>>`.

use crate::ats1000::Pid;
use crate::capability::{CapId, CapabilityTable, ResourceType, Rights};
use crate::input_queue::InputQueue;
use crate::syscall::{SyscallDispatcher, SyscallRequest, SyscallResponse};

/// Dispatcher + Capability-Tabelle als eine Einheit.
pub struct SyscallGate {
    dispatcher: SyscallDispatcher,
    capabilities: CapabilityTable,
}

impl Default for SyscallGate {
    fn default() -> Self {
        Self::new()
    }
}

impl SyscallGate {
    pub const fn new() -> Self {
        Self {
            dispatcher: SyscallDispatcher::new(),
            capabilities: CapabilityTable::new(),
        }
    }

    /// Ein Syscall vom Userspace: Dispatch inkl. Capability-Pruefung.
    pub fn handle(
        &mut self,
        pid: Pid,
        request: SyscallRequest,
        input: &mut InputQueue,
    ) -> SyscallResponse {
        self.dispatcher
            .dispatch(pid, request, &mut self.capabilities, input)
    }

    /// Gibt einem Prozess das Leserecht auf einen Kanal. Fuer den
    /// Kernel-Eingabekanal ist das der einzige Weg, an Tastenereignisse zu
    /// kommen; ohne Cap beantwortet der Syscall nur "kein Transport".
    pub fn grant_channel_read(&mut self, pid: Pid, channel: u64) -> CapId {
        self.capabilities
            .create(pid, ResourceType::IpcChannel, channel, Rights::READ)
    }

    /// Ein Hardware-Tick (Timer-IRQ) — die einzige Quelle fuer MonotonicTime.
    pub fn on_tick(&mut self) {
        self.dispatcher.on_tick();
    }

    pub fn ticks(&self) -> u64 {
        self.dispatcher.ticks()
    }

    pub fn capabilities(&self) -> &CapabilityTable {
        &self.capabilities
    }

    pub fn capabilities_mut(&mut self) -> &mut CapabilityTable {
        &mut self.capabilities
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{ResourceType, Rights};
    use libshivacore::{AbiError, CapabilityHandle, Syscall};

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
    fn gate_dispatches_yield_for_any_pid() {
        let mut gate = SyscallGate::new();
        let r = gate.handle(pid(7), request(Syscall::Yield), &mut InputQueue::new());
        assert_eq!(r.error, None);
    }

    #[test]
    fn gate_enforces_capability_ownership() {
        let mut gate = SyscallGate::new();
        let cap =
            gate.capabilities_mut()
                .create(pid(1), ResourceType::IpcChannel, 5, Rights::WRITE);
        let req = SyscallRequest {
            capability: Some(CapabilityHandle(cap.0)),
            arg0: 5,
            ..request(Syscall::IpcSend)
        };
        assert_eq!(gate.handle(pid(1), req, &mut InputQueue::new()).error, None);
        assert_eq!(
            gate.handle(pid(2), req, &mut InputQueue::new()).error,
            Some(AbiError::PermissionDenied)
        );
    }

    #[test]
    fn gate_close_revokes_through_the_table() {
        let mut gate = SyscallGate::new();
        let cap =
            gate.capabilities_mut()
                .create(pid(1), ResourceType::Memory, 0x1000, Rights::READ);
        let req = SyscallRequest {
            capability: Some(CapabilityHandle(cap.0)),
            ..request(Syscall::HandleClose)
        };
        assert_eq!(gate.handle(pid(1), req, &mut InputQueue::new()).value, 1);
        assert_eq!(gate.capabilities().count(), 0);
    }

    #[test]
    fn gate_time_only_moves_with_ticks() {
        let mut gate = SyscallGate::new();
        let a = gate
            .handle(
                pid(1),
                request(Syscall::MonotonicTime),
                &mut InputQueue::new(),
            )
            .value;
        gate.on_tick();
        gate.on_tick();
        let b = gate
            .handle(
                pid(1),
                request(Syscall::MonotonicTime),
                &mut InputQueue::new(),
            )
            .value;
        assert_eq!(b, a + 2, "zwei Ticks, zwei Schritte");
    }

    #[test]
    fn gate_rejects_unknown_syscall() {
        let mut gate = SyscallGate::new();
        let req = SyscallRequest {
            syscall_id: 0xbeef,
            ..request(Syscall::Yield)
        };
        assert_eq!(
            gate.handle(pid(1), req, &mut InputQueue::new()).error,
            Some(AbiError::InvalidSyscall)
        );
    }
}
