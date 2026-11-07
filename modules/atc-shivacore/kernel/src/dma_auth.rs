// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// SC-006 REQ-SC006-09 — DMA-Autorisierung ueber Device-Caps
// Kernel Layer | Chain-ID 658467
//
// Ein Geraet, das DMA macht, schreibt ohne jeden Syscall in den Speicher. Die
// Rechtepruefung kann deshalb nicht am Zugriff haengen, sondern muss davor
// liegen: an der Zuordnung von Domain und Speicherbereich.
//
// Diese Schicht ist die Policy zwischen Device-Cap und IOMMU-Vertrag. Sie
// entscheidet, WER fuer WELCHES Geraet einen DMA-Bereich bekommen darf. Die
// Hardware-Seite (IOMMU-Seitentabellen schreiben, Faults behandeln) liegt
// nicht hier und ist offen; ohne sie ist das hier eine geprueft entschiedene
// Absicht, keine erzwungene Wirkung. Das steht so auch in der Doku, damit die
// Luecke nicht als erledigt gilt.
//
// Zwei Eigenschaften sind der Kern:
//
// 1. **Kein Erweitern.** Eine Device-Cap autorisiert genau ihr eigenes Geraet.
//    Wer eine Cap fuer Geraet 3 hat, kann keine Domain fuer Geraet 5
//    zuordnen. Sonst waere jede Cap ein Generalschluessel.
// 2. **Domain gehoert dem Prozess.** Eine Domain ist einem Eigentuemer
//    zugeordnet; fremde Prozesse koennen sie weder befuellen noch abraeumen.
// ─────────────────────────────────────────────────────────────────────────

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::ats1000::Pid;
use crate::capability::{Capability, ResourceType, Rights};

/// Was eine DMA-Anforderung ablehnen kann.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaError {
    /// Die Cap zeigt nicht auf ein Geraet.
    NotADeviceCap,
    /// Der Cap fehlt das Zugriffsrecht.
    MissingAccess,
    /// Die Cap gehoert zu einem anderen Geraet als dem angeforderten.
    WrongDevice,
    /// Die Domain existiert nicht.
    DomainNotFound,
    /// Die Domain gehoert einem anderen Prozess.
    DomainOfAnotherProcess,
    /// Der Bereich ist leer oder laeuft ueber.
    InvalidRegion,
    /// Der Bereich ist groesser als fuer ein Geraet erlaubt.
    RegionTooLarge,
    /// Fuer dieses Geraet ist bereits eine Domain eingetragen.
    DeviceAlreadyAssigned,
}

/// Ein DMA-Bereich, wie ihn das Geraet sieht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaWindow {
    /// Adresse, die das Geraet anspricht.
    pub iova: u64,
    /// Adresse im physischen Speicher.
    pub physical: u64,
    pub length: u64,
    pub writable: bool,
}

/// Was diese Schicht ueber einen Bereich wissen muss, um ihn zu beurteilen.
pub const MAX_WINDOW_BYTES: u64 = 64 * 1024 * 1024;

/// Eine DMA-Domain: Eigentuemer, Geraet, Bereiche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmaDomain {
    pub id: u64,
    pub owner: Pid,
    pub device: u64,
    pub windows: Vec<DmaWindow>,
}

/// Zaehler. Ohne sie waere "abgelehnt" nicht von "nie gefragt" zu trennen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DmaStats {
    pub authorized: u64,
    pub denied_wrong_device: u64,
    pub denied_missing_access: u64,
    pub denied_foreign_domain: u64,
}

/// Die Policy-Schicht zwischen Device-Cap und IOMMU.
#[derive(Debug, Default)]
pub struct DmaAuthorizer {
    domains: BTreeMap<u64, DmaDomain>,
    device_to_domain: BTreeMap<u64, u64>,
    next_domain: u64,
    stats: DmaStats,
}

impl DmaAuthorizer {
    pub fn new() -> Self {
        Self {
            domains: BTreeMap::new(),
            device_to_domain: BTreeMap::new(),
            next_domain: 1,
            stats: DmaStats::default(),
        }
    }

    /// Prueft eine Device-Cap gegen das angefragte Geraet.
    ///
    /// Die Reihenfolge ist Absicht: erst die Art (damit ein Memory-Cap nicht
    /// als Geraet durchgeht), dann das Recht, dann die Identitaet. Ein falsches
    /// Geraet ist ein anderer Fehler als ein fehlendes Recht, und der
    /// Unterschied ist bei der Fehlersuche entscheidend.
    fn check_device_cap(&mut self, cap: &Capability, device: u64) -> Result<(), DmaError> {
        if cap.resource_type != ResourceType::Device {
            return Err(DmaError::NotADeviceCap);
        }
        if !cap.rights.has(Rights::ACCESS) {
            self.stats.denied_missing_access += 1;
            return Err(DmaError::MissingAccess);
        }
        if cap.resource_id != device {
            self.stats.denied_wrong_device += 1;
            return Err(DmaError::WrongDevice);
        }
        Ok(())
    }

    /// Legt fuer ein Geraet eine Domain an. Ein Geraet bekommt genau eine;
    /// eine zweite Zuordnung waere ein zweiter Weg in denselben Speicher.
    pub fn assign_device(&mut self, cap: &Capability, device: u64) -> Result<u64, DmaError> {
        self.check_device_cap(cap, device)?;
        if self.device_to_domain.contains_key(&device) {
            return Err(DmaError::DeviceAlreadyAssigned);
        }
        let id = self.next_domain;
        self.next_domain += 1;
        self.domains.insert(
            id,
            DmaDomain {
                id,
                owner: cap.owner,
                device,
                windows: Vec::new(),
            },
        );
        self.device_to_domain.insert(device, id);
        self.stats.authorized += 1;
        Ok(id)
    }

    /// Traegt einen Bereich ein. Die Cap muss zum Geraet der Domain passen und
    /// dem Eigentuemer der Domain gehoeren.
    pub fn map_window(
        &mut self,
        cap: &Capability,
        domain: u64,
        window: DmaWindow,
    ) -> Result<(), DmaError> {
        let device = {
            let entry = self.domains.get(&domain).ok_or(DmaError::DomainNotFound)?;
            (entry.device, entry.owner)
        };
        self.check_device_cap(cap, device.0)?;
        if device.1 != cap.owner {
            self.stats.denied_foreign_domain += 1;
            return Err(DmaError::DomainOfAnotherProcess);
        }
        validate_window(window)?;
        let entry = self
            .domains
            .get_mut(&domain)
            .ok_or(DmaError::DomainNotFound)?;
        entry.windows.push(window);
        self.stats.authorized += 1;
        Ok(())
    }

    /// Nimmt einen Bereich wieder heraus. Die Pruefung ist dieselbe: Abraeumen
    /// ist ein Schreibzugriff auf die Zuordnung und kein Sonderfall.
    pub fn unmap_window(
        &mut self,
        cap: &Capability,
        domain: u64,
        iova: u64,
    ) -> Result<DmaWindow, DmaError> {
        let (device, owner) = {
            let entry = self.domains.get(&domain).ok_or(DmaError::DomainNotFound)?;
            (entry.device, entry.owner)
        };
        self.check_device_cap(cap, device)?;
        if owner != cap.owner {
            self.stats.denied_foreign_domain += 1;
            return Err(DmaError::DomainOfAnotherProcess);
        }
        let entry = self
            .domains
            .get_mut(&domain)
            .ok_or(DmaError::DomainNotFound)?;
        let index = entry
            .windows
            .iter()
            .position(|w| w.iova == iova)
            .ok_or(DmaError::DomainNotFound)?;
        Ok(entry.windows.remove(index))
    }

    pub fn domain(&self, id: u64) -> Option<&DmaDomain> {
        self.domains.get(&id)
    }

    pub fn domains_of(&self, owner: Pid) -> Vec<&DmaDomain> {
        self.domains.values().filter(|d| d.owner == owner).collect()
    }

    pub fn stats(&self) -> DmaStats {
        self.stats
    }
}

/// Bereichspruefung vor der Eintragung: leer, ueberlaufend oder zu gross.
fn validate_window(window: DmaWindow) -> Result<(), DmaError> {
    if window.length == 0
        || window.iova.checked_add(window.length).is_none()
        || window.physical.checked_add(window.length).is_none()
    {
        return Err(DmaError::InvalidRegion);
    }
    if window.length > MAX_WINDOW_BYTES {
        return Err(DmaError::RegionTooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilityTable;

    fn device_cap(
        table: &mut CapabilityTable,
        owner: u32,
        device: u64,
    ) -> crate::capability::CapId {
        table
            .create_typed(Pid(owner), ResourceType::Device, device, Rights::ACCESS)
            .expect("Device-Cap")
    }

    fn window(iova: u64, length: u64) -> DmaWindow {
        DmaWindow {
            iova,
            physical: 0x1000_0000,
            length,
            writable: true,
        }
    }

    #[test]
    fn a_device_cap_opens_exactly_its_own_device() {
        let mut table = CapabilityTable::new();
        let cap = device_cap(&mut table, 7, 3);
        let mut auth = DmaAuthorizer::new();
        let domain = auth
            .assign_device(table.get(cap).unwrap(), 3)
            .expect("Domain fuer Geraet 3");
        let entry = auth.domain(domain).expect("Domain");
        assert_eq!(entry.device, 3);
        assert_eq!(entry.owner, Pid(7));

        // Dieselbe Cap oeffnet kein zweites Geraet.
        assert_eq!(
            auth.assign_device(table.get(cap).unwrap(), 5),
            Err(DmaError::WrongDevice)
        );
        assert_eq!(auth.stats().denied_wrong_device, 1);
    }

    #[test]
    fn a_memory_cap_is_not_a_device_cap() {
        let mut table = CapabilityTable::new();
        let mem = table
            .create_typed(Pid(7), ResourceType::Memory, 3, Rights::READ)
            .expect("Memory-Cap");
        let mut auth = DmaAuthorizer::new();
        assert_eq!(
            auth.assign_device(table.get(mem).unwrap(), 3),
            Err(DmaError::NotADeviceCap)
        );
    }

    #[test]
    fn a_second_domain_for_the_same_device_is_refused() {
        let mut table = CapabilityTable::new();
        let cap = device_cap(&mut table, 7, 3);
        let mut auth = DmaAuthorizer::new();
        auth.assign_device(table.get(cap).unwrap(), 3).unwrap();
        assert_eq!(
            auth.assign_device(table.get(cap).unwrap(), 3),
            Err(DmaError::DeviceAlreadyAssigned)
        );
    }

    #[test]
    fn a_foreign_process_cannot_fill_the_domain() {
        let mut table = CapabilityTable::new();
        let owner = device_cap(&mut table, 7, 3);
        let stranger = device_cap(&mut table, 8, 3);
        let mut auth = DmaAuthorizer::new();
        let domain = auth.assign_device(table.get(owner).unwrap(), 3).unwrap();
        // Prozess 8 hat eine gueltige Cap auf dasselbe Geraet, aber nicht die
        // Domain von Prozess 7.
        assert_eq!(
            auth.map_window(table.get(stranger).unwrap(), domain, window(0, 4096)),
            Err(DmaError::DomainOfAnotherProcess)
        );
        assert_eq!(auth.stats().denied_foreign_domain, 1);
    }

    #[test]
    fn windows_are_checked_before_they_are_recorded() {
        let mut table = CapabilityTable::new();
        let cap = device_cap(&mut table, 7, 3);
        let mut auth = DmaAuthorizer::new();
        let domain = auth.assign_device(table.get(cap).unwrap(), 3).unwrap();
        let c = table.get(cap).unwrap();
        assert_eq!(
            auth.map_window(c, domain, window(0, 0)),
            Err(DmaError::InvalidRegion)
        );
        assert_eq!(
            auth.map_window(c, domain, window(u64::MAX - 16, 64)),
            Err(DmaError::InvalidRegion)
        );
        assert_eq!(
            auth.map_window(c, domain, window(0, MAX_WINDOW_BYTES + 1)),
            Err(DmaError::RegionTooLarge)
        );
        assert!(auth.domain(domain).unwrap().windows.is_empty());
    }

    #[test]
    fn mapping_and_unmapping_round_trip() {
        let mut table = CapabilityTable::new();
        let cap = device_cap(&mut table, 7, 3);
        let mut auth = DmaAuthorizer::new();
        let domain = auth.assign_device(table.get(cap).unwrap(), 3).unwrap();
        let c = table.get(cap).unwrap();
        let w = window(0x4000, 8192);
        auth.map_window(c, domain, w).unwrap();
        assert_eq!(auth.domain(domain).unwrap().windows.len(), 1);
        assert_eq!(auth.unmap_window(c, domain, 0x4000), Ok(w));
        assert!(auth.domain(domain).unwrap().windows.is_empty());
        // Was nicht eingetragen ist, laesst sich nicht abraeumen.
        assert_eq!(
            auth.unmap_window(c, domain, 0x4000),
            Err(DmaError::DomainNotFound)
        );
    }

    #[test]
    fn domains_are_listed_per_owner() {
        let mut table = CapabilityTable::new();
        let a = device_cap(&mut table, 7, 1);
        let b = device_cap(&mut table, 8, 2);
        let mut auth = DmaAuthorizer::new();
        auth.assign_device(table.get(a).unwrap(), 1).unwrap();
        auth.assign_device(table.get(b).unwrap(), 2).unwrap();
        assert_eq!(auth.domains_of(Pid(7)).len(), 1);
        assert_eq!(auth.domains_of(Pid(8)).len(), 1);
        assert_eq!(auth.domains_of(Pid(9)).len(), 0);
    }
}
