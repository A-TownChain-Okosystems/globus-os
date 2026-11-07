// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// SC-006 REQ-SC006-11 — ACPI als zweite Geraetequelle
//
// Der PCI-Scan sieht nur, was hinter einem Konfigurationsraum liegt. Drei
// Geraete, die der Kernel wirklich benutzt, sieht er nie: Local APIC, IOAPIC
// und den High Precision Event Timer. Sie kommen aus den ACPI-Tabellen.
//
// Diese Datei macht aus den bereits geprueften Tabellen eine Geraeteliste in
// derselben Form wie der PCI-Scan sie liefert, damit beide Quellen in einen
// Vorrat passen.
//
// Drei Regeln, gleiche Haltung wie bei `pci_scan`:
//
//   * **Finden vergibt keine Rechte.** Kein Eintrag hier oeffnet irgendetwas.
//   * **Keine Adresse ohne Pruefung.** Der HPET-Adressraum muss Speicher sein,
//     die Adresse darf nicht null sein.
//   * **Was nicht da ist, wird nicht erfunden.** Fehlt die HPET-Tabelle oder
//     meldet die MADT keinen IOAPIC, fehlt der Eintrag.
// ─────────────────────────────────────────────────────────────────────────

use alloc::vec::Vec;

use crate::madt::MadtSummary;

/// Signatur der HPET-Tabelle.
pub const HPET_SIGNATURE: [u8; 4] = *b"HPET";

/// Kleinste sinnvolle HPET-Tabelle. Aufbau ab dem Kopf (36 Byte):
/// 36 Hardware-Revision, 37 Comparator-Zaehler, 38 Zaehlergroesse, 39 reserviert,
/// **40 Adressstruktur (12 Byte)**, 52 HPET-Nummer, 53 Minimal-Tick (2 Byte),
/// 55 Seitenattribute. Die Adresse selbst liegt damit bei 44.
///
/// Diese Offsets sind nicht geraten: ein QEMU-Lauf mit 56-Byte-Tabelle hat
/// `00 00 00 00 | 00 00 d0 fe 00 00 00 00` ab Offset 40 geliefert, also
/// Speicher-Adressraum, Breite 0, Adresse 0xFED00000. Eine Fassung dieses
/// Parsers mit der Adresse bei 48 las dort Nullen und lehnte die Tabelle ab.
pub const HPET_MIN_LEN: usize = 56;

/// Offset der Adressstruktur in der HPET-Tabelle.
const HPET_ADDRESS_STRUCTURE: usize = 40;

/// Offset der Adresse innerhalb der Adressstruktur.
const HPET_ADDRESS_OFFSET: usize = HPET_ADDRESS_STRUCTURE + 4;

/// Adressraum-Kennung "Systemspeicher" in einer ACPI-Adressstruktur.
const ADDRESS_SPACE_MEMORY: u8 = 0;

/// Standardbasis des Local APIC, wenn die MADT keine nennt.
pub const DEFAULT_LOCAL_APIC_ADDRESS: u64 = 0xFEE0_0000;

/// Fehler beim Auswerten einer ACPI-Tabelle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpiTableError {
    /// Tabelle ist kuerzer als der erforderliche Kopf.
    Truncated,
    /// Die Signatur passt nicht zur erwarteten Tabelle.
    BadSignature,
    /// Die Adressstruktur zeigt nicht auf Systemspeicher (z. B. E/A-Ports).
    UnsupportedAddressSpace(u8),
    /// Die Tabelle nennt die Nulladresse.
    NullAddress,
}

/// Art eines ACPI-Geraets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpiDeviceKind {
    /// Local APIC der laufenden CPU.
    LocalApic,
    /// IOAPIC, der die Geraete-IRQs auf Vektoren abbildet.
    IoApic,
    /// High Precision Event Timer.
    Hpet,
}

impl AcpiDeviceKind {
    /// Stabiler Name fuer Log und Registratur.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalApic => "LAPIC",
            Self::IoApic => "IOAPIC",
            Self::Hpet => "HPET",
        }
    }
}

/// Ein Geraet, das nicht ueber den PCI-Konfigurationsraum gefunden wurde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcpiDevice {
    /// Art des Geraets.
    pub kind: AcpiDeviceKind,
    /// Physische Basisadresse des Registersatzes.
    pub address: u64,
    /// Basis-GSI, nur beim IOAPIC belegt.
    pub gsi_base: Option<u32>,
}

impl AcpiDevice {
    /// Name fuer Log und Registratur, z. B. `acpi HPET`.
    pub fn name(self) -> &'static str {
        self.kind.as_str()
    }
}

/// Liest die Basisadresse des HPET aus einer HPET-Tabelle.
///
/// Die Pruefsumme wird hier nicht noch einmal gebildet: die Tabellen sind im
/// Bootpfad bereits geprueft, bevor sie hier ankommen. Was hier geprueft wird,
/// ist die Bedeutung der Bytes, also Signatur, Laenge und Adressraum.
pub fn hpet_address(table: &[u8]) -> Result<u64, AcpiTableError> {
    if table.len() < HPET_MIN_LEN {
        return Err(AcpiTableError::Truncated);
    }
    if table[0..4] != HPET_SIGNATURE {
        return Err(AcpiTableError::BadSignature);
    }
    let address_space = table[HPET_ADDRESS_STRUCTURE];
    if address_space != ADDRESS_SPACE_MEMORY {
        return Err(AcpiTableError::UnsupportedAddressSpace(address_space));
    }
    let base = HPET_ADDRESS_OFFSET;
    let address = u64::from_le_bytes([
        table[base],
        table[base + 1],
        table[base + 2],
        table[base + 3],
        table[base + 4],
        table[base + 5],
        table[base + 6],
        table[base + 7],
    ]);
    if address == 0 {
        return Err(AcpiTableError::NullAddress);
    }
    Ok(address)
}

/// Baut die ACPI-Geraeteliste aus der MADT und einer optionalen HPET-Adresse.
///
/// Reihenfolge ist fest: LAPIC, IOAPIC, HPET. Damit ist die Liste zwischen
/// Laeufen vergleichbar und nicht von der Tabellenreihenfolge abhaengig.
pub fn from_summary(madt: &MadtSummary, hpet_base: Option<u64>) -> Vec<AcpiDevice> {
    let mut devices = Vec::new();

    let lapic = if madt.local_apic_address == 0 {
        DEFAULT_LOCAL_APIC_ADDRESS
    } else {
        madt.local_apic_address
    };
    devices.push(AcpiDevice {
        kind: AcpiDeviceKind::LocalApic,
        address: lapic,
        gsi_base: None,
    });

    if let Some(ioapic) = madt.ioapic.filter(|ioapic| ioapic.address != 0) {
        devices.push(AcpiDevice {
            kind: AcpiDeviceKind::IoApic,
            address: u64::from(ioapic.address),
            gsi_base: Some(ioapic.gsi_base),
        });
    }

    if let Some(base) = hpet_base.filter(|base| *base != 0) {
        devices.push(AcpiDevice {
            kind: AcpiDeviceKind::Hpet,
            address: base,
            gsi_base: None,
        });
    }

    devices
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::madt::{IoApic, MadtSummary};

    fn summary(lapic: u64, ioapic: Option<IoApic>) -> MadtSummary {
        MadtSummary {
            local_apic_address: lapic,
            ioapic,
            processors: 1,
            processors_enabled: 1,
            x2apic_processors: 0,
            overrides: 0,
            first_override: None,
            unknown_entries: 0,
        }
    }

    fn hpet_table(address_space: u8, base: u64) -> Vec<u8> {
        let mut table = alloc::vec![0u8; HPET_MIN_LEN];
        table[0..4].copy_from_slice(&HPET_SIGNATURE);
        table[4..8].copy_from_slice(&(HPET_MIN_LEN as u32).to_le_bytes());
        table[HPET_ADDRESS_STRUCTURE] = address_space;
        table[HPET_ADDRESS_OFFSET..HPET_ADDRESS_OFFSET + 8].copy_from_slice(&base.to_le_bytes());
        table
    }

    #[test]
    fn hpet_address_reads_the_memory_base() {
        let table = hpet_table(0, 0xFED0_0000);
        assert_eq!(hpet_address(&table), Ok(0xFED0_0000));
    }

    #[test]
    fn hpet_address_rejects_io_ports() {
        // Adressraum 1 ist System-E/A, kein Speicher. Ein Geraet dort darf
        // nicht als MMIO-Fenster gemappt werden.
        let table = hpet_table(1, 0xFED0_0000);
        assert_eq!(
            hpet_address(&table),
            Err(AcpiTableError::UnsupportedAddressSpace(1))
        );
    }

    #[test]
    fn hpet_address_rejects_the_null_address() {
        let table = hpet_table(0, 0);
        assert_eq!(hpet_address(&table), Err(AcpiTableError::NullAddress));
    }

    #[test]
    fn hpet_address_rejects_a_foreign_signature() {
        let mut table = hpet_table(0, 0xFED0_0000);
        table[0..4].copy_from_slice(b"FACP");
        assert_eq!(hpet_address(&table), Err(AcpiTableError::BadSignature));
    }

    #[test]
    fn hpet_address_rejects_a_truncated_table() {
        let table = hpet_table(0, 0xFED0_0000);
        assert_eq!(
            hpet_address(&table[..HPET_MIN_LEN - 1]),
            Err(AcpiTableError::Truncated)
        );
    }

    #[test]
    fn summary_without_ioapic_and_hpet_yields_only_the_lapic() {
        let devices = from_summary(&summary(0xFEE0_0000, None), None);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].kind, AcpiDeviceKind::LocalApic);
        assert_eq!(devices[0].address, 0xFEE0_0000);
    }

    #[test]
    fn a_missing_lapic_address_falls_back_to_the_default() {
        // Das BIOS darf 0 melden, dann gilt die Architekturvorgabe.
        let devices = from_summary(&summary(0, None), None);
        assert_eq!(devices[0].address, DEFAULT_LOCAL_APIC_ADDRESS);
    }

    #[test]
    fn ioapic_carries_its_gsi_base() {
        let devices = from_summary(
            &summary(
                0xFEE0_0000,
                Some(IoApic {
                    id: 0,
                    address: 0xFEC0_0000,
                    gsi_base: 0,
                }),
            ),
            None,
        );
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[1].kind, AcpiDeviceKind::IoApic);
        assert_eq!(devices[1].gsi_base, Some(0));
    }

    #[test]
    fn the_full_set_keeps_a_fixed_order() {
        let devices = from_summary(
            &summary(
                0xFEE0_0000,
                Some(IoApic {
                    id: 0,
                    address: 0xFEC0_0000,
                    gsi_base: 0,
                }),
            ),
            Some(0xFED0_0000),
        );
        let kinds: Vec<&str> = devices.iter().map(|device| device.name()).collect();
        assert_eq!(kinds, alloc::vec!["LAPIC", "IOAPIC", "HPET"]);
    }

    #[test]
    fn a_zero_hpet_base_is_not_a_device() {
        let devices = from_summary(&summary(0xFEE0_0000, None), Some(0));
        assert_eq!(devices.len(), 1);
    }

    #[test]
    fn a_zero_ioapic_address_is_not_a_device() {
        let devices = from_summary(
            &summary(
                0xFEE0_0000,
                Some(IoApic {
                    id: 0,
                    address: 0,
                    gsi_base: 0,
                }),
            ),
            None,
        );
        assert_eq!(devices.len(), 1);
    }

    #[test]
    fn finding_a_device_grants_nothing() {
        // Der Typ traegt keine Rechte, keine Cap, keinen Besitzer. Wer ihn
        // benutzen will, braucht eine Device-Cap aus der Registratur.
        let device = from_summary(&summary(0xFEE0_0000, None), Some(0xFED0_0000))
            .pop()
            .unwrap();
        assert_eq!(device.name(), "HPET");
        assert_eq!(device.address, 0xFED0_0000);
    }
}
