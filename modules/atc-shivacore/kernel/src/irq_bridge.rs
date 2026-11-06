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
//!
//! ## ACK-Politik fuer Geraete-Subskribenten
//!
//! "Maskiert bis ACK" allein sagt nicht, wer quittiert und was passiert, wenn
//! niemand quittiert. Diese Bruecke beantwortet beides:
//!
//! 1. **Quittung beim Empfang (Regel).** Holt der Subskribent die Nachricht ab,
//!    quittiert er damit die Zustellung: `ack_on_receive` gibt die Line frei.
//!    Ein eigener ACK-Syscall waere eine ABI-Erweiterung ohne neuen Nutzen.
//! 2. **Frist als Netz (Sicherung).** Kommt die Quittung nicht innerhalb von
//!    `ACK_DEADLINE_TICKS`, gibt der Kernel die Line selbst frei
//!    (`IrqTable::reclaim`), zaehlt die verpasste Quittung und sperrt nach
//!    `ACK_STRIKE_LIMIT` verpassten Quittungen das Abonnement. Ohne diese
//!    Sicherung haelt ein Prozess, der nicht mehr empfaengt, die Line fuer
//!    immer gesperrt — das Geraet waere still ausgefallen, ohne dass es jemand
//!    merkt.
//! 3. **Wiederaufnahme ist ausdruecklich.** Ein ausgesetztes Abonnement laeuft
//!    nur nach `resume` mit der IRQ-Cap wieder an. Stille Wiederaufnahme waere
//!    ein Kreis, in dem ein Prozess sich selbst beliebig oft zurueckholt.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

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

/// Ticks, die eine offene Zustellung hoechstens auf ihre Quittung wartet.
pub const ACK_DEADLINE_TICKS: u64 = 8;

/// So viele verpasste Quittungen hintereinander, dann wird ausgesetzt.
pub const ACK_STRIKE_LIMIT: u8 = 3;

/// Eine Zustellung, die auf ihre Quittung wartet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingAck {
    line: u32,
    vector: u32,
    pid: Pid,
    since: u64,
    strikes: u8,
}

/// Ergebnis einer verstrichenen Frist, damit der Aufrufer es berichten kann.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AckTimeout {
    pub line: u32,
    pub pid: Pid,
    pub strikes: u8,
    pub suspended: bool,
}

/// Zaehler der ACK-Politik. Ohne Zaehler waere "hat funktioniert" eine Meinung.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AckStats {
    pub acked_on_receive: u64,
    pub timed_out: u64,
    pub suspended: usize,
    pub suppressed: u64,
}

/// Haelt die Line-Zustellung und das Maskenregister zusammen.
pub struct IrqBridge<M: IrqMasking> {
    table: IrqTable,
    masking: M,
    pending: Vec<PendingAck>,
    strikes: BTreeMap<u32, u8>,
    suspended: BTreeSet<u32>,
    acked_on_receive: u64,
    timed_out: u64,
    suppressed: u64,
}

impl<M: IrqMasking> IrqBridge<M> {
    pub fn new(masking: M) -> Self {
        Self {
            table: IrqTable::new(),
            masking,
            pending: Vec::new(),
            strikes: BTreeMap::new(),
            suspended: BTreeSet::new(),
            acked_on_receive: 0,
            timed_out: 0,
            suppressed: 0,
        }
    }

    /// Der Interrupt-Pfad ruft das hier auf. Rueckgabe ist der Subskribent, dem
    /// die Zustellung gehoert, oder `None`, wenn die Line ruhig bleibt
    /// (niemand abonniert oder die letzte Zustellung noch offen).
    pub fn on_interrupt(&mut self, line: u32, vector: u32) -> Result<Option<Pid>, BridgeError> {
        self.on_interrupt_at(line, vector, 0)
    }

    /// Wie `on_interrupt`, aber mit Uhrzeit. Nur so kann die Frist greifen:
    /// ohne Zeitstempel laesst sich nicht sagen, wie lange eine Quittung
    /// aussteht. Ausgesetzte Abonnements bekommen nichts mehr, und der
    /// unterdrueckte Trigger wird gezaehlt, damit die Ursache sichtbar bleibt.
    pub fn on_interrupt_at(
        &mut self,
        line: u32,
        vector: u32,
        now: u64,
    ) -> Result<Option<Pid>, BridgeError> {
        if self.suspended.contains(&line) {
            self.suppressed += 1;
            return Ok(None);
        }
        let Some(pid) = self.table.deliver(line) else {
            return Ok(None);
        };
        let strikes = self.strikes.get(&line).copied().unwrap_or(0);
        self.pending.retain(|p| p.line != line);
        self.pending.push(PendingAck {
            line,
            vector,
            pid,
            since: now,
            strikes,
        });
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
        // Jede Quittung raeumt den Eintrag, auch die des Kernels selbst. Ohne
        // das bliebe eine bereits quittierte Zustellung in der Fristliste
        // stehen und wuerde beim Ablauf stillschweigend verworfen — die Frist
        // haette dann nichts zu sagen.
        self.pending.retain(|p| p.line != line);
        self.strikes.remove(&line);
        Ok(line)
    }

    /// Quittung beim Empfang: der Subskribent hat die Nachricht abgeholt, die
    /// Line ist frei. Damit ist REQ-SC006-03 erfuellt, ohne eigenen ACK-Syscall.
    pub fn ack_on_receive(&mut self, cap: &Capability, vector: u32) -> Result<u32, BridgeError> {
        let line = self.ack(cap, vector)?;
        self.acked_on_receive += 1;
        Ok(line)
    }

    /// Laesst die Fristen verstreichen. Rueckgabe sind die verpassten
    /// Quittungen, damit der Aufrufer sie berichten kann.
    pub fn sweep_acks(&mut self, now: u64) -> Vec<AckTimeout> {
        let mut expired = Vec::new();
        let due: Vec<PendingAck> = self
            .pending
            .iter()
            .copied()
            .filter(|p| now.saturating_sub(p.since) > ACK_DEADLINE_TICKS)
            .collect();
        for entry in due {
            // Die Line kann bereits freigegeben sein; dann ist nichts zu tun.
            if self.table.reclaim(entry.line).is_err() {
                self.pending.retain(|p| p.line != entry.line);
                continue;
            }
            // Freigeben scheitert nur an der Hardware; der Zustand ist trotzdem
            // freigegeben, deshalb wird der Vorgang gezaehlt und berichtet.
            let _ = self.masking.unmask_vector(entry.vector);
            self.pending.retain(|p| p.line != entry.line);
            let strikes = entry.strikes.saturating_add(1);
            self.strikes.insert(entry.line, strikes);
            let suspended = strikes >= ACK_STRIKE_LIMIT;
            if suspended {
                self.suspended.insert(entry.line);
            }
            self.timed_out += 1;
            expired.push(AckTimeout {
                line: entry.line,
                pid: entry.pid,
                strikes,
                suspended,
            });
        }
        expired
    }

    /// Nimmt ein ausgesetztes Abonnement wieder auf. Braucht die IRQ-Cap mit
    /// ACK-Recht: wer quittieren darf, darf auch zurueckholen.
    pub fn resume(&mut self, cap: &Capability) -> Result<u32, BridgeError> {
        if cap.resource_type != crate::capability::ResourceType::Irq {
            return Err(BridgeError::Irq(IrqError::NotAnIrqCap));
        }
        if !cap.rights.has(crate::capability::Rights::ACK) {
            return Err(BridgeError::Irq(IrqError::MissingAck));
        }
        let line = cap.resource_id as u32;
        if !self.suspended.remove(&line) {
            return Err(BridgeError::Irq(IrqError::NotSuspended));
        }
        self.strikes.remove(&line);
        Ok(line)
    }

    pub fn ack_stats(&self) -> AckStats {
        AckStats {
            acked_on_receive: self.acked_on_receive,
            timed_out: self.timed_out,
            suspended: self.suspended.len(),
            suppressed: self.suppressed,
        }
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
    fn an_ack_cancels_its_deadline_even_before_the_sweep() {
        // Genau der Fall, der im Bootlauf zuerst stillschweigend verworfen
        // wurde: der Kernel quittiert im Handler. Steht der Eintrag danach
        // noch in der Fristliste, verfaellt eine bereits erledigte Zustellung.
        let (mut bridge, ack) = bridge_with_subscriber(3, 7);
        bridge.on_interrupt_at(3, 3, 100).unwrap();
        assert_eq!(bridge.ack(&ack, 3).unwrap(), 3);
        assert!(bridge.sweep_acks(100 + ACK_DEADLINE_TICKS + 1).is_empty());
        assert_eq!(bridge.ack_stats().timed_out, 0);
    }

    #[test]
    fn receive_ack_releases_the_line_and_counts_it() {
        let (mut bridge, ack) = bridge_with_subscriber(3, 7);
        assert_eq!(bridge.on_interrupt_at(3, 3, 100).unwrap(), Some(Pid(7)));
        assert_eq!(bridge.ack_on_receive(&ack, 3).unwrap(), 3);
        assert_eq!(bridge.masking().unmask_calls, 1);
        let stats = bridge.ack_stats();
        assert_eq!(stats.acked_on_receive, 1);
        assert_eq!(stats.timed_out, 0);
        // Die Line ist wieder frei, der naechste Trigger kommt durch.
        assert_eq!(bridge.on_interrupt_at(3, 3, 101).unwrap(), Some(Pid(7)));
    }

    #[test]
    fn a_missing_ack_expires_and_frees_the_line() {
        let (mut bridge, _ack) = bridge_with_subscriber(3, 7);
        bridge.on_interrupt_at(3, 3, 100).unwrap();
        // Vor Ablauf der Frist passiert nichts.
        assert!(bridge.sweep_acks(100 + ACK_DEADLINE_TICKS).is_empty());
        let expired = bridge.sweep_acks(100 + ACK_DEADLINE_TICKS + 1);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].strikes, 1);
        assert!(!expired[0].suspended);
        assert_eq!(bridge.masking().unmask_calls, 1);
        // Die Line ist frei, ein neuer Trigger wird wieder zugestellt.
        assert_eq!(bridge.on_interrupt_at(3, 3, 200).unwrap(), Some(Pid(7)));
        // Eine zweite Auswertung derselben Frist zaehlt nicht doppelt.
        assert!(bridge.sweep_acks(100 + ACK_DEADLINE_TICKS + 2).is_empty());
        assert_eq!(bridge.ack_stats().timed_out, 1);
    }

    #[test]
    fn three_missed_acks_suspend_the_subscription() {
        let (mut bridge, ack) = bridge_with_subscriber(3, 7);
        let mut expired = Vec::new();
        for round in 0..ACK_STRIKE_LIMIT as u64 {
            let now = 100 + round * 100;
            bridge.on_interrupt_at(3, 3, now).unwrap();
            expired.append(&mut bridge.sweep_acks(now + ACK_DEADLINE_TICKS + 1));
        }
        assert_eq!(expired.len(), ACK_STRIKE_LIMIT as usize);
        assert!(expired.last().unwrap().suspended);
        assert_eq!(bridge.ack_stats().suspended, 1);
        // Ausgesetzt heisst: es kommt nichts mehr, und der Trigger wird gezaehlt.
        assert_eq!(bridge.on_interrupt_at(3, 3, 900).unwrap(), None);
        assert_eq!(bridge.ack_stats().suppressed, 1);
        // Wiederaufnahme nur mit der IRQ-Cap.
        assert_eq!(bridge.resume(&ack).unwrap(), 3);
        assert_eq!(bridge.on_interrupt_at(3, 3, 901).unwrap(), Some(Pid(7)));
    }

    #[test]
    fn resume_rejects_a_non_irq_cap_and_an_unset_line() {
        // create_typed vergibt fuer IRQ-Caps SUBSCRIBE und ACK zusammen; ein
        // IRQ-Cap ohne ACK-Recht laesst sich deshalb nicht bauen. Geprueft wird
        // darum die Art der Cap und der Zustand der Line.
        let mut table = CapabilityTable::new();
        let memory = table
            .create_typed(
                Pid(7),
                crate::capability::ResourceType::Memory,
                3,
                Rights::READ,
            )
            .expect("Memory-Cap");
        let (mut bridge, ack) = bridge_with_subscriber(3, 7);
        assert_eq!(
            bridge.resume(table.get(memory).unwrap()),
            Err(BridgeError::Irq(IrqError::NotAnIrqCap))
        );
        assert_eq!(
            bridge.resume(&ack),
            Err(BridgeError::Irq(IrqError::NotSuspended))
        );
    }

    #[test]
    fn a_received_ack_clears_the_strike_count() {
        let (mut bridge, ack) = bridge_with_subscriber(3, 7);
        bridge.on_interrupt_at(3, 3, 100).unwrap();
        bridge.sweep_acks(100 + ACK_DEADLINE_TICKS + 1);
        assert_eq!(bridge.ack_stats().timed_out, 1);
        bridge.on_interrupt_at(3, 3, 300).unwrap();
        bridge.ack_on_receive(&ack, 3).unwrap();
        // Nach einer echten Quittung faengt die Serie wieder bei null an.
        bridge.on_interrupt_at(3, 3, 400).unwrap();
        let expired = bridge.sweep_acks(400 + ACK_DEADLINE_TICKS + 1);
        assert_eq!(expired[0].strikes, 1);
        assert!(!expired[0].suspended);
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
