// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// SC-006 REQ-SC006-02 / REQ-SC006-03 — IRQ-Zustellung mit Maskierung bis ACK
// Kernel Layer | Chain-ID 658467
//
// Diese Schicht haelt den Zustand der Interrupt-Vermittlung: genau ein
// Subskribent je Line, eine offene Notification, und die Line gilt als
// maskiert, solange kein ACK auf der IRQ-Cap vorliegt. Ein erneuter Trigger
// in dieser Zeit wird gezaehlt, aber nicht zugestellt (kein Re-Trigger vor
// ACK, Koaleszierung im Kernel).
//
// Die Hardware-Seite (LAPIC-Maske, EOI) liegt nicht in diesem Modul: die
// APIC-Anbindung ist im kompilierten Satz nicht vorhanden. Diese Schicht
// liefert den Zustand, den der IRQ-Eintritt abfragt, und die Stelle, an der
// das ACK die Maske wieder freigibt.
// ─────────────────────────────────────────────────────────────────────────

use alloc::collections::BTreeMap;

use crate::ats1000::Pid;
use crate::capability::{Capability, ResourceType, Rights};

/// Fehler der IRQ-Vermittlung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqError {
    /// Cap zeigt nicht auf eine Interrupt-Line.
    NotAnIrqCap,
    /// Der Cap fehlt das Recht, die Line zu abonnieren.
    MissingSubscribe,
    /// Der Cap fehlt das Recht, die Zustellung zu quittieren.
    MissingAck,
    /// Der Cap gehoert nicht dem Subskribenten dieser Line.
    NotSubscriber,
    /// Fuer diese Line ist bereits ein Subskribent eingetragen.
    AlreadySubscribed,
    /// Es liegt keine offene Zustellung vor.
    NotPending,
    /// Fuer diese Line ist kein Abonnement ausgesetzt.
    NotSuspended,
}

/// Zustand einer Line: ruhig oder mit offener, noch nicht quittierter Zustellung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqState {
    Idle,
    Pending,
}

/// Eine vermittelte Interrupt-Line.
#[derive(Debug, Clone, Copy)]
pub struct IrqLine {
    pub line: u32,
    pub subscriber: Pid,
    pub state: IrqState,
    /// Zustellungen insgesamt, auch die koaleszierten.
    pub deliveries: u64,
    /// Trigger, die waehrend einer offenen Zustellung verworfen wurden.
    pub coalesced: u64,
}

impl IrqLine {
    /// Die Line ist maskiert, solange die Zustellung offen ist (REQ-SC006-03).
    pub fn is_masked(&self) -> bool {
        self.state == IrqState::Pending
    }
}

/// Zustandstabelle aller vermittelten Lines.
#[derive(Debug, Default)]
pub struct IrqTable {
    lines: BTreeMap<u32, IrqLine>,
}

impl IrqTable {
    pub fn new() -> Self {
        Self {
            lines: BTreeMap::new(),
        }
    }

    /// Traegt den Eigentuemer der IRQ-Cap als Subskribenten ein.
    pub fn subscribe(&mut self, cap: &Capability) -> Result<u32, IrqError> {
        if cap.resource_type != ResourceType::Irq {
            return Err(IrqError::NotAnIrqCap);
        }
        if !cap.rights.has(Rights::SUBSCRIBE) {
            return Err(IrqError::MissingSubscribe);
        }
        let line = cap.resource_id as u32;
        if self.lines.contains_key(&line) {
            return Err(IrqError::AlreadySubscribed);
        }
        self.lines.insert(
            line,
            IrqLine {
                line,
                subscriber: cap.owner,
                state: IrqState::Idle,
                deliveries: 0,
                coalesced: 0,
            },
        );
        Ok(line)
    }

    /// Meldet einen Trigger. Liefert nur dann einen Empfaenger, wenn die Line
    /// ruhig war; waehrend einer offenen Zustellung wird koalesziert und nichts
    /// zugestellt (kein Re-Trigger vor ACK).
    pub fn deliver(&mut self, line: u32) -> Option<Pid> {
        let entry = self.lines.get_mut(&line)?;
        entry.deliveries += 1;
        if entry.state == IrqState::Pending {
            entry.coalesced += 1;
            return None;
        }
        entry.state = IrqState::Pending;
        Some(entry.subscriber)
    }

    /// Quittiert die Zustellung und gibt die Line wieder frei.
    pub fn ack(&mut self, cap: &Capability) -> Result<u32, IrqError> {
        if cap.resource_type != ResourceType::Irq {
            return Err(IrqError::NotAnIrqCap);
        }
        if !cap.rights.has(Rights::ACK) {
            return Err(IrqError::MissingAck);
        }
        let line = cap.resource_id as u32;
        let entry = self.lines.get_mut(&line).ok_or(IrqError::NotSubscriber)?;
        if entry.subscriber != cap.owner {
            return Err(IrqError::NotSubscriber);
        }
        if entry.state != IrqState::Pending {
            return Err(IrqError::NotPending);
        }
        entry.state = IrqState::Idle;
        Ok(line)
    }

    /// Gibt eine offene Zustellung ohne Subskribenten-ACK frei.
    ///
    /// Das ist ausdruecklich kein ACK: es ist das Sicherheitsnetz gegen einen
    /// Prozess, der die Quittung nie liefert. Die Kernel-Autoritaet reicht,
    /// weil die Alternative eine dauerhaft gesperrte Line ist, also ein
    /// stiller Ausfall genau des Geraets, das der Subskribent abonniert hat.
    /// Der Aufrufer zaehlt den Vorgang als verpasste Quittung; hier wird nur
    /// der Zustand freigegeben.
    pub fn reclaim(&mut self, line: u32) -> Result<Pid, IrqError> {
        let entry = self.lines.get_mut(&line).ok_or(IrqError::NotSubscriber)?;
        if entry.state != IrqState::Pending {
            return Err(IrqError::NotPending);
        }
        entry.state = IrqState::Idle;
        Ok(entry.subscriber)
    }

    pub fn get(&self, line: u32) -> Option<&IrqLine> {
        self.lines.get(&line)
    }

    pub fn masked_lines(&self) -> usize {
        self.lines.values().filter(|l| l.is_masked()).count()
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

#[cfg(test)]
mod sc006_irq_delivery {
    use super::*;
    use crate::capability::CapabilityTable;

    fn pid(n: u32) -> Pid {
        Pid(n)
    }

    fn irq_cap(table: &mut CapabilityTable, owner: u32, line: u64) -> crate::capability::CapId {
        table
            .create_typed(pid(owner), ResourceType::Irq, line, Rights::NONE)
            .expect("IRQ-Cap")
    }

    #[test]
    fn subscribe_needs_an_irq_cap() {
        let mut table = CapabilityTable::new();
        let mem = table
            .create_typed(pid(1), ResourceType::Memory, 1, Rights::READ)
            .unwrap();
        let mut irqs = IrqTable::new();
        assert_eq!(
            irqs.subscribe(table.get(mem).unwrap()),
            Err(IrqError::NotAnIrqCap)
        );
    }

    #[test]
    fn subscribe_takes_the_cap_owner() {
        let mut table = CapabilityTable::new();
        let cap = irq_cap(&mut table, 7, 44);
        let mut irqs = IrqTable::new();
        assert_eq!(irqs.subscribe(table.get(cap).unwrap()), Ok(44));
        assert_eq!(irqs.get(44).unwrap().subscriber, pid(7));
    }

    #[test]
    fn second_subscriber_is_rejected() {
        let mut table = CapabilityTable::new();
        let a = irq_cap(&mut table, 7, 44);
        let b = irq_cap(&mut table, 8, 44);
        let mut irqs = IrqTable::new();
        irqs.subscribe(table.get(a).unwrap()).unwrap();
        assert_eq!(
            irqs.subscribe(table.get(b).unwrap()),
            Err(IrqError::AlreadySubscribed)
        );
    }

    #[test]
    fn no_retrigger_before_ack() {
        let mut table = CapabilityTable::new();
        let cap = irq_cap(&mut table, 7, 9);
        let mut irqs = IrqTable::new();
        irqs.subscribe(table.get(cap).unwrap()).unwrap();

        assert_eq!(irqs.deliver(9), Some(pid(7)), "erste Zustellung");
        assert!(irqs.get(9).unwrap().is_masked());
        assert_eq!(irqs.deliver(9), None, "kein Re-Trigger vor ACK");
        assert_eq!(irqs.deliver(9), None);
        let line = *irqs.get(9).unwrap();
        assert_eq!(line.deliveries, 3, "alle Trigger werden gezaehlt");
        assert_eq!(line.coalesced, 2, "zwei davon koalesziert");
    }

    #[test]
    fn ack_unmasks_and_allows_the_next_delivery() {
        let mut table = CapabilityTable::new();
        let cap = irq_cap(&mut table, 7, 9);
        let mut irqs = IrqTable::new();
        irqs.subscribe(table.get(cap).unwrap()).unwrap();
        irqs.deliver(9).unwrap();
        assert_eq!(irqs.ack(table.get(cap).unwrap()), Ok(9));
        assert!(!irqs.get(9).unwrap().is_masked());
        assert_eq!(irqs.deliver(9), Some(pid(7)));
    }

    #[test]
    fn reclaim_releases_a_pending_line_without_a_subscriber_ack() {
        let mut caps = CapabilityTable::new();
        let id = irq_cap(&mut caps, 7, 3);
        let mut table = IrqTable::new();
        assert_eq!(table.subscribe(caps.get(id).unwrap()), Ok(3));
        assert!(table.deliver(3).is_some());
        assert_eq!(table.get(3).map(|l| l.state), Some(IrqState::Pending));
        assert_eq!(table.reclaim(3), Ok(Pid(7)));
        assert_eq!(table.get(3).map(|l| l.state), Some(IrqState::Idle));
        // Zweimal freigeben ist ein Fehler, kein stiller Erfolg.
        assert_eq!(table.reclaim(3), Err(IrqError::NotPending));
    }

    #[test]
    fn ack_needs_the_ack_right() {
        let mut table = CapabilityTable::new();
        // Cap mit SUBSCRIBE, aber ohne ACK laesst sich ueber create_typed nicht
        // bauen: SC-006 vergibt beide Rechte zusammen. Fuer den Test wird der
        // Rechtesatz deshalb direkt gesetzt.
        let id = irq_cap(&mut table, 7, 9);
        let mut irqs = IrqTable::new();
        irqs.subscribe(table.get(id).unwrap()).unwrap();
        irqs.deliver(9).unwrap();

        let c = table.get(id).unwrap();
        let broken = Capability {
            id: c.id,
            resource_type: c.resource_type,
            resource_id: c.resource_id,
            rights: Rights::SUBSCRIBE,
            owner: c.owner,
            parent: c.parent,
        };
        assert_eq!(irqs.ack(&broken), Err(IrqError::MissingAck));
        assert!(irqs.get(9).unwrap().is_masked(), "Line bleibt maskiert");
    }

    #[test]
    fn only_the_subscriber_may_ack() {
        let mut table = CapabilityTable::new();
        let mine = irq_cap(&mut table, 7, 9);
        let theirs = irq_cap(&mut table, 8, 9);
        let mut irqs = IrqTable::new();
        irqs.subscribe(table.get(mine).unwrap()).unwrap();
        irqs.deliver(9).unwrap();
        assert_eq!(
            irqs.ack(table.get(theirs).unwrap()),
            Err(IrqError::NotSubscriber)
        );
    }

    #[test]
    fn double_ack_is_rejected() {
        let mut table = CapabilityTable::new();
        let cap = irq_cap(&mut table, 7, 9);
        let mut irqs = IrqTable::new();
        irqs.subscribe(table.get(cap).unwrap()).unwrap();
        irqs.deliver(9).unwrap();
        irqs.ack(table.get(cap).unwrap()).unwrap();
        assert_eq!(irqs.ack(table.get(cap).unwrap()), Err(IrqError::NotPending));
    }

    #[test]
    fn unknown_line_is_ignored() {
        let mut irqs = IrqTable::new();
        assert_eq!(irqs.deliver(1234), None);
        assert!(irqs.is_empty());
        assert_eq!(irqs.masked_lines(), 0);
    }
}
