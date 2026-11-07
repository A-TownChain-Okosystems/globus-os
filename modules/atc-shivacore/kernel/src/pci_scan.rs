// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// SC-006 REQ-SC006-11 — Enum-Quelle: PCI-Geraete am Bus finden
//
// Die Geraete-Registratur im Service Space kann Geraete verwalten, aber sie
// erfindet keine. Bisher kam ihre Liste von aussen; diese Datei ist die Quelle:
// sie liest den PCI-Konfigurationsraum und liefert, was tatsaechlich steckt.
//
// Gelesen wird ueber den klassischen Mechanismus (IO-Ports 0xCF8/0xCFC). Das
// ist der kleinste Zugriffsweg, der ohne ACPI- und ohne MMIO-Kenntnisse
// funktioniert, und genau deshalb ist er der richtige erste Schritt: er
// liefert eine belastbare Liste, bevor irgendetwas gemappt wird.
//
// Was hier bewusst NICHT passiert:
//   * keine BAR-Aufloesung, kein MMIO-Mapping
//   * keine Treiberbindung
//   * keine Rechtevergabe — wer ein gefundenes Geraet benutzen darf, entscheidet
//     die Device-Cap, nicht die Entdeckung
//
// Finden ist offen, Besitzen ist geprueft. Das ist dieselbe Linie wie in der
// Registratur, nur eine Stufe frueher.
// ─────────────────────────────────────────────────────────────────────────

use alloc::vec::Vec;

/// Konfigurationsraum-Adresse eines Geraets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PciAddress {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

impl PciAddress {
    pub const fn new(bus: u8, device: u8, function: u8) -> Self {
        Self {
            bus,
            device,
            function,
        }
    }

    pub fn slot(&self) -> u16 {
        ((self.bus as u16) << 8) | ((self.device as u16) << 3) | self.function as u16
    }
}

/// Was ein Geraet ueber sich preisgibt, bevor ein Treiber es anfasst.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciDevice {
    pub address: PciAddress,
    pub vendor: u16,
    pub device: u16,
    pub class: u8,
    pub subclass: u8,
    /// 0 = einzelne Funktion, 0x80-Bit = Mehrfunktionsgeraet.
    pub header_type: u8,
}

impl PciDevice {
    /// Ist das eine Bruecke zu einem weiteren Bus?
    ///
    /// 0x06/0x04 ist die gewoehnliche PCI-zu-PCI-Bruecke, 0x06/0x09 die
    /// halbtransparente Variante und 0x06/0x07 die CardBus-Bruecke. Alle drei
    /// tragen im Kopf einen sekundaeren Bus, dem der Scan folgen muss.
    pub fn is_pci_bridge(&self) -> bool {
        self.class == 0x06 && matches!(self.subclass, 0x04 | 0x07 | 0x09)
    }

    pub fn is_multifunction(&self) -> bool {
        self.header_type & 0x80 != 0
    }

    /// Grobe Gattung, wie sie die Registratur braucht.
    pub fn kind(&self) -> DeviceKind {
        DeviceKind::from_class(self.class, self.subclass)
    }
}

/// Gattung nach PCI-Klassencode (Abschnitt 2.3/Anhang D der PCI-Spezifikation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeviceKind {
    Storage,
    Network,
    Display,
    Input,
    Audio,
    Usb,
    Bridge,
    Other,
}

impl DeviceKind {
    pub fn from_class(class: u8, subclass: u8) -> Self {
        match class {
            0x01 => DeviceKind::Storage,
            0x02 => DeviceKind::Network,
            0x03 => DeviceKind::Display,
            // 0x04 ist "Multimedia": Audio ist 0x0401, Video 0x0400.
            // Nur Audio als Audio gelten lassen, sonst waere jedes
            // Video-Capture-Geraet ein Lautsprecher.
            0x04 if subclass == 0x01 => DeviceKind::Audio,
            0x04 => DeviceKind::Other,
            0x06 => DeviceKind::Bridge,
            0x09 => DeviceKind::Input,
            // 0x0C03 = USB-Controller.
            0x0C if subclass == 0x03 => DeviceKind::Usb,
            _ => DeviceKind::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DeviceKind::Storage => "storage",
            DeviceKind::Network => "network",
            DeviceKind::Display => "display",
            DeviceKind::Input => "input",
            DeviceKind::Audio => "audio",
            DeviceKind::Usb => "usb",
            DeviceKind::Bridge => "bridge",
            DeviceKind::Other => "other",
        }
    }
}

/// Zugriff auf den Konfigurationsraum. Als Trait, damit der Test ohne Hardware
/// denselben Code durchlaufen kann wie der Boot.
pub trait ConfigSpace {
    fn read_u32(&mut self, address: PciAddress, offset: u8) -> u32;
}

/// Der echte Weg: IO-Ports 0xCF8 (Adresse) und 0xCFC (Daten).
/// Nur im Boot-Build vorhanden — die Lib-Tests laufen ohne `x86_64` und
/// pruefen dieselbe Logik ueber `ConfigSpace`.
#[cfg(feature = "x86-boot")]
pub struct IoConfigSpace;

#[cfg(feature = "x86-boot")]

impl ConfigSpace for IoConfigSpace {
    fn read_u32(&mut self, address: PciAddress, offset: u8) -> u32 {
        use x86_64::instructions::port::{Port, PortWriteOnly};
        // Adressregister: Bit 31 = aktivieren, dann Bus/Geraet/Funktion/Offset.
        // Die untersten zwei Bits des Offsets sind immer 0 (DWORD-Zugriff).
        let target: u32 = 0x8000_0000
            | ((address.bus as u32) << 16)
            | ((address.device as u32) << 11)
            | ((address.function as u32) << 8)
            | ((offset as u32) & 0xFC);
        let mut addr_port: PortWriteOnly<u32> = PortWriteOnly::new(0xCF8);
        let mut data_port: Port<u32> = Port::new(0xCFC);
        // Safety: 0xCF8/0xCFC sind die architektonisch festgelegten
        // Konfigurationsports; der Zugriff ist seitenwirkungsfrei.
        unsafe {
            addr_port.write(target);
            data_port.read()
        }
    }
}

/// Ein Gerat lesen. `None`, wenn der Slot leer ist (Herstellerkennung 0xFFFF).
fn probe(cfg: &mut impl ConfigSpace, address: PciAddress) -> Option<PciDevice> {
    let id = cfg.read_u32(address, 0x00);
    let vendor = (id & 0xFFFF) as u16;
    if vendor == 0xFFFF || vendor == 0x0000 {
        return None;
    }
    let class_word = cfg.read_u32(address, 0x08);
    let header = cfg.read_u32(address, 0x0C);
    Some(PciDevice {
        address,
        vendor,
        device: (id >> 16) as u16,
        // Register 0x08: Revision (7:0), Prog-IF (15:8), Subclass (23:16), Class (31:24)
        class: (class_word >> 24) as u8,
        subclass: ((class_word >> 16) & 0xFF) as u8,
        header_type: ((header >> 16) & 0xFF) as u8,
    })
}

/// Alle Geraete auf einem Bus. Mehrfunktionsgeraete werden vollstaendig
/// abgefragt, Einzelfunktionsgeraete nur in Funktion 0.
pub fn scan_bus(cfg: &mut impl ConfigSpace, bus: u8) -> Vec<PciDevice> {
    let mut found = Vec::new();
    for device in 0..32u8 {
        let address = PciAddress::new(bus, device, 0);
        let Some(first) = probe(cfg, address) else {
            continue;
        };
        let multifunction = first.is_multifunction();
        found.push(first);
        if multifunction {
            for function in 1..8u8 {
                if let Some(extra) = probe(cfg, PciAddress::new(bus, device, function)) {
                    found.push(extra);
                }
            }
        }
    }
    found
}

/// Den sekundaeren Bus einer Bruecke lesen.
///
/// Register 0x18 traegt drei Bussnummern: primaer (7:0), sekundaer (15:8),
/// untergeordnet (23:16). Nur der sekundaere Bus ist der Einstieg fuer den
/// naechsten Schritt; die untergeordnete Nummer sagt nur, wie weit der Baum
/// reicht.
pub fn secondary_bus(cfg: &mut impl ConfigSpace, address: PciAddress) -> Option<u8> {
    let word = cfg.read_u32(address, 0x18);
    let secondary = ((word >> 8) & 0xFF) as u8;
    // Bus 0 als Sekundaerbus waere ein Zyklus zur Wurzel; manche Firmware
    // traegt so etwas ein, wenn die Bruecke nicht konfiguriert ist.
    if secondary == 0 {
        None
    } else {
        Some(secondary)
    }
}

/// Wie tief der Scan Bruecken folgt. Acht Etagen sind fuer echte Maschinen
/// reichlich; die Grenze ist da, damit eine kaputte Firmware-Tabelle den Boot
/// nicht in eine Endlosschleife zieht.
pub const MAX_BRIDGE_DEPTH: u8 = 8;

/// Alle Geraete, auch hinter Bruecken.
///
/// Der flache `scan` ueber Bus 0..n findet nur, was direkt am Root-Bus haengt.
/// Auf jeder Maschine mit Steckplaetzen oder Root-Ports liegt der Rest dahinter.
/// Dieser Scan folgt jeder Bruecke einmal: `visited` haelt fest, welche Busse
/// schon dran waren, damit zwei Bruecken auf denselben Bus keine Doppelzaehlung
/// ergeben und eine Ringverbindung nicht ewig laeuft.
pub fn scan_tree(cfg: &mut impl ConfigSpace, root_bus: u8) -> Vec<PciDevice> {
    let mut found = Vec::new();
    let mut visited = Vec::new();
    let mut queue = alloc::vec![(root_bus, 0u8)];
    while let Some((bus, depth)) = queue.pop() {
        if visited.contains(&bus) || depth > MAX_BRIDGE_DEPTH {
            continue;
        }
        visited.push(bus);
        for device in scan_bus(cfg, bus) {
            if device.is_pci_bridge() && depth < MAX_BRIDGE_DEPTH {
                if let Some(secondary) = secondary_bus(cfg, device.address) {
                    if !visited.contains(&secondary) {
                        queue.push((secondary, depth + 1));
                    }
                }
            }
            found.push(device);
        }
    }
    // Reihenfolge stabil machen: Bus, Geraet, Funktion.
    found.sort_by_key(|d| d.address);
    found
}

/// Busse 0..`max_bus` absuchen. `max_bus` ist exklusiv.
pub fn scan(cfg: &mut impl ConfigSpace, max_bus: u8) -> Vec<PciDevice> {
    let mut all = Vec::new();
    for bus in 0..max_bus {
        all.extend(scan_bus(cfg, bus));
    }
    all
}

/// Kurzfassung fuer das Boot-Protokoll: "00:02.0 1234:1111 display".
pub fn describe(device: &PciDevice) -> (u8, u8, u8, u16, u16, &'static str) {
    (
        device.address.bus,
        device.address.device,
        device.address.function,
        device.vendor,
        device.device,
        device.kind().as_str(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ein Konfigurationsraum aus einer Tabelle: (bus,dev,func) -> Registerwerte.
    struct FakeConfig {
        entries: Vec<(PciAddress, u32, u32, u32)>,
        reads: usize,
        /// Register 0x18 je Bruecke: (Adresse, sekundaerer Bus).
        secondary: Vec<(PciAddress, u8)>,
    }

    impl FakeConfig {
        fn new() -> Self {
            Self {
                entries: Vec::new(),
                reads: 0,
                secondary: Vec::new(),
            }
        }

        /// Ein Slot: ((bus, geraet, funktion), (hersteller, geraet),
        /// (klasse, subklasse), header-typ).
        fn with(
            mut self,
            address: (u8, u8, u8),
            id_pair: (u16, u16),
            class_pair: (u8, u8),
            header: u8,
        ) -> Self {
            let (bus, device, function) = address;
            let (vendor, dev) = id_pair;
            let (class, subclass) = class_pair;
            let id = ((dev as u32) << 16) | vendor as u32;
            // Register 0x18 traegt die Busnummern; ohne Eintrag bleibt es 0.
            let class_word = ((class as u32) << 24) | ((subclass as u32) << 16);
            let header_word = (header as u32) << 16;
            self.entries.push((
                PciAddress::new(bus, device, function),
                id,
                class_word,
                header_word,
            ));
            self
        }
    }

    impl ConfigSpace for FakeConfig {
        fn read_u32(&mut self, address: PciAddress, offset: u8) -> u32 {
            self.reads += 1;
            let Some((_, id, class_word, header)) =
                self.entries.iter().find(|(a, ..)| *a == address)
            else {
                return 0xFFFF_FFFF; // leerer Slot
            };
            match offset & 0xFC {
                0x00 => *id,
                0x08 => *class_word,
                0x0C => *header,
                0x18 => {
                    let secondary = self
                        .secondary
                        .iter()
                        .find(|(a, _)| *a == address)
                        .map(|(_, bus)| *bus)
                        .unwrap_or(0);
                    (secondary as u32) << 8
                }
                _ => 0,
            }
        }
    }

    #[test]
    fn an_empty_bus_yields_nothing() {
        let mut cfg = FakeConfig::new();
        assert!(scan_bus(&mut cfg, 0).is_empty());
    }

    #[test]
    fn a_display_adapter_is_found_and_classified() {
        let mut cfg = FakeConfig::new().with((0, 2, 0), (0x1234, 0x1111), (0x03, 0x00), 0x00);
        let devices = scan_bus(&mut cfg, 0);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].vendor, 0x1234);
        assert_eq!(devices[0].device, 0x1111);
        assert_eq!(devices[0].kind(), DeviceKind::Display);
    }

    #[test]
    fn only_real_devices_are_reported() {
        // 0xFFFF ist der leere Slot, 0x0000 kommt bei defekten Bruecken vor.
        let mut cfg = FakeConfig::new()
            .with((0, 3, 0), (0xFFFF, 0x1234), (0x03, 0x00), 0x00)
            .with((0, 4, 0), (0x0000, 0x1234), (0x03, 0x00), 0x00);
        assert!(scan_bus(&mut cfg, 0).is_empty());
    }

    #[test]
    fn functions_of_a_single_function_device_are_not_probed() {
        let mut cfg = FakeConfig::new()
            .with((0, 1, 0), (0x8086, 0x7000), (0x06, 0x01), 0x00)
            // Funktion 1 existiert hier nur, um zu zeigen: sie wird nicht gelesen.
            .with((0, 1, 1), (0x8086, 0x7001), (0x01, 0x01), 0x00);
        let devices = scan_bus(&mut cfg, 0);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].address.function, 0);
    }

    #[test]
    fn a_multifunction_device_reports_all_functions() {
        let mut cfg = FakeConfig::new()
            .with((0, 1, 0), (0x8086, 0x7000), (0x06, 0x01), 0x80)
            .with((0, 1, 1), (0x8086, 0x7001), (0x01, 0x01), 0x80)
            .with((0, 1, 3), (0x8086, 0x7003), (0x0C, 0x03), 0x80);
        let devices = scan_bus(&mut cfg, 0);
        assert_eq!(devices.len(), 3);
        assert_eq!(devices[1].kind(), DeviceKind::Storage);
        assert_eq!(devices[2].kind(), DeviceKind::Usb);
    }

    #[test]
    fn the_scan_is_deterministic() {
        let build = || {
            FakeConfig::new()
                .with((0, 5, 0), (0x8086, 0x100E), (0x02, 0x00), 0x00)
                .with((0, 2, 0), (0x1234, 0x1111), (0x03, 0x00), 0x00)
                .with((0, 1, 0), (0x8086, 0x7000), (0x06, 0x01), 0x00)
        };
        let first: Vec<u8> = scan_bus(&mut build(), 0)
            .iter()
            .map(|d| d.address.device)
            .collect();
        let second: Vec<u8> = scan_bus(&mut build(), 0)
            .iter()
            .map(|d| d.address.device)
            .collect();
        assert_eq!(first, vec![1, 2, 5]);
        assert_eq!(first, second);
    }

    #[test]
    fn audio_is_audio_but_video_capture_is_not() {
        assert_eq!(DeviceKind::from_class(0x04, 0x01), DeviceKind::Audio);
        assert_eq!(DeviceKind::from_class(0x04, 0x00), DeviceKind::Other);
    }

    #[test]
    fn the_class_table_covers_what_a_machine_needs() {
        assert_eq!(DeviceKind::from_class(0x01, 0x08), DeviceKind::Storage); // NVMe
        assert_eq!(DeviceKind::from_class(0x02, 0x00), DeviceKind::Network);
        assert_eq!(DeviceKind::from_class(0x03, 0x00), DeviceKind::Display);
        assert_eq!(DeviceKind::from_class(0x06, 0x00), DeviceKind::Bridge);
        assert_eq!(DeviceKind::from_class(0x09, 0x00), DeviceKind::Input);
        assert_eq!(DeviceKind::from_class(0x0C, 0x03), DeviceKind::Usb);
        assert_eq!(DeviceKind::from_class(0xFF, 0xFF), DeviceKind::Other);
    }

    #[test]
    fn several_buses_are_concatenated_in_order() {
        let mut cfg = FakeConfig::new()
            .with((0, 2, 0), (0x1234, 0x1111), (0x03, 0x00), 0x00)
            .with((1, 0, 0), (0x8086, 0x100E), (0x02, 0x00), 0x00);
        let all = scan(&mut cfg, 2);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].address.bus, 0);
        assert_eq!(all[1].address.bus, 1);
    }

    #[test]
    fn a_description_carries_what_the_log_needs() {
        let device = PciDevice {
            address: PciAddress::new(0, 2, 0),
            vendor: 0x1234,
            device: 0x1111,
            class: 0x03,
            subclass: 0x00,
            header_type: 0x00,
        };
        assert_eq!(describe(&device), (0, 2, 0, 0x1234, 0x1111, "display"));
    }

    /// Eine Bruecke mit sekundaerem Bus.
    fn bridge(bus: u8, device: u8) -> PciDevice {
        PciDevice {
            address: PciAddress::new(bus, device, 0),
            vendor: 0x8086,
            device: 0x244E,
            class: 0x06,
            subclass: 0x04,
            header_type: 0x01,
        }
    }

    #[test]
    fn a_plain_isa_bridge_is_not_a_pci_bridge() {
        // 0x06/0x01 ist die ISA-Bruecke — die fuehrt zu keinem PCI-Bus.
        let isa = PciDevice {
            address: PciAddress::new(0, 1, 0),
            vendor: 0x8086,
            device: 0x7000,
            class: 0x06,
            subclass: 0x01,
            header_type: 0x00,
        };
        assert!(!isa.is_pci_bridge());
        assert!(bridge(0, 4).is_pci_bridge());
    }

    #[test]
    fn the_scan_follows_a_bridge_to_the_bus_behind_it() {
        // Bus 0: Bruecke auf 0x04, sekundaerer Bus 1. Hinter der Bruecke eine
        // Netzwerkkarte, die ein flacher Scan ueber Bus 0 nie sieht.
        let mut cfg = FakeConfig::new()
            .with((0, 4, 0), (0x8086, 0x244E), (0x06, 0x04), 0x01)
            .with((1, 0, 0), (0x8086, 0x100E), (0x02, 0x00), 0x00);
        cfg.secondary = alloc::vec![(PciAddress::new(0, 4, 0), 1)];

        // Der flache Scan findet nur die Bruecke ...
        let flat = scan_bus(&mut cfg, 0);
        assert_eq!(flat.len(), 1);

        // ... der Baum-Scan findet beide, nach Adresse sortiert.
        let tree = scan_tree(&mut cfg, 0);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].address.bus, 0);
        assert_eq!(tree[1].address.bus, 1);
        assert_eq!(tree[1].kind(), DeviceKind::Network);
    }

    #[test]
    fn a_bridge_pointing_at_the_root_bus_is_ignored() {
        let mut cfg = FakeConfig::new().with((0, 4, 0), (0x8086, 0x244E), (0x06, 0x04), 0x01);
        cfg.secondary = alloc::vec![(PciAddress::new(0, 4, 0), 0)];
        // Kein Zyklus, kein zweiter Durchlauf: nur die Bruecke selbst.
        assert_eq!(scan_tree(&mut cfg, 0).len(), 1);
    }

    #[test]
    fn two_bridges_on_the_same_bus_do_not_double_count() {
        let mut cfg = FakeConfig::new()
            .with((0, 4, 0), (0x8086, 0x244E), (0x06, 0x04), 0x01)
            .with((0, 5, 0), (0x8086, 0x244E), (0x06, 0x04), 0x01)
            .with((1, 0, 0), (0x8086, 0x100E), (0x02, 0x00), 0x00);
        cfg.secondary = alloc::vec![(PciAddress::new(0, 4, 0), 1), (PciAddress::new(0, 5, 0), 1),];
        let tree = scan_tree(&mut cfg, 0);
        assert_eq!(tree.len(), 3, "zwei Bruecken plus ein Geraet");
        assert_eq!(tree.iter().filter(|d| d.address.bus == 1).count(), 1);
    }

    #[test]
    fn a_bridge_ring_terminates() {
        // Bus 1 zeigt zurueck auf Bus 2, Bus 2 wieder auf Bus 1.
        let mut cfg = FakeConfig::new()
            .with((0, 4, 0), (0x8086, 0x244E), (0x06, 0x04), 0x01)
            .with((1, 2, 0), (0x8086, 0x244E), (0x06, 0x04), 0x01)
            .with((2, 3, 0), (0x8086, 0x244E), (0x06, 0x04), 0x01);
        cfg.secondary = alloc::vec![
            (PciAddress::new(0, 4, 0), 1),
            (PciAddress::new(1, 2, 0), 2),
            (PciAddress::new(2, 3, 0), 1),
        ];
        let tree = scan_tree(&mut cfg, 0);
        assert_eq!(tree.len(), 3, "jeder Bus genau einmal");
    }

    #[test]
    fn a_deep_chain_stops_at_the_depth_limit() {
        let mut cfg = FakeConfig::new();
        // Bus n hat je eine Bruecke auf Bus n+1.
        for bus in 0..12u8 {
            cfg = cfg.with((bus, 1, 0), (0x8086, 0x244E), (0x06, 0x04), 0x01);
            cfg.secondary.push((PciAddress::new(bus, 1, 0), bus + 1));
        }
        let tree = scan_tree(&mut cfg, 0);
        // Wurzel plus hoechstens MAX_BRIDGE_DEPTH weitere Busse.
        assert!(tree.len() <= MAX_BRIDGE_DEPTH as usize + 1);
        assert!(tree.len() >= 2);
    }
}
