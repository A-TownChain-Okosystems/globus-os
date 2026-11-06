//! SC-006 REQ-SC006-02/03: Verbindung von der IRQ-Tabelle zum Maskenregister.
//!
//! `irq::IrqTable` weiss, ob eine Line eine offene Zustellung hat, und
//! `apic::LocalApic` kann Vektoren sperren. Beide wussten bisher nichts
//! voneinander: die Tabelle wurde im Kernel nirgends instanziiert, `deliver()`
//! hatte keinen Aufrufer, und damit war "maskiert bis ACK" nur eine Eigenschaft
//! der Datenstruktur, nicht des Interrupt-Pfads.
//!
//! Diese Bruecke ist der fehlende Aufrufer. Die Maskenebene haengt hinter einem
//! schmalen Trait, damit die Reihenfolge ohne Hardware pruefbar bleibt.
//!
//! Reihenfolge beim Eintritt: die Zustellung wird in der Tabelle vermerkt, dann
//! wird der Vektor gesperrt, dann kehrt der Handler zum Subskribenten zurueck.
//! Der Vektor ist waehrend des Handlers in Bearbeitung, ein verschachtelter
//! Interrupt derselben Line ist deshalb ausgeschlossen; entscheidend ist, dass
//! die Sperre steht, bevor der Subskribent laeuft. Erst `ack` gibt sie frei.

use crate::apic::{ApicError, ApicRegisters, LocalApic};
use crate::ats1000::Pid;
use crate::capability::Capability;
use crate::irq::{IrqError, IrqTable};

/// Was die Bruecke von der Maskenebene braucht.
pub trait IrqMasking {
    fn mask_vector(&mut self, vector: u32) -> Result<(), ApicError>;
    fn unmask_vector(&mut self, vector: u32) -> Result<(), ApicError>;
}

impl<M: ApicRegisters> IrqMasking for LocalApic<M> {
    fn mask_vector(&mut self, vector: u32) -> Result<(), ApicError> {
        self.mask(vector)
    }

    fn unmask_vector(&mut self, vector: u32) -> Result<(), ApicError> {
        self.unmask(vector)
    }
}

/// Fehler der Bruecke: entweder aus der Tabelle oder vom Maskenregister.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeError {
    Irq(IrqError),
    Apic(ApicError),
}

impl From<IrqError> for BridgeError {
    fn from(error: IrqError) -> Self {
        Self::Irq(error)
    }
}

impl From<ApicError> for BridgeError {
    fn from(error: ApicError) -> Self {
        Self::Apic(error)
    }
}

/// Haelt die Line-Zustellung und das Maskenregister zusammen.
pub struct IrqBridge<M: IrqMasking> {
    table: IrqTable,
    masking: M,
}

impl<M: IrqMasking> IrqBridge<M> {
    pub fn new(masking: M) -> Self {
        Self {
            table: IrqTable::new(),
            masking,
        }
    }

    /// Der Interrupt-Pfad ruft das hier auf. Rueckgabe ist der Subskribent, dem
    /// die Zustellung gehoert, oder `None`, wenn die Line ruhig bleibt
    /// (niemand abonniert oder die letzte Zustellung noch offen).
    pub fn on_interrupt(&mut self, line: u32, vector: u32) -> Result<Option<Pid>, BridgeError> {
        let Some(pid) = self.table.deliver(line) else {
            return Ok(None);
        };
        // Sperren, bevor der Subskribent laeuft. Scheitert das, ist die
        // Zustellung nicht abgesichert und der Aufrufer muss sie verwerfen.
        self.masking.mask_vector(vector)?;
        Ok(Some(pid))
    }

    /// Quittiert die Zustellung und gibt die Line wieder frei. Die Rechte- und
    /// Eigentuemerpruefung steckt in der Tabelle.
    pub fn ack(&mut self, cap: &Capability, vector: u32) -> Result<u32, BridgeError> {
        let line = self.table.ack(cap)?;
        self.masking.unmask_vector(vector)?;
        Ok(line)
    }

    /// Der Interrupt-Pfad muss vor dem Eintrag abonnieren.
    pub fn subscribe(&mut self, cap: &Capability) -> Result<u32, BridgeError> {
        Ok(self.table.subscribe(cap)?)
    }

    pub fn table(&self) -> &IrqTable {
        &self.table
    }

    pub fn masking(&self) -> &M {
        &self.masking
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilityTable, Rights};

    /// Maskenebene, die nur mitschreibt, was mit ihr passiert.
    struct Recorder {
        masked: [bool; 8],
        mask_calls: u32,
        unmask_calls: u32,
        fail_mask: bool,
    }

    impl Recorder {
        fn new() -> Self {
            Self {
                masked: [false; 8],
                mask_calls: 0,
                unmask_calls: 0,
                fail_mask: false,
            }
        }
    }

    impl IrqMasking for Recorder {
        fn mask_vector(&mut self, vector: u32) -> Result<(), ApicError> {
            if self.fail_mask {
                return Err(ApicError::ReservedVector);
            }
            self.mask_calls += 1;
            self.masked[vector as usize] = true;
            Ok(())
        }

        fn unmask_vector(&mut self, vector: u32) -> Result<(), ApicError> {
            self.unmask_calls += 1;
            self.masked[vector as usize] = false;
            Ok(())
        }
    }

    /// Caps entstehen ueber die Tabelle, damit Rechte und Eigentuemer genau so
    /// gesetzt sind wie im Kernel und nicht als Testliteral daneben liegen.
    fn cap_for(
        table: &mut CapabilityTable,
        owner: u64,
        line: u64,
        rights: Rights,
    ) -> crate::capability::CapId {
        table
            .create_typed(
                Pid(owner as u32),
                crate::capability::ResourceType::Irq,
                line,
                rights,
            )
            .expect("IRQ-Cap")
    }

    fn bridge_with_subscriber(line: u64, owner: u64) -> (IrqBridge<Recorder>, Capability) {
        let mut table = CapabilityTable::new();
        let subscribe_cap = cap_for(&mut table, owner, line, Rights::SUBSCRIBE);
        let ack_cap = cap_for(&mut table, owner, line, Rights::ACK);
        let mut bridge = IrqBridge::new(Recorder::new());
        bridge
            .subscribe(table.get(subscribe_cap).expect("Cap"))
            .expect("abonnieren");
        let ack = table.get(ack_cap).expect("Cap").clone();
        (bridge, ack)
    }

    #[test]
    fn interrupt_masks_the_vector_before_returning_the_subscriber() {
        let (mut bridge, _ack) = bridge_with_subscriber(44, 7);
        let pid = bridge.on_interrupt(44, 3).expect("Zustellung");
        assert_eq!(pid, Some(Pid(7)));
        assert_eq!(bridge.masking().mask_calls, 1);
        assert!(bridge.masking().masked[3], "Vektor ist gesperrt");
    }

    #[test]
    fn second_trigger_before_ack_is_coalesced_and_does_not_mask_again() {
        let (mut bridge, _ack) = bridge_with_subscriber(44, 7);
        assert_eq!(bridge.on_interrupt(44, 3).unwrap(), Some(Pid(7)));
        assert_eq!(bridge.on_interrupt(44, 3).unwrap(), None, "kein Re-Trigger");
        assert_eq!(bridge.masking().mask_calls, 1, "keine zweite Sperre");
        assert_eq!(bridge.table().get(44).unwrap().coalesced, 1);
    }

    #[test]
    fn ack_releases_the_line_and_the_next_trigger_masks_again() {
        let (mut bridge, ack_cap) = bridge_with_subscriber(44, 7);
        bridge.on_interrupt(44, 3).unwrap();
        assert_eq!(bridge.ack(&ack_cap, 3).expect("ACK"), 44);
        assert_eq!(bridge.masking().unmask_calls, 1);
        assert!(!bridge.masking().masked[3], "Line ist wieder frei");
        assert_eq!(bridge.on_interrupt(44, 3).unwrap(), Some(Pid(7)));
        assert_eq!(bridge.masking().mask_calls, 2);
    }

    #[test]
    fn ack_with_a_non_irq_cap_does_not_unmask() {
        // Ein IRQ-Cap ohne ACK laesst sich ueber die Tabelle nicht bauen:
        // create_typed vergibt fuer ResourceType::Irq immer SUBSCRIBE|ACK.
        // Die erreichbare Schranke ist deshalb der falsche Resourcentyp.
        let (mut bridge, _ack) = bridge_with_subscriber(44, 7);
        bridge.on_interrupt(44, 3).unwrap();
        let mut table = CapabilityTable::new();
        let memory = table
            .create_typed(
                Pid(7),
                crate::capability::ResourceType::Memory,
                44,
                Rights::READ,
            )
            .expect("Memory-Cap");
        let wrong_kind = table.get(memory).expect("Cap").clone();
        assert_eq!(
            bridge.ack(&wrong_kind, 3),
            Err(BridgeError::Irq(IrqError::NotAnIrqCap))
        );
        assert_eq!(bridge.masking().unmask_calls, 0);
        assert!(bridge.masking().masked[3], "Sperre bleibt bestehen");
    }

    #[test]
    fn foreign_owner_cannot_ack() {
        let (mut bridge, _ack) = bridge_with_subscriber(44, 7);
        bridge.on_interrupt(44, 3).unwrap();
        let mut table = CapabilityTable::new();
        let foreign_id = cap_for(&mut table, 9, 44, Rights::ACK);
        let foreign = table.get(foreign_id).expect("Cap").clone();
        assert_eq!(
            bridge.ack(&foreign, 3),
            Err(BridgeError::Irq(IrqError::NotSubscriber))
        );
        assert!(bridge.masking().masked[3]);
    }

    #[test]
    fn interrupt_without_subscriber_masks_nothing() {
        let mut bridge = IrqBridge::new(Recorder::new());
        assert_eq!(bridge.on_interrupt(9, 3).unwrap(), None);
        assert_eq!(bridge.masking().mask_calls, 0);
        assert!(!bridge.masking().masked[3]);
    }

    #[test]
    fn failing_mask_is_reported_and_no_notification_counted_as_safe() {
        let (mut bridge, _ack) = bridge_with_subscriber(44, 7);
        bridge.masking.fail_mask = true;
        assert_eq!(
            bridge.on_interrupt(44, 3),
            Err(BridgeError::Apic(ApicError::ReservedVector))
        );
    }
}
