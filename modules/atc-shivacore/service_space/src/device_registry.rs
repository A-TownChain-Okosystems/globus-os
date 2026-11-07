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
    /// Zaehlt, was ueber die Enum-Quelle kam. Getrennt von der Cap-Pruefung,
    /// damit im Betrieb sichtbar bleibt, wie viel ohne Cap hereinkam.
    registered_without_cap: u64,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            stats: RegistryStats::default(),
            registered_without_cap: 0,
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

// ── Uebergang von der Enum-Quelle zur Registratur ────────────────────────
//
// Der Kernel findet Geraete (`shivacore::pci_scan`), der Service Space verwaltet
// sie. Die Crate-Grenze laeuft in genau eine Richtung — der Service Space baut
// auf dem Kernel auf —, also liegt die Uebersetzung hier und nicht dort.

use libshivacore::acpi_devices::{AcpiDevice, AcpiDeviceKind};
use shivacore::pci_scan::{DeviceKind, PciDevice};

impl DeviceClass {
    /// Gattung aus der PCI-Entdeckung uebersetzen.
    ///
    /// `Gpu` bleibt absichtlich aussen vor: die PCI-Klasse 0x03 heisst "Display"
    /// und deckt Bildschirm wie Beschleuniger ab. Ob ein Geraet eine GPU ist,
    /// weiss erst der Treiber, nicht der Klassencode.
    pub fn from_kind(kind: DeviceKind) -> Self {
        match kind {
            DeviceKind::Storage => DeviceClass::Storage,
            DeviceKind::Network => DeviceClass::Network,
            DeviceKind::Display => DeviceClass::Display,
            DeviceKind::Input => DeviceClass::Input,
            DeviceKind::Audio => DeviceClass::Audio,
            DeviceKind::Usb => DeviceClass::Usb,
            DeviceKind::Bridge => DeviceClass::Other,
            DeviceKind::Other => DeviceClass::Other,
        }
    }

    /// Der Weg zurueck, fuer Rundlauf-Pruefungen.
    pub fn to_kind(self) -> DeviceKind {
        match self {
            DeviceClass::Storage => DeviceKind::Storage,
            DeviceClass::Network => DeviceKind::Network,
            DeviceClass::Display | DeviceClass::Gpu => DeviceKind::Display,
            DeviceClass::Input => DeviceKind::Input,
            DeviceClass::Audio => DeviceKind::Audio,
            DeviceClass::Usb => DeviceKind::Usb,
            DeviceClass::Other => DeviceKind::Other,
        }
    }
}

/// Kennung, unter der ein PCI-Geraet in der Registratur steht.
///
/// Aus der Hardware abgeleitet, nicht fortlaufend vergeben: Bus, Geraet und
/// Funktion passen in 24 Bit. Damit ist die Kennung stabil ueber Neustarts und
/// zwei Laeufe derselben Maschine liefern dieselben Kennungen — sonst waere jede
/// gespeicherte Device-Cap nach dem naechsten Boot wertlos.
pub fn pci_device_id(device: &PciDevice) -> u64 {
    ((device.address.bus as u64) << 16)
        | ((device.address.device as u64) << 8)
        | device.address.function as u64
}

/// Ein gefundenes Geraet als Registratur-Eintrag.
pub fn entry_from_hardware(device: &PciDevice) -> DeviceEntry {
    DeviceEntry {
        id: pci_device_id(device),
        class: DeviceClass::from_kind(device.kind()),
        name: alloc::format!(
            "pci {:02x}:{:02x}.{} {:04x}:{:04x}",
            device.address.bus,
            device.address.device,
            device.address.function,
            device.vendor,
            device.device
        ),
    }
}

/// Kennungsraum fuer ACPI-Geraete.
///
/// Muss sich von den PCI-Kennungen fernhalten: die liegen bei
/// `bus<<16 | geraet<<8 | funktion`, also unter 2^24. ACPI beginnt deshalb
/// weit darueber, damit eine Device-Cap nie versehentlich das falsche Geraet
/// trifft.
pub const ACPI_ID_BASE: u64 = 0xAC00_0000_0000_0000;

/// Art-Kennung eines ACPI-Geraets innerhalb der Kennung.
fn acpi_kind_code(kind: AcpiDeviceKind) -> u64 {
    match kind {
        AcpiDeviceKind::LocalApic => 1,
        AcpiDeviceKind::IoApic => 2,
        AcpiDeviceKind::Hpet => 3,
    }
}

/// Stabile Kennung eines ACPI-Geraets.
///
/// Anders als beim PCI-Geraet gibt es keine Bus/Geraet/Funktion-Kennung. Die
/// Kennung traegt deshalb Art und Basisadresse: dieselbe Maschine liefert beim
/// naechsten Boot dieselbe Zahl, sonst waere eine gespeicherte Device-Cap
/// wertlos.
pub fn acpi_device_id(device: &AcpiDevice) -> u64 {
    ACPI_ID_BASE | (acpi_kind_code(device.kind) << 32) | (device.address & 0xFFFF_FFFF)
}

/// Ein ACPI-Geraet als Registratur-Eintrag.
///
/// Klasse `Other`: Local APIC, IOAPIC und HPET sind keiner der
/// Geraeteklassen zuzuordnen, die ein Dienst zum Filtern braucht. Sie als
/// `Storage` oder `Input` zu fuehren waere bequem und falsch.
pub fn entry_from_acpi(device: &AcpiDevice) -> DeviceEntry {
    DeviceEntry {
        id: acpi_device_id(device),
        class: DeviceClass::Other,
        name: alloc::format!("acpi {} @{:#x}", device.name(), device.address),
    }
}

/// Ergebnis eines Abgleichs mit der Enum-Quelle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiscoveryOutcome {
    pub adopted: u64,
    pub already_known: u64,
}

impl DeviceRegistry {
    /// Ein gefundenes Geraet uebernehmen.
    ///
    /// Der Unterschied zu `register`: hier prueft keine Cap. Das ist Absicht.
    /// Der Kernel hat Gewalt ueber die Geraete, weil er sie gefunden hat — er
    /// liest den Konfigurationsraum, ein Prozess kann das nicht. `register`
    /// bleibt der Weg fuer Prozesse und prueft weiterhin die Device-Cap.
    ///
    /// Wer diese Funktion aus dem Userspace erreichbar macht, hebt die
    /// Rechtepruefung auf. Sie gehoert ausschliesslich in den Aufruf, in dem die
    /// Enum-Quelle laeuft.
    ///
    /// Ein uebernommenes Geraet hat **keinen Besitzer**. Finden ist offen,
    /// Besitzen bleibt geprueft: der erste `claim` mit passender Cap entscheidet.
    pub fn adopt_discovered(&mut self, device: &PciDevice) -> Result<DeviceEntry, RegistryError> {
        let entry = entry_from_hardware(device);
        if self.devices.iter().any(|(e, _)| e.id == entry.id) {
            return Err(RegistryError::AlreadyRegistered);
        }
        self.registered_without_cap += 1;
        self.devices.push((entry.clone(), None));
        self.devices.sort_by_key(|(e, _)| e.id);
        self.stats.registered += 1;
        Ok(entry)
    }

    /// Alle gefundenen Geraete uebernehmen, was schon da ist wird gezaehlt
    /// statt doppelt eingetragen. Damit ist der Abgleich wiederholbar: ein
    /// zweiter Boot mit derselben Hardware aendert nichts.
    pub fn sync_from_hardware(&mut self, devices: &[PciDevice]) -> DiscoveryOutcome {
        let mut outcome = DiscoveryOutcome::default();
        for device in devices {
            match self.adopt_discovered(device) {
                Ok(_) => outcome.adopted += 1,
                Err(RegistryError::AlreadyRegistered) => outcome.already_known += 1,
                Err(_) => {}
            }
        }
        outcome
    }

    /// Den Geraetevorrat des Bootlaufs uebernehmen.
    ///
    /// Das ist der Anschluss, der bisher fehlte: der Kernel hat die Geraete
    /// gefunden, aber niemand konnte sie zur Laufzeit abfragen. `sync_from_store`
    /// nimmt einen beliebigen Vorrat (testbar), `sync_from_boot` den des
    /// laufenden Systems.
    pub fn sync_from_store(
        &mut self,
        store: &shivacore::boot_devices::DeviceStore,
    ) -> DiscoveryOutcome {
        self.sync_from_hardware(&store.snapshot())
    }

    /// Den Vorrat des laufenden Systems uebernehmen. Der Weg, den der Service
    /// Space beim Start geht.
    pub fn sync_from_boot(&mut self) -> DiscoveryOutcome {
        self.sync_from_store(&shivacore::boot_devices::BOOT_DEVICES)
    }

    /// Ein ACPI-Geraet uebernehmen. Gleiche Regel wie bei `adopt_discovered`:
    /// finden vergibt keine Rechte, ein uebernommenes Geraet hat keinen
    /// Besitzer, und die Cap-Pruefung bleibt bei `claim`.
    pub fn adopt_discovered_acpi(
        &mut self,
        device: &AcpiDevice,
    ) -> Result<DeviceEntry, RegistryError> {
        let entry = entry_from_acpi(device);
        if self.devices.iter().any(|(e, _)| e.id == entry.id) {
            return Err(RegistryError::AlreadyRegistered);
        }
        self.registered_without_cap += 1;
        self.devices.push((entry.clone(), None));
        self.devices.sort_by_key(|(e, _)| e.id);
        self.stats.registered += 1;
        Ok(entry)
    }

    /// Alle ACPI-Geraete uebernehmen, Bekanntes wird gezaehlt statt doppelt
    /// eingetragen. Damit ist der Abgleich wiederholbar.
    pub fn sync_from_acpi(&mut self, devices: &[AcpiDevice]) -> DiscoveryOutcome {
        let mut outcome = DiscoveryOutcome::default();
        for device in devices {
            match self.adopt_discovered_acpi(device) {
                Ok(_) => outcome.adopted += 1,
                Err(RegistryError::AlreadyRegistered) => outcome.already_known += 1,
                Err(_) => {}
            }
        }
        outcome
    }

    /// Den ACPI-Vorrat uebernehmen (`sync_from_store` fuer die zweite Quelle).
    pub fn sync_acpi_from_store(
        &mut self,
        store: &shivacore::boot_devices::AcpiDeviceStore,
    ) -> DiscoveryOutcome {
        self.sync_from_acpi(&store.snapshot())
    }

    /// Den ACPI-Vorrat des laufenden Systems uebernehmen. Das ist der
    /// Anschluss an den Boot: der Kernel legt LAPIC, IOAPIC und HPET in
    /// `BOOT_ACPI_DEVICES` ab, hier werden sie abholbar.
    pub fn sync_acpi_from_boot(&mut self) -> DiscoveryOutcome {
        self.sync_acpi_from_store(&shivacore::boot_devices::BOOT_ACPI_DEVICES)
    }

    /// Wie viele Eintraege ohne Cap-Prüfung uebernommen wurden. Steht nicht in
    /// `RegistryStats`, weil das eine andere Frage ist: nicht "wer wurde
    /// abgelehnt", sondern "wie viel kam ungeprueft herein".
    pub fn discovered_without_cap(&self) -> u64 {
        self.registered_without_cap
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

    // ── Uebergang von der Enum-Quelle ────────────────────────────────────

    fn pci(
        bus: u8,
        device: u8,
        function: u8,
        vendor: u16,
        id: u16,
        class: u8,
        subclass: u8,
    ) -> PciDevice {
        PciDevice {
            address: shivacore::pci_scan::PciAddress::new(bus, device, function),
            vendor,
            device: id,
            class,
            subclass,
            header_type: 0x00,
        }
    }

    #[test]
    fn the_identifier_comes_from_the_hardware_not_from_a_counter() {
        let a = pci(0, 2, 0, 0x1234, 0x1111, 0x03, 0x00);
        let b = pci(0, 2, 1, 0x1234, 0x1111, 0x03, 0x00);
        let c = pci(1, 2, 0, 0x1234, 0x1111, 0x03, 0x00);
        // Zwei Laeufe derselben Maschine liefern dieselbe Kennung ...
        assert_eq!(pci_device_id(&a), pci_device_id(&a));
        // ... und verschiedene Stellen verschiedene.
        assert_ne!(pci_device_id(&a), pci_device_id(&b));
        assert_ne!(pci_device_id(&a), pci_device_id(&c));
        assert_eq!(pci_device_id(&a), (2 << 8));
    }

    #[test]
    fn every_discovered_kind_lands_in_the_right_class() {
        for (class, subclass, expected) in [
            (0x01u8, 0x08u8, DeviceClass::Storage),
            (0x02, 0x00, DeviceClass::Network),
            (0x03, 0x00, DeviceClass::Display),
            (0x09, 0x00, DeviceClass::Input),
            (0x04, 0x01, DeviceClass::Audio),
            (0x0C, 0x03, DeviceClass::Usb),
            (0x06, 0x01, DeviceClass::Other), // Bruecken sind keine Dienstgeraete
            (0xFF, 0x00, DeviceClass::Other),
        ] {
            let entry = entry_from_hardware(&pci(0, 3, 0, 0x8086, 0x1234, class, subclass));
            assert_eq!(entry.class, expected, "Klasse {class:#04x}/{subclass:#04x}");
        }
    }

    #[test]
    fn the_name_carries_the_address_a_human_needs() {
        let entry = entry_from_hardware(&pci(0, 2, 0, 0x1234, 0x1111, 0x03, 0x00));
        assert_eq!(
            entry.name,
            alloc::string::String::from("pci 00:02.0 1234:1111")
        );
    }

    #[test]
    fn adopting_twice_does_not_duplicate() {
        let device = pci(0, 2, 0, 0x1234, 0x1111, 0x03, 0x00);
        let mut reg = DeviceRegistry::new();
        assert!(reg.adopt_discovered(&device).is_ok());
        assert_eq!(
            reg.adopt_discovered(&device),
            Err(RegistryError::AlreadyRegistered)
        );
        assert_eq!(reg.list().len(), 1);
    }

    #[test]
    fn a_discovered_device_has_no_owner() {
        let mut reg = DeviceRegistry::new();
        let entry = reg
            .adopt_discovered(&pci(0, 2, 0, 0x1234, 0x1111, 0x03, 0x00))
            .unwrap();
        // Finden ist offen, Besitzen bleibt geprueft.
        assert_eq!(reg.owner_of(entry.id), None);
        assert_eq!(reg.stats().claimed, 0);
    }

    #[test]
    fn discovery_does_not_open_the_device_for_anyone() {
        let mut table = CapabilityTable::new();
        let mem = table
            .create_typed(Pid(5), ResourceType::Memory, 2 << 8, Rights::ACCESS)
            .unwrap();
        let mut reg = DeviceRegistry::new();
        let entry = reg
            .adopt_discovered(&pci(0, 2, 0, 0x1234, 0x1111, 0x03, 0x00))
            .unwrap();
        // Der Eintrag ist sichtbar ...
        assert!(reg.get(entry.id).is_some());
        // ... aber eine Memory-Cap oeffnet ihn nicht.
        assert_eq!(
            reg.claim(table.get(mem).unwrap(), entry.id),
            Err(RegistryError::NotADeviceCap)
        );
        assert_eq!(reg.owner_of(entry.id), None);
    }

    #[test]
    fn a_second_sync_with_the_same_hardware_changes_nothing() {
        let devices = vec![
            pci(0, 2, 0, 0x1234, 0x1111, 0x03, 0x00),
            pci(0, 3, 0, 0x8086, 0x100E, 0x02, 0x00),
        ];
        let mut reg = DeviceRegistry::new();
        let first = reg.sync_from_hardware(&devices);
        assert_eq!(first.adopted, 2);
        assert_eq!(first.already_known, 0);
        let second = reg.sync_from_hardware(&devices);
        assert_eq!(second.adopted, 0);
        assert_eq!(second.already_known, 2);
        assert_eq!(reg.list().len(), 2);
        assert_eq!(reg.discovered_without_cap(), 2);
    }

    #[test]
    fn the_class_mapping_round_trips_except_for_gpu() {
        for class in [
            DeviceClass::Storage,
            DeviceClass::Network,
            DeviceClass::Display,
            DeviceClass::Input,
            DeviceClass::Audio,
            DeviceClass::Usb,
            DeviceClass::Other,
        ] {
            assert_eq!(DeviceClass::from_kind(class.to_kind()), class);
        }
        // Eine GPU ist am PCI-Klassencode nicht zu erkennen; sie kommt als
        // Display zurueck. Das ist die ehrliche Antwort, keine Luecke.
        assert_eq!(
            DeviceClass::from_kind(DeviceClass::Gpu.to_kind()),
            DeviceClass::Display
        );
    }

    #[test]
    fn a_synced_device_can_be_claimed_with_the_matching_cap() {
        let mut table = CapabilityTable::new();
        let mut reg = DeviceRegistry::new();
        let entry = reg
            .adopt_discovered(&pci(0, 3, 0, 0x8086, 0x100E, 0x02, 0x00))
            .unwrap();
        let cap = cap(&mut table, 7, entry.id);
        assert_eq!(
            reg.claim(table.get(cap).unwrap(), entry.id).unwrap().owner,
            Pid(7)
        );
        assert_eq!(reg.owner_of(entry.id), Some(Pid(7)));
    }

    #[test]
    fn the_boot_store_is_the_missing_link() {
        let store = shivacore::boot_devices::DeviceStore::new();
        store.publish(alloc::vec![
            pci(0, 2, 0, 0x1234, 0x1111, 0x03, 0x00),
            pci(0, 3, 0, 0x8086, 0x100E, 0x02, 0x00),
        ]);
        let mut reg = DeviceRegistry::new();
        let outcome = reg.sync_from_store(&store);
        assert_eq!(outcome.adopted, 2);
        assert_eq!(reg.list().len(), 2);
        // Und der zweite Start mit demselben Vorrat aendert nichts.
        let again = reg.sync_from_store(&store);
        assert_eq!(again.adopted, 0);
        assert_eq!(again.already_known, 2);
    }

    #[test]
    fn an_empty_store_leaves_the_registry_untouched() {
        let store = shivacore::boot_devices::DeviceStore::new();
        let mut reg = DeviceRegistry::new();
        let outcome = reg.sync_from_store(&store);
        assert_eq!(outcome, DiscoveryOutcome::default());
        assert!(reg.list().is_empty());
    }

    fn acpi(kind: AcpiDeviceKind, address: u64) -> AcpiDevice {
        AcpiDevice {
            kind,
            address,
            gsi_base: None,
        }
    }

    #[test]
    fn an_acpi_id_stays_clear_of_the_pci_range() {
        // PCI kommt ueber bus<<16|geraet<<8|funktion nie ueber 2^24 hinaus.
        let highest_pci = 0x00FF_FFFFu64;
        let lowest_acpi = acpi_device_id(&acpi(AcpiDeviceKind::LocalApic, 0));
        assert!(
            lowest_acpi > highest_pci,
            "Kennungsraeume duerfen sich nicht schneiden"
        );
    }

    #[test]
    fn an_acpi_id_is_stable_across_boots() {
        let first = acpi_device_id(&acpi(AcpiDeviceKind::Hpet, 0xFED0_0000));
        let second = acpi_device_id(&acpi(AcpiDeviceKind::Hpet, 0xFED0_0000));
        assert_eq!(first, second, "gleiche Maschine, gleiche Kennung");
    }

    #[test]
    fn different_acpi_kinds_get_different_ids_even_at_one_address() {
        let a = acpi_device_id(&acpi(AcpiDeviceKind::LocalApic, 0xFEE0_0000));
        let b = acpi_device_id(&acpi(AcpiDeviceKind::IoApic, 0xFEE0_0000));
        assert_ne!(a, b);
    }

    #[test]
    fn acpi_entries_are_named_and_classified_honestly() {
        let entry = entry_from_acpi(&acpi(AcpiDeviceKind::Hpet, 0xFED0_0000));
        assert_eq!(entry.class, DeviceClass::Other);
        assert_eq!(entry.name, "acpi HPET @0xfed00000");
    }

    #[test]
    fn syncing_acpi_adopts_and_counts_repeats() {
        let devices = alloc::vec![
            acpi(AcpiDeviceKind::LocalApic, 0xFEE0_0000),
            acpi(AcpiDeviceKind::IoApic, 0xFEC0_0000),
            acpi(AcpiDeviceKind::Hpet, 0xFED0_0000),
        ];
        let mut reg = DeviceRegistry::new();
        assert_eq!(
            reg.sync_from_acpi(&devices),
            DiscoveryOutcome {
                adopted: 3,
                already_known: 0
            }
        );
        assert_eq!(reg.list().len(), 3);
        // Zweiter Lauf mit derselben Hardware darf nichts verdoppeln.
        assert_eq!(
            reg.sync_from_acpi(&devices),
            DiscoveryOutcome {
                adopted: 0,
                already_known: 3
            }
        );
        assert_eq!(reg.list().len(), 3);
    }

    #[test]
    fn an_adopted_acpi_device_still_has_no_owner() {
        let mut reg = DeviceRegistry::new();
        reg.sync_from_acpi(&alloc::vec![acpi(AcpiDeviceKind::Hpet, 0xFED0_0000)]);
        let entry = reg.list()[0];
        assert_eq!(
            reg.owner_of(entry.id),
            None,
            "finden ist offen, besitzen bleibt geprueft"
        );
    }

    #[test]
    fn syncing_from_an_acpi_store_works_like_the_pci_store() {
        let store = shivacore::boot_devices::AcpiDeviceStore::new();
        store.publish(alloc::vec![acpi(AcpiDeviceKind::LocalApic, 0xFEE0_0000)]);
        let mut reg = DeviceRegistry::new();
        let outcome = reg.sync_acpi_from_store(&store);
        assert_eq!(outcome.adopted, 1);
        assert_eq!(reg.list().len(), 1);
    }

    #[test]
    fn an_empty_acpi_store_changes_nothing() {
        let store = shivacore::boot_devices::AcpiDeviceStore::new();
        let mut reg = DeviceRegistry::new();
        assert_eq!(
            reg.sync_acpi_from_store(&store),
            DiscoveryOutcome::default()
        );
        assert!(reg.list().is_empty());
    }

    #[test]
    fn pci_and_acpi_live_in_one_registry_without_colliding() {
        let mut reg = DeviceRegistry::new();
        reg.sync_from_hardware(&alloc::vec![PciDevice {
            address: shivacore::pci_scan::PciAddress::new(0, 2, 0),
            vendor: 0x8086,
            device: 0x1234,
            class: 0x03,
            subclass: 0x00,
            header_type: 0x00,
        }]);
        reg.sync_from_acpi(&alloc::vec![acpi(AcpiDeviceKind::Hpet, 0xFED0_0000)]);
        assert_eq!(reg.list().len(), 2);
        let ids: Vec<u64> = reg.list().iter().map(|entry| entry.id).collect();
        assert_ne!(ids[0], ids[1]);
    }
}
