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

use alloc::sync::Arc;
use spin::MutexGuard;

use crate::ats1000::Pid;
use crate::cap_authority::CapabilityAuthority;
use crate::capability::{CapId, CapabilityTable, ResourceType, Rights};
use crate::input_queue::InputQueue;
use crate::security::{record, set_audit_clock, AuditResult};
use crate::syscall::{SyscallDispatcher, SyscallRequest, SyscallResponse};

/// Dispatcher + Capability-Tabelle als eine Einheit.
pub struct SyscallGate {
    dispatcher: SyscallDispatcher,
    /// Die zentrale Authority. Das Gate haelt keine eigene Tabelle mehr; im
    /// laufenden Kernel zeigt dieser Arc auf dieselbe Instanz wie
    /// `ProcessManager` und `Vfs`.
    authority: Arc<CapabilityAuthority>,
}

impl Default for SyscallGate {
    fn default() -> Self {
        Self::new()
    }
}

impl SyscallGate {
    /// Ein Gate mit eigener Authority — fuer Unit-Tests, die isoliert bleiben
    /// muessen. Der laufende Kernel nimmt `with_authority`.
    pub fn new() -> Self {
        Self::with_authority(CapabilityAuthority::new_shared())
    }

    /// Ein Gate auf einer vorhandenen Authority. Damit entscheidet im Kernel
    /// genau eine Tabelle ueber Syscall-Rechte.
    pub fn with_authority(authority: Arc<CapabilityAuthority>) -> Self {
        Self {
            dispatcher: SyscallDispatcher::new(),
            authority,
        }
    }

    /// Die geteilte Authority dieses Gates.
    pub fn authority(&self) -> &Arc<CapabilityAuthority> {
        &self.authority
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
        let syscall_id = request.syscall_id;
        let response = self
            .dispatcher
            .dispatch(pid, request, &mut self.authority.lock(), input);
        // Autorisierungsfehler sind sicherheitsrelevant: ein Prozess hat ein
        // Recht beansprucht, das ihm nicht gehoert.
        if matches!(response.error, Some(libshivacore::AbiError::PermissionDenied)) {
            record(
                "syscall-gate",
                "authorization.denied",
                &alloc::format!("pid={:?} syscall_id={}", pid, syscall_id),
                AuditResult::Denied,
            );
        }
        response
    }

    /// Gibt einem Prozess das Leserecht auf einen Kanal. Fuer den
    /// Kernel-Eingabekanal ist das der einzige Weg, an Tastenereignisse zu
    /// kommen; ohne Cap beantwortet der Syscall nur "kein Transport".
    pub fn grant_channel_read(&mut self, pid: Pid, channel: u64) -> CapId {
        // Audit kommt aus der Authority, damit es genau eine Aufzeichnung gibt.
        self.authority
            .create(pid, ResourceType::IpcChannel, channel, Rights::READ)
    }

    /// Widerruft alle Capabilities eines Prozesses am Gate.
    pub fn revoke_all_for(&mut self, pid: Pid) -> usize {
        self.authority.revoke_all_for(pid)
    }

    /// Anzahl der Capabilities eines Prozesses am Gate.
    pub fn cap_count_for(&self, pid: Pid) -> usize {
        self.authority.count_for(pid)
    }

    /// Ein Hardware-Tick (Timer-IRQ) — die einzige Quelle fuer MonotonicTime.
    pub fn on_tick(&mut self) {
        self.dispatcher.on_tick();
    }

    pub fn ticks(&self) -> u64 {
        self.dispatcher.ticks()
    }

    pub fn capabilities(&self) -> MutexGuard<'_, CapabilityTable> {
        self.authority.lock()
    }

    pub fn capabilities_mut(&self) -> MutexGuard<'_, CapabilityTable> {
        self.authority.lock()
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
        assert!(crate::security::recorded_events() > before);
        let revoked = gate.revoke_all_for(pid(11));
        assert_eq!(revoked, 1);
        assert!(crate::security::recorded_events() >= before + 2);
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
        assert!(crate::security::recorded_events() > before);
        assert!(crate::security::KERNEL_AUDIT
            .entries()
            .iter()
            .any(|e| e.action == "authorization.denied" && e.result == crate::security::AuditResult::Denied));
    }
}
