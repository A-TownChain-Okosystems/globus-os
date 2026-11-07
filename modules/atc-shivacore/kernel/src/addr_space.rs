// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// Eigener Adressraum je Prozess
//
// Bis hierher teilten alle Prozesse die Seitentabelle, die der Bootloader
// aufgesetzt hat. Ein Programm konnte deshalb den Stack eines anderen lesen;
// `peek` hat genau das gemessen.
//
// Der Kern dieser Datei ist eine Entscheidung, kein Kopierlauf: Beim Bau einer
// neuen Tabelle wird der untere Adressbereich NICHT pauschal uebernommen.
// Nutzerseiten und Kernel-MMIO liegen in derselben PML4-Position, aber in
// verschiedenen PDPT-Eintraegen (1 GiB je Eintrag):
//
//   Nutzer   0x00400000 … 0x07FFFF00  -> PDPT-Eintrag 0
//   LAPIC    0xFEE00000               -> PDPT-Eintrag 3
//   FB       um 0xFD000000            -> PDPT-Eintrag 3
//
// Also: PDPT-Eintrag 0 bleibt leer, Eintraege 1..512 werden geerbt, die obere
// PML4-Haelfte (Kernel) wird vollstaendig uebernommen. Damit erbt jede neue
// Tabelle jede Geraete- und Kernelabbildung und verliert genau den Bereich, den
// ein Prozess nicht sehen soll — ohne handgepflegte Liste von MMIO-Adressen,
// die still veraltet, sobald ein Geraet dazukommt.
// ─────────────────────────────────────────────────────────────────────────

// Die Tabellenarbeit braucht die x86_64-Crate und damit das Feature
// `x86-boot`. Die Entscheidungen darueber, was geerbt wird, sind reine Zahlen
// und bleiben ohne Feature testbar — sonst waere die einzige Logik, die hier
// schiefgehen kann, genau die ungetestete.
#[cfg(feature = "x86-boot")]
use x86_64::registers::control::{Cr3, Cr3Flags};
#[cfg(feature = "x86-boot")]
use x86_64::structures::paging::{
    FrameAllocator, OffsetPageTable, PageTable, PageTableFlags, PhysFrame, Size4KiB,
};
#[cfg(feature = "x86-boot")]
use x86_64::{PhysAddr, VirtAddr};

/// Erster PML4-Eintrag der Kernel-Haelfte (obere 256 GiB ab hier).
pub const KERNEL_HALF_START: usize = 256;

/// Erster PML4-Eintrag, der unveraendert uebernommen wird.
///
/// Nur Eintrag 0 wird neu gebaut. Ihn auszunehmen und erst bei 256 zu beginnen
/// war ein Fehler mit Ansage: der Heap liegt bei 0x4444_4444_0000 und damit in
/// PML4-Eintrag 8, also zwischen Nutzerbereich und Kernel-Haelfte. Die erste
/// neue Tabelle verlor ihn, der erste Allokationszugriff danach beendete den
/// Boot ohne Logzeile (Dreifachfehler -> Reset). Alles ueber 512 GiB gehoert
/// zum Kernel; Nutzerseiten liegen ausnahmslos unter 1 GiB.
pub const FIRST_INHERITED_PML4: usize = 1;

/// Erster PDPT-Eintrag, der geerbt wird.
///
/// 1 und nicht 0: Eintrag 0 deckt 0…1 GiB ab und enthaelt ausschliesslich
/// Nutzerseiten. Ihn zu erben hiesse, die Trennung wieder aufzugeben.
pub const FIRST_INHERITED_PDPT: usize = 1;

/// Groesse eines PDPT-Fensters: 1 GiB.
pub const PDPT_WINDOW: u64 = 1 << 30;

/// Darf dieser PDPT-Eintrag uebernommen werden?
///
/// Die Antwort haengt an den Regionen des Prozesses, nicht an einer festen
/// Grenze. Der erste Versuch vererbte pauschal alles ab Eintrag 1 und begruendete
/// das mit "Nutzerseiten liegen unter 1 GiB" — falsch: der Standard-Stack liegt
/// bei 0x7FFFF000, also knapp unter 2 GiB und damit in Eintrag 1. Init bekam
/// seinen eigenen Stack damit in JEDE fremde Tabelle geerbt, `peek` las die
/// Marke weiter und die Trennung war nur eine Absicht auf dem Papier.
pub fn inherits_pdpt_entry(index: usize, user_regions: &[(u64, u64)]) -> bool {
    if index < FIRST_INHERITED_PDPT {
        return false;
    }
    !user_regions.iter().any(|(start, end_exclusive)| {
        let first = (start / PDPT_WINDOW) as usize;
        let last = (end_exclusive.saturating_sub(1) / PDPT_WINDOW) as usize;
        index >= first && index <= last
    })
}

/// Ein Adressraum: die Wurzel seiner Seitentabelle.
#[cfg(feature = "x86-boot")]
pub struct AddressSpace {
    frame: PhysFrame<Size4KiB>,
}

#[cfg(feature = "x86-boot")]
impl AddressSpace {
    /// Baut eine neue Tabelle, die Kernel und Geraete erbt.
    ///
    /// # Safety
    /// `physical_memory_offset` muss das physische Fenster sein, ueber das die
    /// Seitentabellen erreichbar sind; `allocator` muss exklusiv verwendet
    /// werden.
    /// `source` ist die Tabelle, aus der geerbt wird — die Boot-Tabelle, NICHT
    /// die gerade aktive.
    ///
    /// Der Unterschied ist der ganze Fehler: beim Bau von Prozess 2 ist die
    /// aktive Tabelle schon die von Prozess 1. Wird daraus geerbt, wandert
    /// dessen Stack in jede folgende Tabelle, und die Trennung ist wieder weg.
    /// Die Quelle muss ein fester Bezugspunkt sein, der selbst keine
    /// Nutzerseiten enthaelt.
    pub unsafe fn create(
        physical_memory_offset: VirtAddr,
        allocator: &mut impl FrameAllocator<Size4KiB>,
        user_regions: &[(u64, u64)],
        source: PhysFrame<Size4KiB>,
    ) -> Option<Self> {
        let table = allocator.allocate_frame()?;
        let table_ptr =
            (physical_memory_offset + table.start_address().as_u64()).as_mut_ptr::<PageTable>();
        table_ptr.write(PageTable::new());

        let active_ptr =
            (physical_memory_offset + source.start_address().as_u64()).as_ptr::<PageTable>();
        // Edition 2024 verbietet implizite Autoref auf rohe Zeiger: die
        // Referenzen werden einmal ausgesprochen, danach ist der Zugriff klar.
        let dst: &mut PageTable = &mut *table_ptr;
        let src: &PageTable = &*active_ptr;

        // Alles ausser Eintrag 0: Kernel-Haelfte, Heap, physikalisches Fenster.
        for index in FIRST_INHERITED_PML4..512 {
            dst[index] = src[index].clone();
        }

        // Untere Haelfte: eigener PDPT, geerbte Geraeteabbildungen.
        let active_low = src[0].clone();
        if active_low.flags().contains(PageTableFlags::PRESENT) {
            let pdpt = allocator.allocate_frame()?;
            let pdpt_ptr =
                (physical_memory_offset + pdpt.start_address().as_u64()).as_mut_ptr::<PageTable>();
            pdpt_ptr.write(PageTable::new());
            let active_pdpt =
                (physical_memory_offset + active_low.addr().as_u64()).as_ptr::<PageTable>();
            let pdpt_dst: &mut PageTable = &mut *pdpt_ptr;
            let pdpt_src: &PageTable = &*active_pdpt;
            for index in FIRST_INHERITED_PDPT..512 {
                if inherits_pdpt_entry(index, user_regions) {
                    pdpt_dst[index] = pdpt_src[index].clone();
                }
            }
            let mut entry = x86_64::structures::paging::page_table::PageTableEntry::new();
            entry.set_addr(
                PhysAddr::new(pdpt.start_address().as_u64()),
                active_low.flags(),
            );
            dst[0] = entry;
        }

        Some(Self { frame: table })
    }

    /// Mapper fuer genau diese Tabelle.
    ///
    /// # Safety
    /// Es darf immer nur ein Mapper je Tabelle gleichzeitig benutzt werden.
    pub unsafe fn mapper(&self, physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
        let ptr = (physical_memory_offset + self.frame.start_address().as_u64())
            .as_mut_ptr::<PageTable>();
        OffsetPageTable::new(&mut *ptr, physical_memory_offset)
    }

    pub fn frame(&self) -> PhysFrame<Size4KiB> {
        self.frame
    }

    /// Schaltet auf diesen Adressraum um.
    pub fn install(&self) {
        unsafe { Cr3::write(self.frame, Cr3Flags::empty()) }
    }
}

/// Das aktuell aktive CR3, zum Zurueckschalten.
#[cfg(feature = "x86-boot")]
pub fn active() -> (PhysFrame<Size4KiB>, Cr3Flags) {
    Cr3::read()
}

/// Schaltet auf eine Tabelle um, die nur als physische Adresse bekannt ist.
///
/// So kommt der Wert aus dem Scheduler: dort steht eine Zahl, keine Struktur.
/// 0 bedeutet "noch keine eigene Tabelle" und laesst das CR3 unberuehrt.
#[cfg(feature = "x86-boot")]
pub fn install_raw(cr3: u64) {
    if cr3 == 0 {
        return;
    }
    let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(cr3));
    unsafe { Cr3::write(frame, Cr3Flags::empty()) }
}

/// Stellt einen frueher gelesenen Adressraum wieder her.
#[cfg(feature = "x86-boot")]
pub fn restore(frame: PhysFrame<Size4KiB>, flags: Cr3Flags) {
    unsafe { Cr3::write(frame, flags) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_user_gigabyte_is_not_inherited() {
        // Genau dieser Eintrag enthaelt Code, Daten und Stack aller Prozesse.
        assert!(!inherits_pdpt_entry(0, &[]));
    }

    #[test]
    fn device_mappings_above_one_gib_are_inherited() {
        // 0xFEE00000 (LAPIC) und das Framebuffer-Fenster liegen hier.
        assert!(inherits_pdpt_entry(3, &[]));
        assert!(inherits_pdpt_entry(511, &[]));
    }

    #[test]
    fn a_stack_near_two_gib_removes_its_window() {
        // Der Standard-Stack liegt bei 0x7FFFF000: knapp unter 2 GiB, also in
        // Eintrag 1. Ohne diesen Test waere die Luecke wieder offen, denn
        // optisch sieht "unter 2 GiB" nach Nutzerbereich aus — und genau
        // deshalb wurde sie beim ersten Versuch uebersehen.
        let stack = (0x7FFF_0000u64, 0x8000_0000u64);
        assert_eq!((stack.0 >> 30) as usize, 1);
        assert!(!inherits_pdpt_entry(1, &[stack]));
        // Eintrag 0 wird IMMER neu gebaut, unabhaengig von den Regionen.
        assert!(!inherits_pdpt_entry(0, &[stack]));
        // LAPIC bleibt unberuehrt, obwohl die Region existiert.
        assert!(inherits_pdpt_entry(3, &[stack]));
    }

    #[test]
    fn a_region_spanning_a_window_boundary_removes_both_windows() {
        // Schutz gegen die naechste Annahme: eine Region, die eine 1-GiB-Grenze
        // ueberschreitet, muss beide Fenster entfernen, nicht nur das erste.
        let spanning = (0x3FFF_0000u64, 0x4001_0000u64);
        assert_eq!((spanning.0 >> 30) as usize, 0);
        assert_eq!(((spanning.1 - 1) >> 30) as usize, 1);
        assert!(!inherits_pdpt_entry(0, &[spanning]));
        assert!(!inherits_pdpt_entry(1, &[spanning]));
        assert!(inherits_pdpt_entry(2, &[spanning]));
    }

    #[test]
    fn everything_but_the_low_entry_is_inherited() {
        // Der Heap bei 0x4444_4444_0000 liegt in Eintrag 8 — zwischen
        // Nutzerbereich und Kernel-Haelfte. Genau ihn hat die erste Fassung
        // verloren, deshalb steht dieser Test hier.
        assert_eq!(FIRST_INHERITED_PML4, 1);
        let heap_index = (0x4444_4444_0000u64 >> 39) as usize;
        assert!(heap_index >= FIRST_INHERITED_PML4);
        // Nachgerechnet, nicht geschaetzt: 0x4444_4444_0000 >> 39 = 136.
        assert_eq!(heap_index, 136);
    }

    #[test]
    fn user_pages_live_in_the_rebuilt_entry() {
        let code_index = (0x0040_0000u64 >> 39) as usize;
        let mmio_index = (0xFEE0_0000u64 >> 39) as usize;
        assert_eq!(code_index, 0);
        assert_eq!(mmio_index, 0);
        // Beide im selben PML4-Eintrag: deshalb entscheidet der PDPT-Eintrag.
        assert!(!inherits_pdpt_entry(0, &[]));
        assert!(inherits_pdpt_entry(3, &[]));
    }

    #[test]
    fn kernel_half_starts_at_the_256th_entry() {
        // Eintrag 256 adressiert linear 0x0000_8000_0000_0000. Erst die
        // Vorzeichenerweiterung des 48-Bit-Raums macht daraus die obere
        // Haelfte. Die Multiplikation 256 * 512 GiB vergisst genau diesen
        // Schritt: sie ergibt 0x8000_0000_0000 und sieht nur auf den ersten
        // Blick wie die Kernel-Grenze aus.
        assert_eq!(KERNEL_HALF_START, 256);
        let linear = (KERNEL_HALF_START as u64) << 39;
        assert_eq!(linear, 0x0000_8000_0000_0000);
        assert_eq!(linear | 0xFFFF_0000_0000_0000, 0xFFFF_8000_0000_0000);
    }
}
