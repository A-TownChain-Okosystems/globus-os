// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// SPAWN-002 — Adressraum zur Laufzeit bauen
//
// Der Bootpfad baut Prozesse von Hand. Fuer einen spaeteren Prozessstart muss
// dasselbe aus dem Syscall-Kontext gehen; damit daraus kein zweiter Weg wird,
// steht es einmal hier, und der Boot-Selbsttest benutzt dieselbe Funktion.
//
// Was der Bau braucht und deshalb beim Boot veroeffentlicht wird: den
// Speicherabstand (um Seitentabellen lesen zu koennen) und die **Kernel-Wurzel**
// als festen Bezugspunkt. Die aktive Wurzel taugt nicht — beim Bau eines
// zweiten Prozesses ist das schon dessen Tabelle, und daraus geerbt wandert der
// Stack des Vorgaengers in jede folgende Tabelle.
// ─────────────────────────────────────────────────────────────────────────

use core::sync::atomic::{AtomicU64, Ordering};
use shivacore::spawn::{SpawnError, layout_for_slot};
use shivacore::userspace::UserBinary;
use x86_64::structures::paging::{PhysFrame, Size4KiB};
use x86_64::{PhysAddr, VirtAddr};

/// Speicherabstand des Bootloaders (0 = noch nicht veroeffentlicht).
pub static PHYS_OFFSET: AtomicU64 = AtomicU64::new(0);
/// Physische Adresse der Kernel-Wurzel (0 = noch nicht veroeffentlicht).
pub static KERNEL_ROOT: AtomicU64 = AtomicU64::new(0);

/// Veroeffentlicht den Bootbezug fuer spaetere Adressraeume.
pub fn publish_boot_reference(physical_memory_offset: u64, kernel_root: u64) {
    PHYS_OFFSET.store(physical_memory_offset, Ordering::SeqCst);
    KERNEL_ROOT.store(kernel_root, Ordering::SeqCst);
}

fn phys_offset() -> Option<VirtAddr> {
    match PHYS_OFFSET.load(Ordering::SeqCst) {
        0 => None,
        offset => Some(VirtAddr::new(offset)),
    }
}

fn kernel_root() -> Option<PhysFrame<Size4KiB>> {
    match KERNEL_ROOT.load(Ordering::SeqCst) {
        0 => None,
        root => Some(PhysFrame::containing_address(PhysAddr::new(root))),
    }
}

/// Gibt die Frames eines verworfenen Adressraums an den Reclaimer zurueck.
///
/// Reihenfolge der Starttransaktion und ihrer Ruecknahme:
/// 1. `build_space` baut den Raum und registriert seine Frames unter `tag`.
/// 2. `register_spawned_process` nimmt den Prozess in den Scheduler auf.
/// 3. `session_authority::transfer` uebergibt die Sitzung an die neue PID.
///
/// Scheitert 2, ist nichts zu entfernen, aber der Raum muss weg: `abandon_space`.
/// Scheitert 3, muss zuerst der Scheduler-Eintrag zurueckgenommen werden
/// (`remove_process`), dann `abandon_space`. Umgekehrt bliebe ein Prozess ohne
/// Frames im Scheduler stehen — ein Eintrag, der nie laufen kann.
///
/// Die Frames werden nur eingereiht, nicht freigegeben: das Abraeumen macht der
/// bestehende Reclaimer-Pfad mit `drain_with`, damit es genau einen Ort gibt,
/// an dem Frames tatsaechlich zurueckgehen.
pub fn abandon_space(tag: u64) -> usize {
    shivacore::reclaim::RECLAIMER.queue_process(shivacore::ats1000::Pid(tag as u32))
}

/// Ein gebauter, noch nicht gestarteter Adressraum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltSpace {
    /// Physische Adresse der Wurzel (der Wert fuer CR3).
    pub cr3: u64,
    /// Anzahl der Frames, die zu diesem Adressraum gehoeren.
    pub frames: usize,
}

/// Baut einen Adressraum fuer `binary` im Fenster von `slot`.
///
/// Reihenfolge wie beim Boot: Wurzel aus der Kernel-Wurzel erben, Programm
/// abbilden, alle Frames beim Reclaimer anmelden. Der Raum wird **nicht**
/// installiert — das tut der Aufrufer, wenn er den Prozess startet.
pub fn build_space(slot: u64, binary: &UserBinary, tag: u64) -> Result<BuiltSpace, SpawnError> {
    let layout = layout_for_slot(slot).ok_or(SpawnError::NoFreeSlot)?;
    let offset = phys_offset().ok_or(SpawnError::NotReady)?;
    let root = kernel_root().ok_or(SpawnError::NotReady)?;
    let mut guard = crate::memory::FRAME_ALLOCATOR.lock();
    let allocator = guard.as_mut().ok_or(SpawnError::NotReady)?;
    // Safety: Quelle ist die Kernel-Wurzel, die selbst keine Nutzerseiten
    // enthaelt; der Speicherabstand ist beim Boot veroeffentlicht.
    let mut space = unsafe {
        shivacore::addr_space::AddressSpace::create(
            offset,
            allocator,
            &layout.mapped_regions(),
            root,
        )
    }
    .ok_or(SpawnError::ResourcesUnavailable)?;
    // `map_binary` schreibt ueber die **virtuellen** Adressen des neuen Raums
    // (wie der Bootpfad, der vorher `install()` ruft). Ein Raum, der nicht
    // aktiv ist, hat diese Adressen nicht — der Schreibzugriff landet dann in
    // einem Seitenfehler. Deshalb wird fuer die Abbildung kurz auf den neuen
    // Raum gewechselt und danach zurueck: die Kernel-Haelfte ist in beiden
    // gleich, laufender Code und Stapel bleiben also gueltig. Unterbrechungen
    // sind fuer die Dauer des Wechsels aus.
    let (previous, flags) = shivacore::addr_space::active();
    let mapped = x86_64::instructions::interrupts::without_interrupts(|| {
        space.install();
        // Safety: `space` ist genau der Raum, dessen Wurzel oben angefordert
        // wurde; abgebildet wird ausschliesslich in dessen Fenster.
        let mapped = unsafe { space.map_binary(allocator, offset, binary, &layout) };
        shivacore::addr_space::restore(previous, flags);
        mapped
    });
    mapped.map_err(|_| SpawnError::MapFailed)?;
    let frames = space.all_frames();
    shivacore::reclaim::RECLAIMER
        .register_tables(shivacore::ats1000::Pid(tag as u32), frames.clone());
    Ok(BuiltSpace {
        cr3: space.frame().start_address().as_u64(),
        frames: frames.len(),
    })
}
