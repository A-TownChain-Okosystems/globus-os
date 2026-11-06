//! Verwaltung der Kernel-Abo-Capability fuer IRQ-Lines.
//!
//! Anlass war ein Fehler in meiner eigenen Verdrahtung: Beim Abonnieren einer
//! Line legte ich eine Capability an, warf die Tabelle weg und behielt nur eine
//! Kopie. Eine Capability ausserhalb ihrer Tabelle kennt ihre Abstammung nicht
//! und laesst sich nicht widerrufen — `revoke` haette nie etwas gefunden.
//!
//! Hier lebt die Capability dort, wo sie hingehoert: in der Tabelle. Der
//! Interrupt-Pfad holt sie bei jedem ACK ab, statt mit einer Kopie zu arbeiten.

use crate::ats1000::Pid;
use crate::capability::{
    CapId, Capability, CapabilityError, CapabilityTable, ResourceType, Rights,
};

/// Capability-Tabelle des Kernels und die Line, die er abonniert hat.
pub struct KernelCaps {
    table: CapabilityTable,
    subscriber: Option<CapId>,
}

impl Default for KernelCaps {
    fn default() -> Self {
        Self::new()
    }
}

impl KernelCaps {
    pub fn new() -> Self {
        Self {
            table: CapabilityTable::new(),
            subscriber: None,
        }
    }

    /// Legt die Abo-Capability fuer eine Line an. Der Rechtesatz kommt aus
    /// `create_typed`, also aus SC-006, nicht aus diesem Aufruf.
    pub fn grant(&mut self, line: u32) -> Result<Capability, CapabilityError> {
        let id = self.table.create_typed(
            Pid(0),
            ResourceType::Irq,
            u64::from(line),
            Rights(Rights::SUBSCRIBE.0 | Rights::ACK.0),
        )?;
        let cap = self
            .table
            .get(id)
            .cloned()
            .ok_or(CapabilityError::RightsExceeded)?;
        self.subscriber = Some(id);
        Ok(cap)
    }

    /// Die Capability zum ACK, aus der Tabelle gelesen.
    pub fn subscriber_cap(&self) -> Option<Capability> {
        self.subscriber.and_then(|id| self.table.get(id).cloned())
    }

    /// Beendet das Abonnement und widerruft die Capability.
    pub fn revoke_subscription(&mut self) -> bool {
        match self.subscriber.take() {
            Some(id) => {
                self.table.revoke(id);
                true
            }
            None => false,
        }
    }

    pub fn has_subscription(&self) -> bool {
        self.subscriber.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capability_carries_the_rights_of_its_resource_type() {
        let mut caps = KernelCaps::new();
        let cap = caps.grant(1).expect("IRQ-Capability");
        assert_eq!(cap.resource_type, ResourceType::Irq);
        assert_eq!(cap.resource_id, 1);
        assert!(cap.rights.has(Rights::SUBSCRIBE));
        assert!(cap.rights.has(Rights::ACK));
        assert_eq!(cap.owner, Pid(0));
    }

    #[test]
    fn the_capability_is_readable_while_subscribed() {
        let mut caps = KernelCaps::new();
        caps.grant(1).expect("IRQ-Capability");
        assert!(caps.has_subscription());
        let cap = caps.subscriber_cap().expect("Capability in der Tabelle");
        assert_eq!(cap.resource_id, 1);
    }

    #[test]
    fn revoke_removes_the_capability_from_the_table() {
        // Der eigentliche Punkt des Umbaus: eine Kopie ohne Tabelle liesse sich
        // hier nicht entfernen.
        let mut caps = KernelCaps::new();
        caps.grant(1).expect("IRQ-Capability");
        assert!(caps.revoke_subscription());
        assert!(caps.subscriber_cap().is_none(), "Capability ist weg");
        assert!(!caps.has_subscription());
    }

    #[test]
    fn revoking_twice_reports_nothing_to_revoke() {
        let mut caps = KernelCaps::new();
        caps.grant(1).expect("IRQ-Capability");
        assert!(caps.revoke_subscription());
        assert!(!caps.revoke_subscription());
    }

    #[test]
    fn a_line_can_be_subscribed_again_after_a_revoke() {
        let mut caps = KernelCaps::new();
        let first = caps.grant(1).expect("IRQ-Capability");
        caps.revoke_subscription();
        let second = caps.grant(1).expect("IRQ-Capability nach Widerruf");
        assert_ne!(first.id, second.id, "neue Capability, neue Id");
        assert_eq!(second.resource_id, 1);
    }

    #[test]
    fn two_lines_get_two_capabilities() {
        let mut caps = KernelCaps::new();
        caps.grant(1).expect("Line 1");
        // grant setzt den Subskribenten um; das ist gewollt, weil der Kernel
        // bisher genau eine Line abonniert. Die Tabelle behaelt aber beide.
        let second = caps.grant(12).expect("Line 12");
        assert_eq!(second.resource_id, 12);
        assert_ne!(second.id, CapId(0));
    }
}
