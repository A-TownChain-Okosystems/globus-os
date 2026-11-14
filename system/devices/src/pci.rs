//! PCI/PCIe discovery and MMIO contracts.
//!
//! Raw config-space access and BAR mapping remain privileged HAL operations.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Adresse eines PCI-Geraets.
pub struct PciAddress {
    /// PCI-Segment.
    pub segment: u16,
    /// Busnummer.
    pub bus: u8,
    /// Geraetenummer.
    pub device: u8,
    /// Funktionsnummer.
    pub function: u8,
}
impl PciAddress {
    /// Baut eine Geraeteadresse.
    pub const fn new(segment: u16, bus: u8, device: u8, function: u8) -> Self {
        Self {
            segment,
            bus,
            device,
            function,
        }
    }
    /// True, wenn Bus, Geraet und Funktion im gueltigen Bereich liegen.
    pub const fn valid(&self) -> bool {
        self.device < 32 && self.function < 8
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Kennung eines PCI-Geraets.
pub struct PciId {
    /// Herstellerkennung.
    pub vendor: u16,
    /// Geraetekennung.
    pub device: u16,
    /// Klassenbyte.
    pub class: u8,
    /// Unterklasse.
    pub subclass: u8,
    /// Programmierschnittstelle der Klasse.
    pub prog_if: u8,
}
impl PciId {
    /// True, wenn das Geraet ein NVMe-Controller ist.
    pub const fn is_nvme(&self) -> bool {
        self.class == 0x01 && self.subclass == 0x08
    }
    /// True, wenn das Geraet ein Netzwerkgeraet ist.
    pub const fn is_network(&self) -> bool {
        self.class == 0x02
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Art einer Base Address Region.
pub enum BarKind {
    /// 32-Bit-Speicher.
    Memory32,
    /// 64-Bit-Speicher.
    Memory64,
    /// Port-Ein- und -Ausgabe.
    Io,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Eine Base Address Region.
pub struct PciBar {
    /// Index der BAR im Konfigurationsraum.
    pub index: u8,
    /// Art der Region.
    pub kind: BarKind,
    /// Basisadresse.
    pub base: u64,
    /// Groesse in Byte.
    pub size: u64,
    /// True, wenn die Region vorauslesbar ist.
    pub prefetchable: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler beim PCI-Zugriff.
pub enum PciError {
    /// Die Adresse ist ungueltig.
    InvalidAddress,
    /// Der Konfigurationszugriff ist fehlgeschlagen.
    ConfigAccessFailed,
    /// Die Region ist ungueltig.
    InvalidBar,
    /// Die Abbildung wurde abgelehnt.
    MappingDenied,
}
/// Zugriff auf den PCI-Konfigurationsraum.
pub trait PciConfigAccess {
    /// Liest ein 32-Bit-Register.
    fn read_u32(&self, address: PciAddress, offset: u16) -> Result<u32, PciError>;
    /// Schreibt ein 32-Bit-Register.
    fn write_u32(&self, address: PciAddress, offset: u16, value: u32) -> Result<(), PciError>;
}
/// Bildet Geraetespeicher in den Kerneladressraum ab.
pub trait MmioMapper {
    /// Ergebnis der Abbildung, etwa ein virtueller Zeiger oder eine Kennung.
    type Mapping;
    /// Bildet eine BAR ab und liefert das Ergebnis der Abbildung.
    fn map_device_bar(&self, bar: PciBar) -> Result<Self::Mapping, PciError>;
}

/// Baut eine Kennung aus Klassen- und Herstellerangaben.
pub fn decode_id(class: u8, subclass: u8, prog_if: u8, vendor: u16, device: u16) -> PciId {
    PciId {
        vendor,
        device,
        class,
        subclass,
        prog_if,
    }
}
/// Prueft Groesse und Ausrichtung einer BAR.
pub fn validate_bar(bar: PciBar) -> Result<(), PciError> {
    if bar.index >= 6
        || bar.size == 0
        || !bar.size.is_power_of_two()
        || bar.base.checked_add(bar.size).is_none()
    {
        Err(PciError::InvalidBar)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supports_64bit_bar_addresses() {
        let bar = PciBar {
            index: 0,
            kind: BarKind::Memory64,
            base: 0x1_0000_0000,
            size: 0x1000,
            prefetchable: false,
        };
        assert!(bar.base > u32::MAX as u64);
        assert!(validate_bar(bar).is_ok());
    }
    #[test]
    fn identifies_nvme() {
        assert!(decode_id(1, 8, 2, 0, 0).is_nvme());
    }
}
