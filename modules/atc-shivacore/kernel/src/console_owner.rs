// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// CONSOLE-001 — Wem die Bildflaeche gehoert
//
// Die Flaeche hat genau einen Schreiber: entweder den Kernel oder den Prozess,
// dem die Console-Cap gehoert. Gespeichert wird die PID, nicht ein Ja/Nein.
//
// Ein blosses Flag war eine Einbahnstrasse: endete der Prozess (Absturz, Ende
// nach dem Neustart-Limit), blieb "gehoert dem Userspace" fuer immer stehen und
// der Kernel konnte die Flaeche nie zurueckholen — der Bildschirm waere tot
// geblieben. Deshalb steht die Regel hier als eigener, ohne QEMU pruefbarer
// Gegenstand und nicht als Feld in der Zeichenroutine.
// ─────────────────────────────────────────────────────────────────────────

use core::sync::atomic::{AtomicU64, Ordering};

/// Der Inhaber der Bildflaeche.
pub struct ConsoleOwner(AtomicU64);

impl ConsoleOwner {
    /// Noch kein Besitzer (der Kernel schreibt).
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    /// Uebergabe an `pid`; true nur beim ersten Mal, damit die Uebergabe einmal
    /// protokolliert wird und nicht bei jedem Leeren.
    pub fn claim(&self, pid: u64) -> bool {
        self.0
            .compare_exchange(0, pid, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Gibt die Flaeche zurueck, wenn `pid` der Besitzer ist. Nur der Besitzer
    /// kann sie freigeben, und danach schreibt wieder der Kernel.
    pub fn release(&self, pid: u64) -> bool {
        self.0
            .compare_exchange(pid, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Aktueller Besitzer (0 = Kernel).
    pub fn owner(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

impl Default for ConsoleOwner {
    fn default() -> Self {
        Self::new()
    }
}

/// Der eine Inhaber der Bildflaeche.
pub static CONSOLE_OWNER: ConsoleOwner = ConsoleOwner::new();

#[cfg(test)]
mod tests {
    use super::ConsoleOwner;

    #[test]
    fn nur_der_erste_uebernimmt() {
        let owner = ConsoleOwner::new();
        assert_eq!(owner.owner(), 0, "der Kernel schreibt");
        assert!(owner.claim(1003));
        assert_eq!(owner.owner(), 1003);
        assert!(!owner.claim(1004), "ein zweiter Schreiber wird nicht Inhaber");
        assert_eq!(owner.owner(), 1003);
    }

    #[test]
    fn nur_der_besitzer_gibt_zurueck() {
        let owner = ConsoleOwner::new();
        assert!(owner.claim(1003));
        assert!(!owner.release(1004), "eine fremde PID gibt nichts frei");
        assert_eq!(owner.owner(), 1003);
        assert!(owner.release(1003));
        assert_eq!(owner.owner(), 0, "der Kernel schreibt wieder");
        assert!(!owner.release(1003), "die zweite Freigabe ist keine");
        assert!(owner.claim(1004), "nach der Rueckgabe darf neu uebergeben werden");
    }

    #[test]
    fn ein_beendeter_dienst_gibt_die_flaeche_frei() {
        // Der Fall, der das Flag frueher zur Einbahnstrasse machte.
        let owner = ConsoleOwner::new();
        assert!(owner.claim(1003));
        assert!(owner.release(1003));
        assert_eq!(owner.owner(), 0);
        assert!(owner.claim(1003), "derselbe Dienst darf sie wieder uebernehmen");
    }
}
