// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// SC-006 REQ-SC006-11 — Geraete-Enumeration im Service Space
// Service Space | Chain-ID 658467
//
// Ein Dienst, der ein Geraet benutzen will, muss es erst finden. Bisher gab es
// dafuer nichts: die Geraeteliste lebte im Kernel, und ein Prozess konnte nur
// raten, welche Kennung er in eine Device-Cap schreiben lassen soll.
//
// Diese Registratur trennt zwei Dinge, die leicht verwechselt werden:
//
//   * **Finden ist offen.** Kennung, Klasse und Name eines Geraets sind
//     Metadaten. Ohne sie koennte kein Dienst je eine Cap anfragen. Wer sie
//     sieht, hat noch nichts gewonnen.
//   * **Besitzen ist geprueft.** Ein Anspruch auf ein Geraet entsteht nur mit
//     einer Device-Cap, die genau dieses Geraet adressiert und `ACCESS` traegt.
//     Ein Geraet hat genau einen Besitzer; ein zweiter Anspruch wird abgelehnt,
//     nicht ueberschrieben.
//
// Dieselbe Regel wie in der DMA-Autorisierung: eine Cap oeffnet genau ihr
// Geraet. Eine Registratur, die beim Anspruch nur "ist die Cap gueltig?" fragt,
// waere ein Generalschluessel fuer alle Geraete.
//
// Was hier NICHT passiert: es wird keine Hardware abgefragt. Die Eintraege
// kommen aus einer Quelle (spaeter ACPI/MADT, heute eine uebergebene Liste).
// Das Einsammeln ist die naechste Stufe, nicht diese.
// ─────────────────────────────────────────────────────────────────────────

use alloc::string::String;
use alloc::vec::Vec;

use shivacore::ats1000::Pid;
use shivacore::capability::{Capability, ResourceType, Rights};

/// Geraeteklassen, wie sie ein Dienst zum Filtern braucht.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeviceClass {
    Storage,
    Network,
    Display,
    Input,
    Audio,
    Usb,
    Gpu,
    Other,
}

impl DeviceClass {
    pub fn as_str(self) -> &'static str {
        match self {
            DeviceClass::Storage => "storage",
            DeviceClass::Network => "network",
            DeviceClass::Display => "display",
            DeviceClass::Input => "input",
            DeviceClass::Audio => "audio",
            DeviceClass::Usb => "usb",
            DeviceClass::Gpu => "gpu",
            DeviceClass::Other => "other",
        }
    }
}

/// Was ueber ein Geraet oeffentlich ist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceEntry {
    pub id: u64,
    pub class: DeviceClass,
    pub name: String,
}

/// Ein Anspruch: wer ein Geraet besitzt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceClaim {
    pub device: u64,
    pub owner: Pid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryError {
    /// Die Cap ist keine Device-Cap.
    NotADeviceCap,
    /// Der Cap fehlt das Zugriffsrecht.
    MissingAccess,
    /// Die Cap gehoert zu einem anderen Geraet.
    WrongDevice,
    /// Diese Kennung ist bereits eingetragen.
    AlreadyRegistered,
    /// Dieses Geraet ist nicht eingetragen.
    UnknownDevice,
    /// Das Geraet gehoert bereits einem anderen Prozess.
    AlreadyClaimed,
    /// Der Anspruch gehoert einem anderen Prozess.
    NotTheOwner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RegistryStats {
    pub registered: u64,
    pub claimed: u64,
    pub denied_no_cap: u64,
    pub denied_foreign_claim: u64,
}

/// Die Registratur.
#[derive(Debug, Default)]
pub struct DeviceRegistry {
    devices: Vec<(DeviceEntry, Option<Pid>)>,
    stats: RegistryStats,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            stats: RegistryStats::default(),
        }
    }

    /// Prueft, ob die Cap genau dieses Geraet mit Zugriffsrecht abdeckt.
    fn check(&mut self, cap: &Capability, device: u64) -> Result<(), RegistryError> {
        if cap.resource_type != ResourceType::Device {
            return Err(RegistryError::NotADeviceCap);
        }
        if !cap.rights.has(Rights::ACCESS) {
            self.stats.denied_no_cap += 1;
            return Err(RegistryError::MissingAccess);
        }
        if cap.resource_id != device {
            self.stats.denied_no_cap += 1;
            return Err(RegistryError::WrongDevice);
        }
        Ok(())
    }

    /// Traegt ein Geraet ein. Wer eintraegt, muss Gewalt ueber genau dieses
    /// Geraet haben — sonst koennte jeder fremde Kennungen anlegen und
    /// Dienste auf ein Geraet zeigen, das ihm nicht gehoert.
    pub fn register(&mut self, cap: &Capability, entry: DeviceEntry) -> Result<(), RegistryError> {
        self.check(cap, entry.id)?;
        if self.devices.iter().any(|(e, _)| e.id == entry.id) {
            return Err(RegistryError::AlreadyRegistered);
        }
        self.devices.push((entry, None));
        self.devices.sort_by_key(|(e, _)| e.id);
        self.stats.registered += 1;
        Ok(())
    }

    /// Nimmt ein Geraet heraus. Nur der Besitzer oder eine Cap auf genau
    /// dieses Geraet darf das; ein fremder Prozess nicht.
    pub fn remove(&mut self, cap: &Capability, device: u64) -> Result<DeviceEntry, RegistryError> {
        self.check(cap, device)?;
        let index = self
            .devices
            .iter()
            .position(|(e, _)| e.id == device)
            .ok_or(RegistryError::UnknownDevice)?;
        if let Some(owner) = self.devices[index].1 {
            if owner != cap.owner {
                self.stats.denied_foreign_claim += 1;
                return Err(RegistryError::NotTheOwner);
            }
        }
        Ok(self.devices.remove(index).0)
    }

    /// Alle Geraete, nach Kennung sortiert. Oeffentlich: ohne diese Liste
    /// koennte kein Dienst je eine Cap anfragen.
    pub fn list(&self) -> Vec<&DeviceEntry> {
        self.devices.iter().map(|(e, _)| e).collect()
    }

    /// Nur eine Klasse.
    pub fn list_class(&self, class: DeviceClass) -> Vec<&DeviceEntry> {
        self.devices
            .iter()
            .map(|(e, _)| e)
            .filter(|e| e.class == class)
            .collect()
    }

    /// Ein Geraet ansehen.
    pub fn get(&self, device: u64) -> Option<&DeviceEntry> {
        self.devices
            .iter()
            .find(|(e, _)| e.id == device)
            .map(|(e, _)| e)
    }

    /// Ein Geraet beanspruchen. Das ist der Punkt, an dem Rechte zaehlen.
    pub fn claim(&mut self, cap: &Capability, device: u64) -> Result<DeviceClaim, RegistryError> {
        self.check(cap, device)?;
        let index = self
            .devices
            .iter()
            .position(|(e, _)| e.id == device)
            .ok_or(RegistryError::UnknownDevice)?;
        match self.devices[index].1 {
            Some(owner) if owner != cap.owner => {
                self.stats.denied_foreign_claim += 1;
                Err(RegistryError::AlreadyClaimed)
            }
            Some(_) => Ok(DeviceClaim {
                device,
                owner: cap.owner,
            }),
            None => {
                self.devices[index].1 = Some(cap.owner);
                self.stats.claimed += 1;
                Ok(DeviceClaim {
                    device,
                    owner: cap.owner,
                })
            }
        }
    }

    /// Einen Anspruch freigeben. Nur der Besitzer.
    pub fn release(&mut self, cap: &Capability, device: u64) -> Result<(), RegistryError> {
        self.check(cap, device)?;
        let index = self
            .devices
            .iter()
            .position(|(e, _)| e.id == device)
            .ok_or(RegistryError::UnknownDevice)?;
        match self.devices[index].1 {
            Some(owner) if owner == cap.owner => {
                self.devices[index].1 = None;
                Ok(())
            }
            Some(_) => {
                self.stats.denied_foreign_claim += 1;
                Err(RegistryError::NotTheOwner)
            }
            None => Err(RegistryError::NotTheOwner),
        }
    }

    pub fn owner_of(&self, device: u64) -> Option<Pid> {
        self.devices
            .iter()
            .find(|(e, _)| e.id == device)
            .and_then(|(_, owner)| *owner)
    }

    pub fn stats(&self) -> RegistryStats {
        self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shivacore::capability::CapabilityTable;

    fn cap(table: &mut CapabilityTable, owner: u32, device: u64) -> shivacore::capability::CapId {
        table
            .create_typed(Pid(owner), ResourceType::Device, device, Rights::ACCESS)
            .expect("Device-Cap")
    }

    fn entry(id: u64, class: DeviceClass, name: &str) -> DeviceEntry {
        DeviceEntry {
            id,
            class,
            name: String::from(name),
        }
    }

    #[test]
    fn registering_requires_authority_over_exactly_that_device() {
        let mut table = CapabilityTable::new();
        let c = cap(&mut table, 5, 3);
        let other = cap(&mut table, 5, 4);
        let mut reg = DeviceRegistry::new();
        assert_eq!(
            reg.register(
                table.get(other).unwrap(),
                entry(3, DeviceClass::Input, "kbd")
            ),
            Err(RegistryError::WrongDevice)
        );
        assert!(reg.list().is_empty());
        assert!(reg
            .register(table.get(c).unwrap(), entry(3, DeviceClass::Input, "kbd"))
            .is_ok());
    }

    #[test]
    fn a_memory_cap_cannot_register_a_device() {
        let mut table = CapabilityTable::new();
        let mem = table
            .create_typed(Pid(5), ResourceType::Memory, 3, Rights::READ)
            .unwrap();
        let mut reg = DeviceRegistry::new();
        assert_eq!(
            reg.register(table.get(mem).unwrap(), entry(3, DeviceClass::Input, "kbd")),
            Err(RegistryError::NotADeviceCap)
        );
    }

    #[test]
    fn a_duplicate_id_is_refused_not_overwritten() {
        let mut table = CapabilityTable::new();
        let c = cap(&mut table, 5, 3);
        let mut reg = DeviceRegistry::new();
        reg.register(table.get(c).unwrap(), entry(3, DeviceClass::Input, "kbd"))
            .unwrap();
        assert_eq!(
            reg.register(
                table.get(c).unwrap(),
                entry(3, DeviceClass::Display, "fremd")
            ),
            Err(RegistryError::AlreadyRegistered)
        );
        assert_eq!(reg.get(3).unwrap().class, DeviceClass::Input);
    }

    #[test]
    fn the_listing_is_sorted_and_filterable() {
        let mut table = CapabilityTable::new();
        let mut reg = DeviceRegistry::new();
        for (id, class, name) in [
            (9u64, DeviceClass::Network, "nic"),
            (3, DeviceClass::Input, "kbd"),
            (7, DeviceClass::Storage, "ssd"),
        ] {
            let c = cap(&mut table, 5, id);
            reg.register(table.get(c).unwrap(), entry(id, class, name))
                .unwrap();
        }
        let ids: Vec<u64> = reg.list().iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![3, 7, 9]);
        assert_eq!(reg.list_class(DeviceClass::Input).len(), 1);
        assert_eq!(reg.list_class(DeviceClass::Gpu).len(), 0);
    }

    #[test]
    fn claiming_needs_a_cap_for_that_device() {
        let mut table = CapabilityTable::new();
        let c = cap(&mut table, 5, 3);
        let wrong = cap(&mut table, 5, 9);
        let mut reg = DeviceRegistry::new();
        reg.register(table.get(c).unwrap(), entry(3, DeviceClass::Input, "kbd"))
            .unwrap();
        assert_eq!(
            reg.claim(table.get(wrong).unwrap(), 3),
            Err(RegistryError::WrongDevice)
        );
        assert_eq!(reg.owner_of(3), None);
        assert_eq!(
            reg.claim(table.get(c).unwrap(), 3),
            Ok(DeviceClaim {
                device: 3,
                owner: Pid(5)
            })
        );
        assert_eq!(reg.owner_of(3), Some(Pid(5)));
    }

    #[test]
    fn a_second_owner_is_refused() {
        let mut table = CapabilityTable::new();
        let mine = cap(&mut table, 5, 3);
        let yours = cap(&mut table, 6, 3);
        let mut reg = DeviceRegistry::new();
        reg.register(
            table.get(mine).unwrap(),
            entry(3, DeviceClass::Input, "kbd"),
        )
        .unwrap();
        reg.claim(table.get(mine).unwrap(), 3).unwrap();
        assert_eq!(
            reg.claim(table.get(yours).unwrap(), 3),
            Err(RegistryError::AlreadyClaimed)
        );
        assert_eq!(reg.owner_of(3), Some(Pid(5)));
        assert_eq!(reg.stats().denied_foreign_claim, 1);
    }

    #[test]
    fn releasing_frees_the_device_for_the_next_owner() {
        let mut table = CapabilityTable::new();
        let mine = cap(&mut table, 5, 3);
        let yours = cap(&mut table, 6, 3);
        let mut reg = DeviceRegistry::new();
        reg.register(
            table.get(mine).unwrap(),
            entry(3, DeviceClass::Input, "kbd"),
        )
        .unwrap();
        reg.claim(table.get(mine).unwrap(), 3).unwrap();
        assert_eq!(
            reg.release(table.get(yours).unwrap(), 3),
            Err(RegistryError::NotTheOwner)
        );
        reg.release(table.get(mine).unwrap(), 3).unwrap();
        assert_eq!(reg.owner_of(3), None);
        assert!(reg.claim(table.get(yours).unwrap(), 3).is_ok());
        assert_eq!(reg.owner_of(3), Some(Pid(6)));
    }

    #[test]
    fn removing_a_device_requires_the_owner() {
        let mut table = CapabilityTable::new();
        let mine = cap(&mut table, 5, 3);
        let yours = cap(&mut table, 6, 3);
        let mut reg = DeviceRegistry::new();
        reg.register(
            table.get(mine).unwrap(),
            entry(3, DeviceClass::Input, "kbd"),
        )
        .unwrap();
        reg.claim(table.get(mine).unwrap(), 3).unwrap();
        assert_eq!(
            reg.remove(table.get(yours).unwrap(), 3),
            Err(RegistryError::NotTheOwner)
        );
        assert!(reg.get(3).is_some());
        assert_eq!(
            reg.remove(table.get(mine).unwrap(), 3).unwrap().name,
            String::from("kbd")
        );
        assert!(reg.get(3).is_none());
        assert_eq!(
            reg.claim(table.get(mine).unwrap(), 3),
            Err(RegistryError::UnknownDevice)
        );
    }

    #[test]
    fn the_stats_separate_refusal_from_never_asked() {
        let mut table = CapabilityTable::new();
        let c = cap(&mut table, 5, 3);
        let mut reg = DeviceRegistry::new();
        reg.register(table.get(c).unwrap(), entry(3, DeviceClass::Input, "kbd"))
            .unwrap();
        assert_eq!(reg.stats().registered, 1);
        assert_eq!(reg.stats().denied_no_cap, 0);
        assert_eq!(reg.stats().claimed, 0);
    }
}
