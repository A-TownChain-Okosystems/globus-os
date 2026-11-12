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
use crate::security::{record, set_audit_clock, AuditResult};
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
        // Der Takt, mit dem Audit-Eintraege versehen werden, kommt aus der
        // einzigen Zeitquelle des Kernels (Timer-Ticks).
        set_audit_clock(self.dispatcher.ticks());
        let response = self
            .dispatcher
            .dispatch(pid, request, &mut self.capabilities, input);
        // Autorisierungsfehler sind sicherheitsrelevant: ein Prozess hat ein
        // Recht beansprucht, das ihm nicht gehoert.
        if matches!(response.error, Some(libshivacore::AbiError::PermissionDenied)) {
            record(
                "syscall-gate",
                "authorization.denied",
                &alloc::format!("pid={:?} syscall_id={}", pid, request.syscall_id),
                AuditResult::Denied,
            );
        }
        response
    }

    /// Gibt einem Prozess das Leserecht auf einen Kanal. Fuer den
    /// Kernel-Eingabekanal ist das der einzige Weg, an Tastenereignisse zu
    /// kommen; ohne Cap beantwortet der Syscall nur "kein Transport".
    pub fn grant_channel_read(&mut self, pid: Pid, channel: u64) -> CapId {
        let cap = self
            .capabilities
            .create(pid, ResourceType::IpcChannel, channel, Rights::READ);
        record(
            "syscall-gate",
            "capability.create",
            &alloc::format!("pid={:?} cap={:?} channel={}", pid, cap, channel),
            AuditResult::Allowed,
        );
        cap
    }

    /// Widerruft alle Capabilities eines Prozesses am Gate.
    pub fn revoke_all_for(&mut self, pid: Pid) -> usize {
        let revoked = self.capabilities.revoke_all_for(pid);
        record(
            "syscall-gate",
            "capability.revoke_all",
            &alloc::format!("pid={:?} revoked={}", pid, revoked),
            AuditResult::Allowed,
        );
        revoked
    }

    /// Anzahl der Capabilities eines Prozesses am Gate.
    pub fn cap_count_for(&self, pid: Pid) -> usize {
        self.capabilities.count_for(pid)
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

    #[test]
    fn gate_records_capability_events_in_audit_log() {
        let mut gate = SyscallGate::new();
        let before = crate::security::recorded_events();
        let cap = gate.grant_channel_read(pid(11), 0x1234);
        assert_eq!(crate::security::recorded_events(), before + 1);
        let revoked = gate.revoke_all_for(pid(11));
        assert_eq!(revoked, 1);
        assert_eq!(crate::security::recorded_events(), before + 2);
        let entries = crate::security::KERNEL_AUDIT.entries();
        let actions: alloc::vec::Vec<&str> =
            entries.iter().map(|e| e.action.as_str()).collect();
        assert!(actions.contains(&"capability.create"));
        assert!(actions.contains(&"capability.revoke_all"));
        assert!(crate::security::audit_chain_intact());
        assert!(entries.iter().any(|e| e.resource.contains(&alloc::format!("{:?}", cap))));
    }

    #[test]
    fn gate_records_authorization_denials() {
        let mut gate = SyscallGate::new();
        let cap = gate
            .capabilities_mut()
            .create(pid(12), ResourceType::IpcChannel, 3, Rights::READ);
        let req = SyscallRequest {
            capability: Some(CapabilityHandle(cap.0)),
            arg0: 3,
            ..request(Syscall::IpcSend)
        };
        let before = crate::security::recorded_events();
        assert_eq!(
            gate.handle(pid(13), req, &mut InputQueue::new()).error,
            Some(AbiError::PermissionDenied)
        );
        assert_eq!(crate::security::recorded_events(), before + 1);
        assert!(crate::security::KERNEL_AUDIT
            .entries()
            .iter()
            .any(|e| e.action == "authorization.denied" && e.result == crate::security::AuditResult::Denied));
    }
}
