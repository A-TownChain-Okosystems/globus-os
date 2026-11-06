// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// IOAPIC — Treiberschicht für die Redirection-Tabelle
// Kernel Layer | Chain-ID 658467
//
// Die MADT nennt Adresse und GSI-Basis. Diese Schicht macht daraus bedienbare
// Hardware: Einträge über IOREGSEL/IOWIN lesen, schreiben, maskieren. Jeder
// Eintrag ist 64 Bit breit und liegt in zwei aufeinanderfolgenden Registern.
// ─────────────────────────────────────────────────────────────────────────

/// Register zur Auswahl eines indirekten Registers.
pub const IOREGSEL: u32 = 0x00;
/// Fenster, über das der Inhalt des gewählten Registers gelesen/geschrieben wird.
pub const IOWIN: u32 = 0x10;
/// Erstes Register der Redirection-Tabelle (jeder Eintrag belegt zwei).
pub const IOREDTBL_BASE: u32 = 0x10;

const REG_ID: u32 = 0x00;
const REG_VERSION: u32 = 0x01;

/// Bit 16 der Low-Hälfte maskiert den Eintrag.
const MASK_BIT: u32 = 1 << 16;

/// Fehler beim Zugriff auf die Redirection-Tabelle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoApicError {
    /// Die GSI liegt unterhalb der GSI-Basis des IOAPIC.
    GsiBelowBase,
    /// Die GSI zeigt über den letzten Eintrag hinaus.
    GsiOutOfRange,
}

/// Ein Eintrag der Redirection-Tabelle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedirectionEntry {
    /// Zielvektor im Local APIC (0x20..=0xFE sinnvoll, 0x00..0x1F sind Exceptions).
    pub vector: u8,
    /// 0 = Fixed, 1 = Lowest Priority, 2 = SMI, 4 = NMI, 5 = INIT, 7 = ExtINT.
    pub delivery_mode: u8,
    /// true = physisches Ziel, false = logisches Ziel.
    pub physical_destination: bool,
    /// true = Low-Pegel auslösend, false = High-Pegel.
    pub active_low: bool,
    /// true = Level-getriggert, false = flankengetriggert.
    pub level_triggered: bool,
    /// true = Eintrag ist maskiert und liefert nichts.
    pub masked: bool,
    /// Ziel-APIC (bei physischem Ziel die APIC-ID).
    pub destination: u8,
}

impl RedirectionEntry {
    /// Standardeintrag: fester Vektor, physisches Ziel, flankengetriggert, maskiert.
    pub const fn fixed(vector: u8, destination: u8) -> Self {
        Self {
            vector,
            delivery_mode: 0,
            physical_destination: true,
            active_low: false,
            level_triggered: false,
            masked: true,
            destination,
        }
    }

    /// Kodiert den Eintrag in die beiden 32-Bit-Hälften (low, high).
    pub const fn encode(&self) -> (u32, u32) {
        let mut low = self.vector as u32;
        low |= (self.delivery_mode as u32 & 0x7) << 8;
        if !self.physical_destination {
            low |= 1 << 11;
        }
        if self.active_low {
            low |= 1 << 13;
        }
        if self.level_triggered {
            low |= 1 << 15;
        }
        if self.masked {
            low |= MASK_BIT;
        }
        let high = (self.destination as u32) << 24;
        (low, high)
    }

    /// Dekodiert die beiden Hälften zurück in einen Eintrag.
    pub const fn decode(low: u32, high: u32) -> Self {
        Self {
            vector: (low & 0xFF) as u8,
            delivery_mode: ((low >> 8) & 0x7) as u8,
            physical_destination: (low >> 11) & 1 == 0,
            active_low: (low >> 13) & 1 == 1,
            level_triggered: (low >> 15) & 1 == 1,
            masked: low & MASK_BIT != 0,
            destination: ((high >> 24) & 0xFF) as u8,
        }
    }
}

/// Registerzugriff auf ein gemapptes IOAPIC-MMIO-Fenster. Volatil, weil die
/// Hardware die Werte hinter dem Rücken des Compilers ändert.
pub struct MappedRegisters {
    base: *mut u32,
}

impl MappedRegisters {
    /// # Safety
    /// `base` muss auf ein gemapptes IOAPIC-Fenster zeigen (mindestens bis
    /// Byte-Offset 0x10, also Index 4).
    pub unsafe fn new(base: *mut u32) -> Self {
        Self { base }
    }
}

impl IoApicRegisters for MappedRegisters {
    fn read(&self, register: u32) -> u32 {
        unsafe { core::ptr::read_volatile(self.base.add((register / 4) as usize)) }
    }

    fn write(&mut self, register: u32, value: u32) {
        unsafe { core::ptr::write_volatile(self.base.add((register / 4) as usize), value) }
    }
}

/// Registerzugriff auf den IOAPIC. Über ein Trait, damit die Logik ohne
/// Hardware prüfbar bleibt.
pub trait IoApicRegisters {
    fn read(&self, register: u32) -> u32;
    fn write(&mut self, register: u32, value: u32);
}

/// IOAPIC mit Redirection-Tabelle.
pub struct IoApic<M: IoApicRegisters> {
    registers: M,
    gsi_base: u32,
    max_entries: u32,
}

impl<M: IoApicRegisters> IoApic<M> {
    /// Erzeugt die Schicht und liest die Eintragszahl aus dem Versionsregister.
    pub fn new(mut registers: M, gsi_base: u32) -> Self {
        // Auch das Versionsregister liegt hinter IOREGSEL/IOWIN.
        registers.write(IOREGSEL, REG_VERSION);
        let version = registers.read(IOWIN);
        // Bits 16..24 tragen die hoechste Eintragsnummer (0-basiert).
        let max_entries = ((version >> 16) & 0xFF) + 1;
        Self {
            registers,
            gsi_base,
            max_entries,
        }
    }

    /// Kennung dieses IOAPIC.
    pub fn id(&mut self) -> u8 {
        (self.read_register(REG_ID) >> 24) as u8
    }

    /// Versionsregister im Rohwert.
    pub fn version(&mut self) -> u32 {
        self.read_register(REG_VERSION)
    }

    /// Anzahl der Einträge in der Redirection-Tabelle.
    pub fn max_entries(&self) -> u32 {
        self.max_entries
    }

    /// Untere GSI, die dieser IOAPIC bedient.
    pub fn gsi_base(&self) -> u32 {
        self.gsi_base
    }

    fn entry_index(&self, gsi: u32) -> Result<u32, IoApicError> {
        if gsi < self.gsi_base {
            return Err(IoApicError::GsiBelowBase);
        }
        let index = gsi - self.gsi_base;
        if index >= self.max_entries {
            return Err(IoApicError::GsiOutOfRange);
        }
        Ok(index)
    }

    fn read_register(&mut self, register: u32) -> u32 {
        self.registers.write(IOREGSEL, register);
        self.registers.read(IOWIN)
    }

    fn write_register(&mut self, register: u32, value: u32) {
        self.registers.write(IOREGSEL, register);
        self.registers.write(IOWIN, value);
    }

    /// Liest den Eintrag für eine GSI.
    pub fn redirection_entry(&mut self, gsi: u32) -> Result<RedirectionEntry, IoApicError> {
        let index = self.entry_index(gsi)?;
        let low_register = IOREDTBL_BASE + index * 2;
        let low = self.read_register(low_register);
        let high = self.read_register(low_register + 1);
        Ok(RedirectionEntry::decode(low, high))
    }

    /// Schreibt den Eintrag für eine GSI und liest ihn zur Kontrolle zurück.
    pub fn set_redirection_entry(
        &mut self,
        gsi: u32,
        entry: RedirectionEntry,
    ) -> Result<RedirectionEntry, IoApicError> {
        let index = self.entry_index(gsi)?;
        let low_register = IOREDTBL_BASE + index * 2;
        let (low, high) = entry.encode();
        self.write_register(low_register, low);
        self.write_register(low_register + 1, high);
        self.redirection_entry(gsi)
    }

    /// Setzt das Maskenbit, ohne die übrigen Felder anzutasten.
    pub fn mask(&mut self, gsi: u32) -> Result<RedirectionEntry, IoApicError> {
        let mut entry = self.redirection_entry(gsi)?;
        entry.masked = true;
        self.set_redirection_entry(gsi, entry)
    }

    /// Löscht das Maskenbit, ohne die übrigen Felder anzutasten.
    pub fn unmask(&mut self, gsi: u32) -> Result<RedirectionEntry, IoApicError> {
        let mut entry = self.redirection_entry(gsi)?;
        entry.masked = false;
        self.set_redirection_entry(gsi, entry)
    }
}

#[cfg(test)]
mod ioapic_tests {
    use super::*;

    /// Zeichnet Registerzugriffe auf und verhält sich wie ein IOAPIC mit vier
    /// Einträgen (Version 0x00170003).
    struct FakeIoApic {
        selected: u32,
        regs: alloc::collections::BTreeMap<u32, u32>,
        writes: alloc::vec::Vec<(u32, u32)>,
    }

    impl FakeIoApic {
        fn new() -> Self {
            let mut regs = alloc::collections::BTreeMap::new();
            regs.insert(REG_ID, 0x0200_0000);
            regs.insert(REG_VERSION, 0x0003_0003);
            Self {
                selected: 0,
                regs,
                writes: alloc::vec::Vec::new(),
            }
        }
    }

    impl IoApicRegisters for FakeIoApic {
        fn read(&self, register: u32) -> u32 {
            if register == IOWIN {
                *self.regs.get(&self.selected).unwrap_or(&0)
            } else {
                *self.regs.get(&register).unwrap_or(&0)
            }
        }

        fn write(&mut self, register: u32, value: u32) {
            if register == IOREGSEL {
                self.selected = value;
            } else if register == IOWIN {
                self.writes.push((self.selected, value));
                self.regs.insert(self.selected, value);
            }
        }
    }

    #[test]
    fn reads_id_version_and_entry_count() {
        let mut apic = IoApic::new(FakeIoApic::new(), 0);
        assert_eq!(apic.id(), 2);
        assert_eq!(apic.max_entries(), 4, "Bit 16..24 = hoechster Index 3");
        assert_eq!(apic.gsi_base(), 0);
    }

    #[test]
    fn encoded_entry_keeps_the_defined_bit_positions() {
        let entry = RedirectionEntry {
            vector: 0x50,
            delivery_mode: 0,
            physical_destination: true,
            active_low: false,
            level_triggered: false,
            masked: false,
            destination: 0,
        };
        let (low, high) = entry.encode();
        assert_eq!(low & 0xFF, 0x50, "Vektor in Bit 0..8");
        assert_eq!(low & MASK_BIT, 0, "unmaskiert");
        assert_eq!(high, 0);

        let masked = RedirectionEntry::fixed(0x50, 0);
        assert_eq!(masked.encode().0 & MASK_BIT, MASK_BIT, "Bit 16 maskiert");
    }

    #[test]
    fn level_triggered_and_active_low_set_their_bits() {
        let mut entry = RedirectionEntry::fixed(0x21, 0);
        entry.level_triggered = true;
        entry.active_low = true;
        let low = entry.encode().0;
        assert_eq!((low >> 15) & 1, 1, "Bit 15 = Level");
        assert_eq!((low >> 13) & 1, 1, "Bit 13 = Polaritaet");
    }

    #[test]
    fn destination_lands_in_the_high_half() {
        let entry = RedirectionEntry::fixed(0x40, 0xAB);
        let (_, high) = entry.encode();
        assert_eq!((high >> 24) & 0xFF, 0xAB);
        assert_eq!(high, 0xAB00_0000);
    }

    #[test]
    fn encode_decode_roundtrip() {
        let entry = RedirectionEntry {
            vector: 0xFE,
            delivery_mode: 4,
            physical_destination: false,
            active_low: true,
            level_triggered: true,
            masked: true,
            destination: 0x7F,
        };
        let (low, high) = entry.encode();
        assert_eq!(RedirectionEntry::decode(low, high), entry);
    }

    #[test]
    fn programming_a_gsi_writes_both_halves_and_reads_the_same_back() {
        let mut apic = IoApic::new(FakeIoApic::new(), 0);
        let wanted = RedirectionEntry::fixed(0x50, 0);
        let read_back = apic.set_redirection_entry(2, wanted).unwrap();
        assert_eq!(read_back.vector, 0x50);
        assert!(read_back.masked);

        // Eintrag 2 liegt bei 0x14/0x15.
        let written: alloc::vec::Vec<u32> = apic.registers.writes.iter().map(|w| w.0).collect();
        assert!(written.contains(&(IOREDTBL_BASE + 4)), "Low-Register 0x14");
        assert!(written.contains(&(IOREDTBL_BASE + 5)), "High-Register 0x15");
    }

    #[test]
    fn mask_and_unmask_only_touch_the_mask_bit() {
        let mut apic = IoApic::new(FakeIoApic::new(), 0);
        let mut entry = RedirectionEntry::fixed(0x33, 0);
        entry.masked = false;
        apic.set_redirection_entry(1, entry).unwrap();

        let masked = apic.mask(1).unwrap();
        assert!(masked.masked);
        assert_eq!(masked.vector, 0x33, "Vektor bleibt erhalten");

        let unmasked = apic.unmask(1).unwrap();
        assert!(!unmasked.masked);
        assert_eq!(unmasked.vector, 0x33);
    }

    #[test]
    fn gsi_mapping_respects_the_basis() {
        let mut apic = IoApic::new(FakeIoApic::new(), 4);
        assert_eq!(
            apic.redirection_entry(3).unwrap_err(),
            IoApicError::GsiBelowBase
        );
        assert!(apic.redirection_entry(4).is_ok(), "GSI 4 ist Index 0");
        assert!(apic.redirection_entry(7).is_ok(), "GSI 7 ist Index 3");
        assert_eq!(
            apic.redirection_entry(8).unwrap_err(),
            IoApicError::GsiOutOfRange
        );
    }
}
