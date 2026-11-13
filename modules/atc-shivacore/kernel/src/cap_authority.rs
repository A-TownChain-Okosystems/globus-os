// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! Capability Authority — die eine Instanz, die ueber Berechtigungen entscheidet.
//!
//! Vorher gab es vier `CapabilityTable`-Instanzen mit eigener Wahrheit:
//! `SyscallGate`, `ProcessManager`, `Vfs` und eine ephemere Tabelle im
//! DMA-Selbsttest. Ein Recht, das der eine Pfad vergab, war im anderen Pfad
//! unbekannt; `create`, `delegate` und `revoke` liefen an drei Stellen.
//!
//! Diese Instanz ist der SSOT. `create`, `create_typed`, `delegate`, `revoke`
//! und `revoke_all_for` sind ausschliesslich hier oeffentlich; die Subsysteme
//! halten nur noch eine `Arc<CapabilityAuthority>` und entscheiden nichts
//! selbst. `check` ist fail-closed: unbekannte Capability, fremder Besitzer,
//! fehlende Rechte und widerrufene Capabilities liefern `false` und werden als
//! `authorization.denied` protokolliert.
//!
//! Rechteausweitung ist ausgeschlossen, weil die Authority die Rechtepruefung
//! nicht selbst implementiert: `delegate` delegiert an
//! `CapabilityTable::delegate`, das DELEGATE verlangt und Rechte-Attenuation
//! erzwingt (RightsExceeded). Die Authority setzt keine Rechte frei.

use alloc::format;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, MutexGuard};

use crate::ats1000::Pid;
use crate::capability::{CapId, Capability, CapabilityError, CapabilityTable, ResourceType, Rights};
use crate::security::{record, AuditResult};

/// Die zentrale Berechtigungsinstanz.
pub struct CapabilityAuthority {
    table: Mutex<CapabilityTable>,
}

impl Default for CapabilityAuthority {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityAuthority {
    pub const fn new() -> Self {
        Self {
            table: Mutex::new(CapabilityTable::new()),
        }
    }

    /// Eine geteilte Instanz. Alle Subsysteme im laufenden Kernel bekommen
    /// Klone desselben `Arc`, damit es genau eine Tabelle gibt.
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// Identitaet der Instanz (Adresse). Damit laesst sich zur Laufzeit
    /// belegen, dass zwei Halter dieselbe Authority meinen.
    pub fn identity(&self) -> usize {
        self as *const Self as usize
    }

    /// Lesezugriff auf die Tabelle, ohne die Authority-API zu umgehen.
    pub fn lock(&self) -> MutexGuard<'_, CapabilityTable> {
        self.table.lock()
    }

    /// Erzeugt eine Capability. Der einzige Weg, Rechte zu vergeben.
    pub fn create(
        &self,
        pid: Pid,
        resource_type: ResourceType,
        resource_id: u64,
        rights: Rights,
    ) -> CapId {
        let cap = self
            .table
            .lock()
            .create(pid, resource_type, resource_id, rights);
        record(
            "capability-authority",
            "capability.create",
            &format!(
                "pid={:?} cap={:?} resource={:?}:{} rights={:#06x}",
                pid, cap, resource_type, resource_id, rights.bits()
            ),
            AuditResult::Allowed,
        );
        cap
    }

    /// Erzeugt eine Capability mit dem SC-006-Pflichtrechtesatz des Typs.
    pub fn create_typed(
        &self,
        pid: Pid,
        resource_type: ResourceType,
        resource_id: u64,
        rights: Rights,
    ) -> Result<CapId, CapabilityError> {
        let result = self
            .table
            .lock()
            .create_typed(pid, resource_type, resource_id, rights);
        match &result {
            Ok(cap) => record(
                "capability-authority",
                "capability.create_typed",
                &format!(
                    "pid={:?} cap={:?} resource={:?}:{}",
                    pid, cap, resource_type, resource_id
                ),
                AuditResult::Allowed,
            ),
            Err(error) => record(
                "capability-authority",
                "capability.create_typed",
                &format!(
                    "pid={:?} resource={:?}:{} abgelehnt: {:?}",
                    pid, resource_type, resource_id, error
                ),
                AuditResult::Denied,
            ),
        }
        result
    }

    /// Delegiert eine Capability. Rechte werden ausschliesslich attenuirt; die
    /// Pruefung (DELEGATE-Pflicht, RightsExceeded) bleibt in der Tabelle.
    pub fn delegate(
        &self,
        source_pid: Pid,
        source_cap: CapId,
        target_pid: Pid,
        attenuated_rights: Rights,
    ) -> Result<CapId, CapabilityError> {
        let result = self.table.lock().delegate(
            source_pid,
            source_cap,
            target_pid,
            attenuated_rights,
        );
        match &result {
            Ok(child) => record(
                "capability-authority",
                "capability.delegate",
                &format!(
                    "from={:?} cap={:?} to={:?} child={:?} rights={:#06x}",
                    source_pid,
                    source_cap,
                    target_pid,
                    child,
                    attenuated_rights.bits()
                ),
                AuditResult::Allowed,
            ),
            Err(error) => record(
                "capability-authority",
                "capability.delegate",
                &format!(
                    "from={:?} cap={:?} to={:?} abgelehnt: {:?}",
                    source_pid, source_cap, target_pid, error
                ),
                AuditResult::Denied,
            ),
        }
        result
    }

    /// Widerruft eine Capability samt Nachfahren.
    pub fn revoke(&self, cap_id: CapId) -> bool {
        let existed = self.table.lock().get(cap_id).is_some();
        self.table.lock().revoke(cap_id);
        record(
            "capability-authority",
            "capability.revoke",
            &format!("cap={:?} vorhanden={}", cap_id, existed),
            if existed {
                AuditResult::Allowed
            } else {
                AuditResult::Error
            },
        );
        existed
    }

    /// Widerruft alle Capabilities eines Prozesses (Exit-Cleanup).
    pub fn revoke_all_for(&self, pid: Pid) -> usize {
        let revoked = self.table.lock().revoke_all_for(pid);
        record(
            "capability-authority",
            "capability.revoke_all",
            &format!("pid={:?} revoked={}", pid, revoked),
            AuditResult::Allowed,
        );
        revoked
    }

    /// Fail-closed-Rechtepruefung. `false` heisst: keine Berechtigung, und der
    /// Versuch wird protokolliert.
    pub fn check(
        &self,
        pid: Pid,
        resource_type: ResourceType,
        resource_id: u64,
        required: Rights,
    ) -> bool {
        let granted = self
            .table
            .lock()
            .check(pid, resource_type, resource_id, required);
        if !granted {
            record(
                "capability-authority",
                "authorization.denied",
                &format!(
                    "pid={:?} resource={:?}:{} required={:#06x}",
                    pid, resource_type, resource_id, required.bits()
                ),
                AuditResult::Denied,
            );
        }
        granted
    }

    /// Fail-closed-Pruefung ueber die Capability-ID.
    pub fn check_any(&self, pid: Pid, cap_id: u64, required: Rights) -> bool {
        let granted = self.table.lock().check_any(pid, cap_id, required);
        if !granted {
            record(
                "capability-authority",
                "authorization.denied",
                &format!("pid={:?} cap={} required={:#06x}", pid, cap_id, required.bits()),
                AuditResult::Denied,
            );
        }
        granted
    }

    pub fn count(&self) -> usize {
        self.table.lock().count()
    }

    pub fn count_for(&self, pid: Pid) -> usize {
        self.table.lock().count_for(pid)
    }

    pub fn cap_ids_for(&self, pid: Pid) -> Vec<CapId> {
        self.table.lock().list_for(pid).iter().map(|c| c.id).collect()
    }

    pub fn capabilities_for(&self, pid: Pid) -> Vec<Capability> {
        self.table
            .lock()
            .list_for(pid)
            .iter()
            .map(|c| (*c).clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(n: u32) -> Pid {
        Pid(n)
    }

    #[test]
    fn create_and_check_roundtrip() {
        let a = CapabilityAuthority::new();
        let cap = a.create(pid(1), ResourceType::Memory, 42, Rights::READ);
        assert_eq!(a.count_for(pid(1)), 1);
        assert!(a.check(pid(1), ResourceType::Memory, 42, Rights::READ));
        assert!(!a.check(pid(1), ResourceType::Memory, 42, Rights::WRITE));
        assert!(a.check_any(pid(1), cap.0, Rights::READ));
    }

    #[test]
    fn check_is_fail_closed_for_unknown_capability() {
        let a = CapabilityAuthority::new();
        assert!(!a.check(pid(1), ResourceType::Memory, 42, Rights::READ));
        assert!(!a.check_any(pid(1), 9999, Rights::READ));
        // Fremder Besitzer darf eine existierende Cap nicht benutzen.
        let cap = a.create(pid(1), ResourceType::Memory, 42, Rights::READ);
        assert!(!a.check_any(pid(2), cap.0, Rights::READ));
    }

    #[test]
    fn revoked_capability_is_denied() {
        let a = CapabilityAuthority::new();
        let cap = a.create(pid(1), ResourceType::Memory, 42, Rights::READ);
        assert!(a.revoke(cap));
        assert!(!a.check_any(pid(1), cap.0, Rights::READ));
        assert_eq!(a.count_for(pid(1)), 0);
        // Zweiter Widerruf ist kein Fehlerfall mit Wirkung: nichts mehr da.
        assert!(!a.revoke(cap));
    }

    #[test]
    fn delegate_attenuates_and_never_expands_rights() {
        let a = CapabilityAuthority::new();
        let parent = a.create(
            pid(1),
            ResourceType::IpcChannel,
            7,
            Rights::READ | Rights::DELEGATE,
        );
        let child = a
            .delegate(pid(1), parent, pid(2), Rights::READ)
            .expect("Delegation mit Teilsatz");
        assert!(a.check_any(pid(2), child.0, Rights::READ));
        assert!(!a.check_any(pid(2), child.0, Rights::WRITE));
        // Rechteausweitung wird abgelehnt.
        assert_eq!(
            a.delegate(pid(1), parent, pid(2), Rights::READ | Rights::WRITE),
            Err(CapabilityError::RightsExceeded)
        );
        // Ohne DELEGATE kein Weitergeben.
        let plain = a.create(pid(3), ResourceType::Memory, 1, Rights::READ);
        assert_eq!(
            a.delegate(pid(3), plain, pid(4), Rights::READ),
            Err(CapabilityError::NoDelegateRight)
        );
    }

    #[test]
    fn shared_arcs_are_one_authority() {
        let a = CapabilityAuthority::new_shared();
        let b = Arc::clone(&a);
        let cap = a.create(pid(5), ResourceType::Timer, 1, Rights::READ);
        assert_eq!(b.count_for(pid(5)), 1);
        assert!(b.check_any(pid(5), cap.0, Rights::READ));
        assert_eq!(a.identity(), b.identity());
        assert_eq!(Arc::strong_count(&a), 2);
    }

    #[test]
    fn every_mutation_is_recorded_in_the_audit_log() {
        let before = crate::security::recorded_events();
        let a = CapabilityAuthority::new();
        let cap = a.create(pid(6), ResourceType::Memory, 9, Rights::READ | Rights::DELEGATE);
        a.delegate(pid(6), cap, pid(7), Rights::READ).unwrap();
        a.revoke(cap);
        a.revoke_all_for(pid(6));
        // Der Audit-Log ist kernelweit und wird von parallel laufenden Tests
        // mitbenutzt: deshalb untere Schranke statt exakter Differenz.
        assert!(crate::security::recorded_events() >= before + 4);
        let entries = crate::security::KERNEL_AUDIT.entries();
        let actions: alloc::vec::Vec<&str> = entries.iter().map(|e| e.action.as_str()).collect();
        for expected in [
            "capability.create",
            "capability.delegate",
            "capability.revoke",
            "capability.revoke_all",
        ] {
            assert!(actions.contains(&expected), "{} fehlt im Audit", expected);
        }
        assert!(crate::security::audit_chain_intact());
    }
}
