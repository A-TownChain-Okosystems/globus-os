// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! Verzoegerte Freigabe von Seitentabellen-Frames (P1-3B).
//!
//! Der Timer-Handler laeuft im Interrupt-Kontext. Dort physischen Speicher
//! freizugeben ist heikel: der Frame-Allocator ist eine einzige Instanz mit
//! einem Lock, und ein blockierendes `lock()` im Handler kann die Maschine
//! stehen lassen. Deshalb ist das Einsammeln (Reaping) von der Freigabe
//! entkoppelt:
//!
//! 1. Beim Prozessende werden die Frames seines Adressraums in eine
//!    Warteschlange gehaengt (`queue_process`). Das ist reine Buchfuehrung.
//! 2. Der Tick versucht danach `drain()`; das verwendet `try_lock` auf den
//!    einen Allocator. Ist er belegt, bleibt der Auftrag liegen (kein Warten,
//!    kein Blockieren) und der naechste Tick nimmt ihn.
//!
//! Sicherheit gegen vorzeitige Freigabe: ein Auftrag wird nur ausgefuehrt, wenn
//! die Wurzel des Adressraums nicht die aktive CR3 ist und der Prozess nicht
//! gerade laeuft. Gegen Doppelfreigabe wirkt die Freiliste des Allocators
//! (`deallocate_frame` weist Duplikate ab und zaehlt sie) plus die Tatsache,
//! dass ein Auftrag beim Entnehmen aus der Warteschlange verschwindet.
//!
//! Gemeinsame Frames gibt es hier nicht: L4 und PDPT eines Prozesses werden
//! bei `AddressSpace::create` frisch alloziert. Die Kernel-Haelfte wird durch
//! Kopieren der Eintraege geerbt, nicht durch Teilen der Frames.

// Dieses Modul braucht die x86_64-Paging-Typen. Ohne die Boot-Feature wird
// die Kiste fuer andere Ziele gebaut (z. B. als Bibliothek des Boot-Crates);
// dort existiert kein Seitentabellen-Kontext.
#![cfg(feature = "x86-boot")]

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;
use x86_64::structures::paging::{PhysFrame, Size4KiB};

use crate::ats1000::Pid;
use crate::security::{record, AuditResult};

/// Ein Auftrag: die Frames eines Adressraums, der niemandem mehr gehoert.
struct PendingRelease {
    pid: Pid,
    root: u64,
    frames: Vec<PhysFrame>,
}

pub struct FrameReclaimer {
    /// Welche Seitentabellen-Frames gehoeren zu welchem Prozess.
    registry: Mutex<BTreeMap<u64, Vec<PhysFrame>>>,
    pending: Mutex<Vec<PendingRelease>>,
    freed_frames: AtomicUsize,
    busy_retries: AtomicUsize,
    skipped_active: AtomicUsize,
    /// Freigaben, die wegen weiterer aktiver CPUs aufgeschoben wurden.
    held_multi_cpu: AtomicUsize,
}

/// Wie viele CPUs beim Boot aktiv sind. Bei mehr als einer gibt es keinen
/// IPI-Shootdown und damit keinen Nachweis, dass niemand mehr auf die Seite
/// zugreift — dann wird nicht freigegeben (fail-closed).
static ONLINE_CPUS: AtomicUsize = AtomicUsize::new(1);

pub fn set_online_cpus(count: usize) {
    ONLINE_CPUS.store(count.max(1), Ordering::Release);
}

impl FrameReclaimer {
    pub const fn new() -> Self {
        Self {
            registry: Mutex::new(BTreeMap::new()),
            pending: Mutex::new(Vec::new()),
            freed_frames: AtomicUsize::new(0),
            busy_retries: AtomicUsize::new(0),
            skipped_active: AtomicUsize::new(0),
            held_multi_cpu: AtomicUsize::new(0),
        }
    }

    /// Merkt sich die Seitentabellen-Frames eines Prozesses. Wird beim Bau des
    /// Adressraums aufgerufen, nicht beim Prozessstart.
    pub fn register_tables(&self, pid: Pid, frames: Vec<PhysFrame>) {
        if frames.is_empty() {
            return;
        }
        self.registry.lock().insert(pid.0 as u64, frames);
    }

    pub fn registered(&self, pid: Pid) -> usize {
        self.registry
            .lock()
            .get(&(pid.0 as u64))
            .map(|f| f.len())
            .unwrap_or(0)
    }

    pub fn pending_len(&self) -> usize {
        self.pending.lock().len()
    }

    pub fn freed_frames(&self) -> usize {
        self.freed_frames.load(Ordering::Relaxed)
    }

    pub fn busy_retries(&self) -> usize {
        self.busy_retries.load(Ordering::Relaxed)
    }

    /// Haengt die Frames eines beendeten Prozesses zur Freigabe an. Gibt die
    /// Anzahl zurueck; 0 heisst, es war nichts registriert oder es steht schon
    /// ein Auftrag in der Warteschlange (idempotent).
    pub fn queue_process(&self, pid: Pid) -> usize {
        let frames = match self.registry.lock().remove(&(pid.0 as u64)) {
            Some(frames) if !frames.is_empty() => frames,
            _ => return 0,
        };
        let root = frames[0].start_address().as_u64();
        let count = frames.len();
        {
            let mut pending = self.pending.lock();
            if pending.iter().any(|p| p.pid == pid) {
                // Schon vorgemerkt: Registry bleibt geleert, kein zweiter Auftrag.
                return 0;
            }
            pending.push(PendingRelease { pid, root, frames });
        }
        record(
            "kernel",
            "memory.reclaim_queued",
            &alloc::format!("pid={:?} frames={} root={:#x}", pid, count, root),
            AuditResult::Allowed,
        );
        count
    }

    /// Vermerkt, dass der Allocator belegt war. Kein Warten, nur Zaehlen.
    pub fn note_busy(&self) {
        self.busy_retries.fetch_add(1, Ordering::Relaxed);
    }

    pub fn skipped_active(&self) -> usize {
        self.skipped_active.load(Ordering::Relaxed)
    }

    pub fn held_multi_cpu(&self) -> usize {
        self.held_multi_cpu.load(Ordering::Relaxed)
    }

    /// Darf auf dieser Maschine ueberhaupt freigegeben werden?
    fn single_cpu(&self) -> bool {
        ONLINE_CPUS.load(Ordering::Acquire) <= 1
    }

    /// Fuehrt liegengebliebene Auftraege aus. Der Aufrufer reicht die
    /// Freigabe herein (`free_frame`), damit nur EIN Allocator physischen
    /// Speicher zuruecknimmt und diese Instanz keinen zweiten erzeugt.
    ///
    /// Kein Warten: der Aufrufer entscheidet, ob er den Allocator bekommt
    /// (`try_lock`). Ist die Wurzel eines Auftrags die aktive Seitentabelle,
    /// bleibt der Auftrag liegen.
    pub fn drain_with<F>(&self, mut free_frame: F) -> usize
    where
        F: FnMut(PhysFrame<Size4KiB>) -> bool,
    {
        if self.pending.lock().is_empty() {
            return 0;
        }

        // `CR3` zu lesen ist privilegiert; im Testprozess (Ring 3 des
        // Testlaeufers) wuerde das sofort abbrechen. Dort gibt es keinen
        // aktiven Kernel-Adressraum, den man schonen muesste.
        #[cfg(not(test))]
        let active_root = x86_64::registers::control::Cr3::read()
            .0
            .start_address()
            .as_u64();
        #[cfg(test)]
        let active_root = u64::MAX;

        let jobs: Vec<PendingRelease> = {
            let mut pending = self.pending.lock();
            core::mem::take(&mut *pending)
        };
        let mut freed = 0usize;
        let mut requeue: Vec<PendingRelease> = Vec::new();
        for job in jobs {
            if job.root == active_root {
                // Die Seitentabelle ist in Benutzung. Nicht freigeben, spaeter
                // erneut versuchen — eine vorzeitige Freigabe waere ein
                // Use-after-Free der uebelsten Sorte.
                self.skipped_active.fetch_add(1, Ordering::Relaxed);
                requeue.push(job);
                continue;
            }
            if !self.single_cpu() {
                // Mehr als eine CPU: ohne Shootdown laesst sich nicht
                // ausschliessen, dass ein anderer Prozessor den Adressraum
                // noch benutzt. Auftrag bleibt liegen, Zaehler steigt.
                self.held_multi_cpu.fetch_add(1, Ordering::Relaxed);
                requeue.push(job);
                continue;
            }
            // Barriere: alle vorherigen Schreibvorgaenge (Widerruf, TLB-Flush,
            // Zombie-Status) muessen sichtbar sein, bevor Speicher wieder
            // vergeben wird.
            core::sync::atomic::fence(Ordering::SeqCst);
            // Reihenfolge: zuerst die unteren Tabellen, die Wurzel zuletzt.
            // So zeigt kein Pfad auf eine bereits freigegebene Tabelle.
            let mut released = 0usize;
            let mut rejected = 0usize;
            for frame in job.frames.iter().skip(1).chain(job.frames.first()) {
                if free_frame(*frame) {
                    released += 1;
                } else {
                    rejected += 1;
                }
            }
            freed += released;
            self.freed_frames.fetch_add(released, Ordering::Relaxed);
            record(
                "kernel",
                "memory.reclaim",
                &alloc::format!(
                    "pid={:?} root={:#x} freigegeben={} abgewiesen={}",
                    job.pid, job.root, released, rejected
                ),
                AuditResult::Allowed,
            );
        }
        if !requeue.is_empty() {
            self.pending.lock().extend(requeue);
        }
        freed
    }
}

impl Default for FrameReclaimer {
    fn default() -> Self {
        Self::new()
    }
}

pub static RECLAIMER: FrameReclaimer = FrameReclaimer::new();

#[cfg(test)]
mod tests {
    use super::*;
    use x86_64::{PhysAddr, structures::paging::PhysFrame};

    fn frame(addr: u64) -> PhysFrame {
        PhysFrame::containing_address(PhysAddr::new(addr))
    }

    fn pid(n: u32) -> Pid {
        Pid(n)
    }

    #[test]
    fn queue_takes_registered_frames_once() {
        let reclaimer = FrameReclaimer::new();
        reclaimer.register_tables(pid(7), alloc::vec![frame(0x10_0000), frame(0x10_1000)]);
        assert_eq!(reclaimer.registered(pid(7)), 2);
        assert_eq!(reclaimer.queue_process(pid(7)), 2);
        assert_eq!(reclaimer.registered(pid(7)), 0);
        // Zweiter Aufruf findet nichts mehr: Reaping ist idempotent.
        assert_eq!(reclaimer.queue_process(pid(7)), 0);
        assert_eq!(reclaimer.pending_len(), 1);
    }

    #[test]
    fn queue_without_registration_is_a_noop() {
        let reclaimer = FrameReclaimer::new();
        assert_eq!(reclaimer.queue_process(pid(8)), 0);
        assert_eq!(reclaimer.pending_len(), 0);
    }

    #[test]
    fn drain_hands_frames_to_the_single_allocator() {
        let reclaimer = FrameReclaimer::new();
        reclaimer.register_tables(pid(9), alloc::vec![frame(0x20_0000), frame(0x20_1000)]);
        reclaimer.queue_process(pid(9));
        let mut freed_now: Vec<u64> = alloc::vec::Vec::new();
        let freed = reclaimer.drain_with(|f| {
            freed_now.push(f.start_address().as_u64());
            true
        });
        assert_eq!(freed, 2);
        assert_eq!(reclaimer.pending_len(), 0);
        // Untere Tabelle zuerst, Wurzel zuletzt.
        assert_eq!(freed_now, alloc::vec![0x20_1000, 0x20_0000]);
        // Zweiter Lauf hat nichts mehr zu tun: idempotent.
        assert_eq!(reclaimer.drain_with(|_| true), 0);
    }

    #[test]
    fn rejected_frames_are_counted_not_repeated() {
        // Weist der Allocator ab (Doppelfreigabe), wird das gezaehlt und der
        // Auftrag nicht erneut eingereiht.
        let reclaimer = FrameReclaimer::new();
        reclaimer.register_tables(pid(10), alloc::vec![frame(0x40_0000)]);
        reclaimer.queue_process(pid(10));
        assert_eq!(reclaimer.drain_with(|_| false), 0);
        assert_eq!(reclaimer.pending_len(), 0);
        assert_eq!(reclaimer.freed_frames(), 0);
    }


}
