// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// ACPI MADT (Multiple APIC Description Table, Signatur "APIC") — Auswertung
// Kernel Layer | Chain-ID 658467
//
// Die Bootpfad-Pruefung validiert bisher nur Checksummen. Diese Auswertung
// liefert die Ressourcen, die der APIC-Aufbau wirklich braucht: die gemeldete
// LAPIC-Basis, den IOAPIC samt GSI-Basis und die ISA-Overrides, die IRQs auf
// andere GSIs verschieben.
// ─────────────────────────────────────────────────────────────────────────

/// Signatur der MADT im ACPI-Baum.
pub const MADT_SIGNATURE: [u8; 4] = *b"APIC";

/// Kopf des MADT vor den Eintraegen: 36 Byte SDT-Kopf + LAPIC-Adresse + Flags.
pub const MADT_HEADER_LEN: usize = 44;

const ENTRY_PROCESSOR_LOCAL_APIC: u8 = 0;
const ENTRY_IO_APIC: u8 = 1;
const ENTRY_INTERRUPT_SOURCE_OVERRIDE: u8 = 2;
const ENTRY_LOCAL_APIC_OVERRIDE: u8 = 5;
const ENTRY_LOCAL_X2APIC: u8 = 9;

/// Fehler beim Lesen der MADT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MadtError {
    /// Tabelle ist kuerzer als der MADT-Kopf.
    Truncated,
    /// Ein Eintrag hat eine Laenge unter 2 Byte.
    BadEntryLength,
    /// Ein Eintrag ragt ueber das Tabellenende hinaus.
    EntryOverrun,
}

/// IOAPIC-Eintrag aus der MADT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoApic {
    pub id: u8,
    pub address: u32,
    pub gsi_base: u32,
}

/// Interrupt-Source-Override: verschiebt einen ISA-IRQ auf eine andere GSI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqOverride {
    pub source_irq: u8,
    pub gsi: u32,
}

/// Ergebnis der Auswertung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MadtSummary {
    /// Vom BIOS gemeldete LAPIC-Basis (kann 0 sein, dann gilt der Standard).
    pub local_apic_address: u64,
    /// Erster IOAPIC, falls vorhanden.
    pub ioapic: Option<IoApic>,
    /// Anzahl Local-APIC-Eintraege (Prozessoren).
    pub processors: u32,
    /// Davon als nutzbar markiert (Flag-Bit 0).
    pub processors_enabled: u32,
    /// Anzahl x2APIC-Eintraege (nur gezaehlt, nicht ausgewertet).
    pub x2apic_processors: u32,
    /// Anzahl ISA-Overrides.
    pub overrides: u32,
    /// Erster Override, sofern vorhanden.
    pub first_override: Option<IrqOverride>,
    /// Alles, was dieser Parser nicht kennt.
    pub unknown_entries: u32,
}

/// Wertet die MADT aus.
pub fn summarize(table: &[u8]) -> Result<MadtSummary, MadtError> {
    if table.len() < MADT_HEADER_LEN {
        return Err(MadtError::Truncated);
    }
    if table[0..4] != MADT_SIGNATURE {
        return Err(MadtError::Truncated);
    }

    let local_apic_address =
        u32::from_le_bytes([table[36], table[37], table[38], table[39]]) as u64;

    let mut summary = MadtSummary {
        local_apic_address,
        ioapic: None,
        processors: 0,
        processors_enabled: 0,
        x2apic_processors: 0,
        overrides: 0,
        first_override: None,
        unknown_entries: 0,
    };

    let mut offset = MADT_HEADER_LEN;
    while offset < table.len() {
        let kind = table[offset];
        // Ein 1-Byte-Rest ist kein gueltiger Eintrag mehr.
        let length = *table.get(offset + 1).ok_or(MadtError::BadEntryLength)? as usize;
        if length < 2 {
            return Err(MadtError::BadEntryLength);
        }
        let end = offset.checked_add(length).ok_or(MadtError::EntryOverrun)?;
        if end > table.len() {
            return Err(MadtError::EntryOverrun);
        }
        let entry = &table[offset..end];

        match kind {
            ENTRY_PROCESSOR_LOCAL_APIC => {
                summary.processors += 1;
                if entry.len() >= 5 && entry[4] & 1 != 0 {
                    summary.processors_enabled += 1;
                }
            }
            ENTRY_IO_APIC => {
                if entry.len() >= 12 && summary.ioapic.is_none() {
                    summary.ioapic = Some(IoApic {
                        id: entry[2],
                        address: u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]]),
                        gsi_base: u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]),
                    });
                }
            }
            ENTRY_INTERRUPT_SOURCE_OVERRIDE => {
                if entry.len() >= 8 {
                    summary.overrides += 1;
                    if summary.first_override.is_none() {
                        summary.first_override = Some(IrqOverride {
                            source_irq: entry[3],
                            gsi: u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]]),
                        });
                    }
                }
            }
            ENTRY_LOCAL_APIC_OVERRIDE => {
                if entry.len() >= 12 {
                    let address = u64::from_le_bytes([
                        entry[4], entry[5], entry[6], entry[7], entry[8], entry[9], entry[10],
                        entry[11],
                    ]);
                    if address != 0 {
                        summary.local_apic_address = address;
                    }
                }
            }
            ENTRY_LOCAL_X2APIC => summary.x2apic_processors += 1,
            _ => summary.unknown_entries += 1,
        }

        offset = end;
    }

    Ok(summary)
}

/// Bildet einen ISA-IRQ auf die GSI ab, die die MADT vorschreibt.
/// Ohne Override gilt GSI = IRQ.
pub fn gsi_for_irq(summary: &MadtSummary, irq: u8) -> u32 {
    match summary.first_override {
        Some(override_entry) if override_entry.source_irq == irq => override_entry.gsi,
        _ => u32::from(irq),
    }
}

#[cfg(test)]
mod madt_tests {
    use super::*;

    /// Baut eine MADT mit Kopf und beliebigen Eintraegen.
    fn table(entries: &[&[u8]]) -> alloc::vec::Vec<u8> {
        let mut bytes = alloc::vec![0u8; MADT_HEADER_LEN];
        bytes[0..4].copy_from_slice(&MADT_SIGNATURE);
        for entry in entries {
            bytes.extend_from_slice(entry);
        }
        bytes
    }

    fn processor(apic_id: u8, enabled: bool) -> [u8; 8] {
        [0, 8, 0, apic_id, u8::from(enabled), 0, 0, 0]
    }

    fn ioapic(address: u32, gsi_base: u32) -> [u8; 12] {
        let mut entry = [0u8; 12];
        entry[0] = 1;
        entry[1] = 12;
        entry[2] = 2;
        entry[4..8].copy_from_slice(&address.to_le_bytes());
        entry[8..12].copy_from_slice(&gsi_base.to_le_bytes());
        entry
    }

    fn override_entry(irq: u8, gsi: u32) -> [u8; 10] {
        let mut entry = [0u8; 10];
        entry[0] = 2;
        entry[1] = 10;
        entry[3] = irq;
        entry[4..8].copy_from_slice(&gsi.to_le_bytes());
        entry
    }

    #[test]
    fn reads_lapic_address_ioapic_and_processors() {
        let mut bytes = table(&[&processor(0, true), &ioapic(0xFEC0_0000, 0)]);
        bytes[36..40].copy_from_slice(&0xFEE0_0000u32.to_le_bytes());
        let summary = summarize(&bytes).unwrap();
        assert_eq!(summary.local_apic_address, 0xFEE0_0000);
        assert_eq!(summary.processors, 1);
        assert_eq!(summary.processors_enabled, 1);
        assert_eq!(
            summary.ioapic,
            Some(IoApic {
                id: 2,
                address: 0xFEC0_0000,
                gsi_base: 0
            })
        );
    }

    #[test]
    fn counts_only_enabled_processors() {
        let bytes = table(&[&processor(0, true), &processor(1, false)]);
        let summary = summarize(&bytes).unwrap();
        assert_eq!(summary.processors, 2);
        assert_eq!(summary.processors_enabled, 1);
    }

    #[test]
    fn interrupt_override_moves_the_irq() {
        let bytes = table(&[&override_entry(0, 2)]);
        let summary = summarize(&bytes).unwrap();
        assert_eq!(summary.overrides, 1);
        assert_eq!(gsi_for_irq(&summary, 0), 2, "IRQ 0 wird auf GSI 2 umgelegt");
        assert_eq!(gsi_for_irq(&summary, 1), 1, "unbeteiligte IRQs bleiben");
    }

    #[test]
    fn local_apic_override_wins_over_the_header_address() {
        let mut entry = [0u8; 12];
        entry[0] = 5;
        entry[1] = 12;
        entry[4..12].copy_from_slice(&0xFEE0_1000u64.to_le_bytes());
        let bytes = table(&[&entry]);
        let summary = summarize(&bytes).unwrap();
        assert_eq!(summary.local_apic_address, 0xFEE0_1000);
    }

    #[test]
    fn zero_lapic_override_is_ignored() {
        let mut entry = [0u8; 12];
        entry[0] = 5;
        entry[1] = 12;
        let bytes = table(&[&entry]);
        assert_eq!(summarize(&bytes).unwrap().local_apic_address, 0);
    }

    #[test]
    fn unknown_entries_are_counted_not_rejected() {
        let bytes = table(&[&[0x7F, 4, 0, 0]]);
        let summary = summarize(&bytes).unwrap();
        assert_eq!(summary.unknown_entries, 1);
    }

    #[test]
    fn truncated_header_is_rejected() {
        assert_eq!(summarize(&[0u8; 40]), Err(MadtError::Truncated));
    }

    #[test]
    fn wrong_signature_is_rejected() {
        let mut bytes = table(&[]);
        bytes[0..4].copy_from_slice(b"FACP");
        assert_eq!(summarize(&bytes), Err(MadtError::Truncated));
    }

    #[test]
    fn zero_length_entry_is_rejected() {
        let bytes = table(&[&[0, 0]]);
        assert_eq!(summarize(&bytes), Err(MadtError::BadEntryLength));
    }

    #[test]
    fn entry_beyond_the_end_is_rejected() {
        let bytes = table(&[&[0, 40, 0, 0]]);
        assert_eq!(summarize(&bytes), Err(MadtError::EntryOverrun));
    }
}
