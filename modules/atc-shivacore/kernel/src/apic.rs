// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// SC-006 REQ-SC006-03 — LAPIC-Registerschicht (Hardware-Seite der Maskierung)
// Kernel Layer | Chain-ID 658467
//
// `irq::IrqTable` haelt fest, ob eine Line maskiert ist. Diese Schicht setzt
// die Maske tatsaechlich: pro Zustellung wird der Vektor im Interrupt-Enable-
// Register gesperrt, nach dem ACK per EOI quittiert und wieder freigegeben.
//
// Die MMIO-Zugriffe laufen ueber `ApicRegisters`, damit die Registerlogik ohne
// Hardware pruefbar ist. Der reale Zugriff (Mapping von LAPIC_DEFAULT_BASE)
// ist noch nicht angebunden; ein QEMU-Bootlauf steht weiterhin aus.
// ─────────────────────────────────────────────────────────────────────────

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Standardbasis des Local APIC (Physical Address, ueblich per ACPI/MADT).
pub const LAPIC_DEFAULT_BASE: u64 = 0xFEE0_0000;

/// Local APIC ID Register.
pub const REG_ID: u32 = 0x020;
/// Task Priority Register.
pub const REG_TASK_PRIORITY: u32 = 0x080;
/// End of Interrupt.
pub const REG_EOI: u32 = 0x0B0;

/// true = Interrupts werden ueber den IOAPIC zugestellt, die Bestaetigung
/// laeuft dann ueber den Local APIC (EOI-Register 0x0B0) und nicht ueber die PIC.
static DELIVERY_VIA_APIC: AtomicBool = AtomicBool::new(false);
/// Basis des gemappten LAPIC-Fensters fuer die Bestaetigung im IRQ-Pfad.
static EOI_BASE: AtomicU64 = AtomicU64::new(0);

/// Stellt den Bestaetigungsweg auf den Local APIC um. Muss erfolgen, bevor der
/// IOAPIC-Eintrag freigegeben wird, sonst bleibt der erste Interrupt haengen.
pub fn switch_acknowledge_to_apic(mapped_base: u64) {
    EOI_BASE.store(mapped_base, Ordering::Release);
    DELIVERY_VIA_APIC.store(true, Ordering::Release);
}

/// Gemappte LAPIC-Basis (0 = nicht gemappt).
pub fn apic_base() -> u64 {
    EOI_BASE.load(Ordering::Acquire)
}

/// Meldet, ob der Local-APIC-Weg aktiv ist.
pub fn acknowledges_via_apic() -> bool {
    DELIVERY_VIA_APIC.load(Ordering::Acquire)
}

/// Quittiert den laufenden Interrupt am Local APIC. Ohne umgestellten Weg und
/// ohne gemappte Basis passiert nichts.
pub fn acknowledge_interrupt() {
    if !acknowledges_via_apic() {
        return;
    }
    let base = EOI_BASE.load(Ordering::Acquire);
    if base == 0 {
        return;
    }
    // SAFETY: base stammt aus dem gemappten LAPIC-Fenster (map_mmio_page) und
    // deckt das EOI-Register bei Offset 0x0B0 ab.
    unsafe {
        core::ptr::write_volatile((base + EOI_BASE_OFFSET) as *mut u32, 0);
    }
}

/// Zaehlt Sperren und Freigaben im echten Interrupt-Pfad. Beide muessen gleich
/// oft auftreten; ein Auseinanderlaufen hiesse, dass eine Line haengen bleibt.
static MASK_COUNT: AtomicU64 = AtomicU64::new(0);
static UNMASK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Sperrt einen Vektor im Interrupt-Enable-Register des gemappten LAPIC.
/// Rueckgabe sagt, ob geschrieben wurde (falsch ohne gemappte Basis).
pub fn mask_interrupt_vector(vector: u8) -> bool {
    set_vector_mask(vector, true)
}

/// Gibt den Vektor wieder frei. Das ist die eigentliche ACK-Handlung: ohne
/// Freigabe bleibt die Line stumm.
pub fn unmask_interrupt_vector(vector: u8) -> bool {
    set_vector_mask(vector, false)
}

fn set_vector_mask(vector: u8, masked: bool) -> bool {
    let base = EOI_BASE.load(Ordering::Acquire);
    if base == 0 {
        return false;
    }
    // Nutzvektoren liegen ab 32, deshalb ist das Wort nie 0 und der Offset
    // bleibt im 4-KiB-Fenster (Wort 7 endet bei 0x3F0).
    let word = u32::from(vector) / 32;
    let bit = u32::from(vector) % 32;
    let address = (base + u64::from(REG_IRQ_ENABLE) + u64::from(word) * u64::from(REGISTER_STRIDE))
        as *mut u32;
    // SAFETY: address liegt im gemappten LAPIC-Fenster (map_mmio_page deckt
    // 0x350 bis 0x3F0 ab), Zugriff volatil, weil Hardware mitliest.
    unsafe {
        let current = core::ptr::read_volatile(address);
        let next = if masked {
            current | (1 << bit)
        } else {
            current & !(1 << bit)
        };
        core::ptr::write_volatile(address, next);
    }
    if masked {
        MASK_COUNT.fetch_add(1, Ordering::Relaxed);
    } else {
        UNMASK_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    true
}

/// Stand der Sperren und Freigaben im Interrupt-Pfad.
pub fn mask_unmask_counts() -> (u64, u64) {
    (
        MASK_COUNT.load(Ordering::Relaxed),
        UNMASK_COUNT.load(Ordering::Relaxed),
    )
}

const EOI_BASE_OFFSET: u64 = 0x0B0;
/// Spurious Interrupt Vector Register.
pub const REG_SPURIOUS: u32 = 0x0F0;
/// Erstes In-Service-Register (8 Stueck, Schrittweite 0x10).
pub const REG_IN_SERVICE: u32 = 0x100;
/// Erstes Interrupt-Enable-Register (Schrittweite 0x10).
pub const REG_IRQ_ENABLE: u32 = 0x350;

const REGISTER_STRIDE: u32 = 0x10;
const SPURIOUS_ENABLE: u32 = 1 << 8;

/// Vektor des Spurious Interrupts.
pub const SPURIOUS_VECTOR: u32 = 0xFF;
/// Erster Vektor, der als Geraete-IRQ verwendet werden darf.
pub const FIRST_USABLE_VECTOR: u32 = 32;
/// Letzter Vektor, der als Geraete-IRQ verwendet werden darf.
pub const LAST_USABLE_VECTOR: u32 = 255;

/// Fehler der Vektorverwaltung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApicError {
    /// Vektor liegt im Bereich der Prozessor-Exceptions (0..32) oder ueber 255.
    ReservedVector,
}

/// Zugriff auf die LAPIC-Register. Ein Test kann das durch ein Array ersetzen,
/// der Kernel durch ein gemapptes MMIO-Fenster.
pub trait ApicRegisters {
    fn read(&self, offset: u32) -> u32;
    fn write(&mut self, offset: u32, value: u32);
}

/// Registersatz auf einem gemappten MMIO-Fenster. Alle Zugriffe sind volatil,
/// weil die Hardware die Werte hinter dem Ruecken des Compilers aendert.
pub struct MappedRegisters {
    base: *mut u32,
}

impl MappedRegisters {
    /// # Safety
    /// `base` muss auf ein gemapptes, 4 KiB ausgerichtetes LAPIC-Fenster zeigen,
    /// das mindestens bis Offset 0x3F0 reicht.
    pub unsafe fn new(base: *mut u32) -> Self {
        Self { base }
    }

    fn slot(&self, offset: u32) -> *mut u32 {
        // Register liegen auf 16-Byte-Grenzen; das Fenster wird als u32 gelesen.
        unsafe { self.base.add((offset / 4) as usize) }
    }
}

impl ApicRegisters for MappedRegisters {
    fn read(&self, offset: u32) -> u32 {
        unsafe { core::ptr::read_volatile(self.slot(offset)) }
    }

    fn write(&mut self, offset: u32, value: u32) {
        unsafe { core::ptr::write_volatile(self.slot(offset), value) }
    }
}

/// Local APIC mit Masken-, Quittungs- und In-Service-Abfrage.
pub struct LocalApic<M: ApicRegisters> {
    regs: M,
}

impl<M: ApicRegisters> LocalApic<M> {
    pub fn new(regs: M) -> Self {
        Self { regs }
    }

    pub fn registers(&self) -> &M {
        &self.regs
    }

    /// Nur fuer Tests: direkter Zugriff auf den Registersatz, um Hardware-
    /// Zustaende (ISR, EOI) vorzubelegen.
    #[cfg(test)]
    pub fn registers_mut_for_test(&mut self) -> &mut M {
        &mut self.regs
    }

    /// Schaltet den APIC ein und setzt den Spurious-Vektor.
    pub fn enable(&mut self) -> u32 {
        let current = self.regs.read(REG_SPURIOUS);
        let value = (current & !0xFF) | SPURIOUS_VECTOR | SPURIOUS_ENABLE;
        self.regs.write(REG_SPURIOUS, value);
        value
    }

    /// Meldet, ob der APIC scharf ist (Enable-Bit im Spurious-Register).
    pub fn is_enabled(&self) -> bool {
        self.regs.read(REG_SPURIOUS) & SPURIOUS_ENABLE != 0
    }

    pub fn id(&self) -> u8 {
        (self.regs.read(REG_ID) >> 24) as u8
    }

    pub fn set_task_priority(&mut self, priority: u8) {
        self.regs
            .write(REG_TASK_PRIORITY, u32::from(priority) << 24);
    }

    /// Sperrt einen Vektor im Interrupt-Enable-Register.
    pub fn mask(&mut self, vector: u32) -> Result<(), ApicError> {
        let (offset, bit) = Self::location(vector)?;
        let current = self.regs.read(offset);
        self.regs.write(offset, current | (1 << bit));
        Ok(())
    }

    /// Gibt einen Vektor wieder frei.
    pub fn unmask(&mut self, vector: u32) -> Result<(), ApicError> {
        let (offset, bit) = Self::location(vector)?;
        let current = self.regs.read(offset);
        self.regs.write(offset, current & !(1 << bit));
        Ok(())
    }

    /// Ist der Vektor aktuell gesperrt?
    pub fn is_masked(&self, vector: u32) -> Result<bool, ApicError> {
        let (offset, bit) = Self::location(vector)?;
        Ok(self.regs.read(offset) & (1 << bit) != 0)
    }

    /// Quittiert den laufenden Interrupt. Ohne EOI bleibt ein maskierter
    /// Vektor maskiert, deshalb gehoert dieser Aufruf zum ACK dazu.
    pub fn eoi(&mut self) {
        self.regs.write(REG_EOI, 0);
    }

    /// Steht der Vektor gerade in Bearbeitung?
    pub fn in_service(&self, vector: u32) -> Result<bool, ApicError> {
        let (offset, bit) = Self::location(vector)?;
        let offset = offset - REG_IRQ_ENABLE + REG_IN_SERVICE;
        Ok(self.regs.read(offset) & (1 << bit) != 0)
    }

    /// Registeradresse und Bitposition eines Vektors innerhalb der Wortpaare.
    fn location(vector: u32) -> Result<(u32, u32), ApicError> {
        if !(FIRST_USABLE_VECTOR..=LAST_USABLE_VECTOR).contains(&vector) {
            return Err(ApicError::ReservedVector);
        }
        let word = vector / 32;
        Ok((REG_IRQ_ENABLE + word * REGISTER_STRIDE, vector % 32))
    }
}

#[cfg(test)]
mod sc006_lapic_masking {
    use super::*;

    /// Registersatz als flaches Array; deckt alle Offsets bis 0x400 ab.
    struct FakeApic {
        words: [u32; 256],
    }

    impl FakeApic {
        fn new() -> Self {
            Self { words: [0; 256] }
        }
    }

    impl ApicRegisters for FakeApic {
        fn read(&self, offset: u32) -> u32 {
            self.words[(offset / 4) as usize]
        }

        fn write(&mut self, offset: u32, value: u32) {
            self.words[(offset / 4) as usize] = value;
        }
    }

    fn apic() -> LocalApic<FakeApic> {
        LocalApic::new(FakeApic::new())
    }

    #[test]
    fn masking_without_a_mapped_base_writes_nothing() {
        // Vor dem Mapping ist EOI_BASE null: die Helfer duerfen dann weder
        // schreiben noch zaehlen, sonst behauptet der Bootpfad eine Sperre,
        // die es nicht gibt.
        assert_eq!(apic_base(), 0);
        assert!(!mask_interrupt_vector(0x20));
        assert!(!unmask_interrupt_vector(0x20));
        assert_eq!(mask_unmask_counts(), (0, 0));
    }

    #[test]
    fn enable_sets_spurious_vector_and_enable_bit() {
        let mut apic = apic();
        assert!(!apic.is_enabled());
        let spurious = apic.enable();
        assert!(apic.is_enabled());
        assert_eq!(spurious & 0xFF, SPURIOUS_VECTOR);
        assert_eq!(spurious & SPURIOUS_ENABLE, SPURIOUS_ENABLE);
    }

    #[test]
    fn reserved_vectors_are_refused() {
        let mut apic = apic();
        for vector in [0, 1, 31, 256] {
            assert_eq!(apic.mask(vector), Err(ApicError::ReservedVector));
            assert_eq!(apic.unmask(vector), Err(ApicError::ReservedVector));
            assert_eq!(apic.is_masked(vector), Err(ApicError::ReservedVector));
        }
        // Nichts geschrieben: alle Enable-Register bleiben null.
        assert_eq!(apic.registers().read(REG_IRQ_ENABLE), 0);
    }

    #[test]
    fn mask_and_unmask_roundtrip() {
        let mut apic = apic();
        assert!(!apic.is_masked(40).unwrap());
        apic.mask(40).unwrap();
        assert!(apic.is_masked(40).unwrap());
        // Vektor 40 liegt in Wort 1 (32..63), also im zweiten Enable-Register.
        assert_eq!(
            apic.registers().read(REG_IRQ_ENABLE + REGISTER_STRIDE),
            1 << 8
        );
        assert_eq!(
            apic.registers().read(REG_IRQ_ENABLE),
            0,
            "Wort 0 bleibt frei"
        );
        apic.unmask(40).unwrap();
        assert!(!apic.is_masked(40).unwrap());
    }

    #[test]
    fn high_vectors_use_the_third_enable_word() {
        let mut apic = apic();
        apic.mask(70).unwrap();
        assert_eq!(
            apic.registers().read(REG_IRQ_ENABLE + 2 * REGISTER_STRIDE),
            1 << 6
        );
        assert_eq!(apic.registers().read(REG_IRQ_ENABLE + REGISTER_STRIDE), 0);
    }

    #[test]
    fn vectors_in_one_word_are_independent() {
        let mut apic = apic();
        apic.mask(40).unwrap();
        apic.mask(47).unwrap();
        assert_eq!(
            apic.registers().read(REG_IRQ_ENABLE + REGISTER_STRIDE),
            (1 << 8) | (1 << 15)
        );
        apic.unmask(40).unwrap();
        assert!(!apic.is_masked(40).unwrap());
        assert!(apic.is_masked(47).unwrap(), "Nachbarvektor bleibt gesperrt");
    }

    #[test]
    fn eoi_writes_the_eoi_register() {
        let mut apic = apic();
        apic.registers_mut_for_test().write(REG_EOI, 0xFFFF_FFFF);
        apic.eoi();
        assert_eq!(apic.registers().read(REG_EOI), 0);
    }

    #[test]
    fn in_service_reads_the_isr_word() {
        let apic = {
            let mut apic = apic();
            // Vektor 35 liegt in ISR-Wort 1 (32..63), also Offset 0x110.
            apic.registers_mut_for_test()
                .write(REG_IN_SERVICE + REGISTER_STRIDE, 1 << 3);
            apic
        };
        assert!(apic.in_service(35).unwrap(), "Vektor 35 liegt auf Bit 3");
        assert!(!apic.in_service(36).unwrap());
    }

    #[test]
    fn ack_releases_the_vector_only_after_eoi() {
        let mut apic = apic();
        apic.mask(33).unwrap();
        // ACK-Reihenfolge nach SC-006: EOI, dann Maske loesen.
        apic.eoi();
        assert!(apic.is_masked(33).unwrap(), "Maske haelt bis zum Freigeben");
        apic.unmask(33).unwrap();
        assert!(!apic.is_masked(33).unwrap());
    }
}
