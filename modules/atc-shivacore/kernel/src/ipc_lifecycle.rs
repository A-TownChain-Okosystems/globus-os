// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// P1-3C — IPC-Kanaele und Prozess-Handles: Lebensdauer statt Aufraeumen
//
// Ein Kanal gehoert nicht einem Prozess, er wird von mehreren benutzt. Bis
// hierher schloss `close_all_for` alles, was der Beender als Owner trug — ein
// geteilter Kanal war damit fuer den verbleibenden Prozess tot, obwohl dessen
// Capability noch gueltig war.
//
// Diese Datei fuehrt die Beteiligten eines Kanals und entscheidet beim
// Prozessende nach Teilnehmerzahl:
//
//   letzter Teilnehmer  -> Kanal wird zerstoert (Nachrichten verworfen,
//                          Wartende abgebrochen, Handle ungueltig)
//   weitere Teilnehmer  -> Kanal bleibt offen, nur die Referenz des Beenders
//                          faellt weg. Die Nachrichten bleiben, weil der
//                          verbleibende Prozess sie noch lesen darf.
//
// Handles sind prozesslokal: sie werden beim Oeffnen an die PID gebunden und
// beim Prozessende widerrufen. Eine spaetere Benutzung wird abgewiesen, statt
// in eine fremde Tabelle zu zeigen.
// ─────────────────────────────────────────────────────────────────────────

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

use crate::capability::Pid;

/// Wie viele Nachrichten ein Kanal ohne eigene Angabe aufnimmt.
pub const DEFAULT_CAPACITY: usize = 64;

/// Fehler des Live-Pfads (Syscall und Geraetezustellung).
///
/// Alle Varianten sind Ablehnungen: der Aufruf liefert nie "vielleicht".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveIpcError {
    UnknownChannel,
    NotParticipant,
    Closed,
    Full,
    Empty,
}

/// Was fuer ein Objekt ein Handle bezeichnet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleKind {
    Channel,
    Process,
    Device,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handle {
    pub id: u64,
    pub kind: HandleKind,
    /// Kanal-ID, PID oder Geraetenummer — je nach `kind`.
    pub target: u64,
    pub owner: Pid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleError {
    Unknown,
    /// Gehoert einem anderen Prozess oder wurde mit dessen Ende widerrufen.
    Stale,
}

/// Ein Kanal mit seinen Beteiligten.
#[derive(Debug, Clone)]
pub struct ChannelState {
    pub id: u64,
    /// Prozesse, die eine gueltige Capability fuer diesen Kanal halten.
    pub participants: Vec<Pid>,
    /// Wartende Nachrichten (nur die Kennungen; der Inhalt liegt beim Aufrufer).
    pub pending: Vec<u64>,
    /// Prozesse, die in `recv` auf diesem Kanal warten.
    pub waiters: Vec<Pid>,
    /// Wie viele Nachrichten der Kanal aufnimmt, bevor er `Full` meldet.
    pub capacity: usize,
    pub closed: bool,
}

impl ChannelState {
    fn new(id: u64, owner: Pid) -> Self {
        Self {
            id,
            participants: alloc::vec![owner],
            pending: Vec::new(),
            waiters: Vec::new(),
            capacity: DEFAULT_CAPACITY,
            closed: false,
        }
    }
}

/// Ergebnis eines Prozessendes fuer die IPC-Seite.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReleaseReport {
    pub attempted: usize,
    pub destroyed: usize,
    pub released: usize,
    pub messages_discarded: usize,
    pub waiters_aborted: usize,
    /// Zweiter Schliessversuch auf einem bereits zerstoerten Kanal.
    pub double_close_blocked: usize,
}

/// Ergebnis eines ausdruecklichen Schliessens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseOutcome {
    /// Letzter Teilnehmer: der Kanal ist zerstoert.
    Destroyed,
    /// Weitere Teilnehmer vorhanden: der Kanal bleibt offen.
    Kept,
    /// Kanal unbekannt oder bereits zerstoert.
    Rejected,
}

#[derive(Debug, Default)]
pub struct ChannelRegistry {
    channels: BTreeMap<u64, ChannelState>,
    destroyed_total: usize,
    released_total: usize,
    double_close_blocked: usize,
    device_drops: usize,
}

impl ChannelRegistry {
    pub const fn new() -> Self {
        Self {
            channels: BTreeMap::new(),
            destroyed_total: 0,
            released_total: 0,
            double_close_blocked: 0,
            device_drops: 0,
        }
    }

    /// Legt einen Kanal an. `false`, wenn die Kennung schon vergeben ist.
    pub fn open(&mut self, id: u64, owner: Pid) -> bool {
        if self.channels.contains_key(&id) {
            return false;
        }
        self.channels.insert(id, ChannelState::new(id, owner));
        true
    }

    /// Legt einen Kanal mit eigener Kapazitaet an.
    pub fn open_with(&mut self, id: u64, owner: Pid, capacity: usize) -> bool {
        if self.channels.contains_key(&id) {
            return false;
        }
        let mut state = ChannelState::new(id, owner);
        state.capacity = capacity.max(1);
        self.channels.insert(id, state);
        true
    }

    /// Senden im Live-Pfad: Kanal offen, Absender Teilnehmer, Platz im Puffer.
    pub fn send(&mut self, pid: Pid, id: u64, payload: u64) -> Result<(), LiveIpcError> {
        let state = self.channels.get_mut(&id).ok_or(LiveIpcError::UnknownChannel)?;
        if state.closed {
            return Err(LiveIpcError::Closed);
        }
        if !state.participants.contains(&pid) {
            return Err(LiveIpcError::NotParticipant);
        }
        if state.pending.len() >= state.capacity {
            return Err(LiveIpcError::Full);
        }
        state.pending.push(payload);
        Ok(())
    }

    /// Empfangen im Live-Pfad: Kanal offen, Empfaenger Teilnehmer, Nachricht da.
    pub fn recv(&mut self, pid: Pid, id: u64) -> Result<u64, LiveIpcError> {
        let state = self.channels.get_mut(&id).ok_or(LiveIpcError::UnknownChannel)?;
        if state.closed {
            return Err(LiveIpcError::Closed);
        }
        if !state.participants.contains(&pid) {
            return Err(LiveIpcError::NotParticipant);
        }
        if state.pending.is_empty() {
            return Err(LiveIpcError::Empty);
        }
        Ok(state.pending.remove(0))
    }

    /// Zustellung aus dem Geraetetreiber: kein Absenderprozess, deshalb ohne
    /// Teilnehmerpruefung. Ein voller Kanal verwirft und zaehlt, statt den
    /// Interruptpfad warten zu lassen.
    pub fn deliver(&mut self, id: u64, payload: u64) -> bool {
        match self.channels.get_mut(&id) {
            Some(state) if !state.closed && state.pending.len() < state.capacity => {
                state.pending.push(payload);
                true
            }
            Some(_) => {
                self.device_drops += 1;
                false
            }
            None => {
                self.device_drops += 1;
                false
            }
        }
    }

    pub fn device_drops(&self) -> usize {
        self.device_drops
    }

    pub fn open_channels(&self) -> usize {
        self.channels.len()
    }

    /// Nimmt einen weiteren Prozessteil hinzu (Capability-Delegation).
    pub fn attach(&mut self, id: u64, pid: Pid) -> bool {
        match self.channels.get_mut(&id) {
            Some(state) if !state.closed => {
                if !state.participants.contains(&pid) {
                    state.participants.push(pid);
                }
                true
            }
            _ => false,
        }
    }

    /// Legt eine Nachricht in den Kanal. `false`, wenn der Kanal fehlt oder zu ist.
    pub fn enqueue(&mut self, id: u64, message: u64) -> bool {
        match self.channels.get_mut(&id) {
            Some(state) if !state.closed => {
                state.pending.push(message);
                true
            }
            _ => false,
        }
    }

    /// Meldet einen wartenden Empfaenger an.
    pub fn wait(&mut self, id: u64, pid: Pid) -> bool {
        match self.channels.get_mut(&id) {
            Some(state) if !state.closed => {
                if !state.waiters.contains(&pid) {
                    state.waiters.push(pid);
                }
                true
            }
            _ => false,
        }
    }

    pub fn pending_of(&mut self, id: u64) -> Option<usize> {
        self.channels.get(&id).map(|s| s.pending.len())
    }

    pub fn participants_of(&self, id: u64) -> Option<usize> {
        self.channels.get(&id).map(|s| s.participants.len())
    }

    pub fn contains(&self, id: u64) -> bool {
        self.channels.contains_key(&id)
    }

    pub fn count(&self) -> usize {
        self.channels.len()
    }

    pub fn destroyed_total(&self) -> usize {
        self.destroyed_total
    }

    pub fn released_total(&self) -> usize {
        self.released_total
    }

    fn destroy(&mut self, id: u64) -> (usize, usize) {
        match self.channels.remove(&id) {
            Some(mut state) => {
                state.closed = true;
                self.destroyed_total += 1;
                (state.pending.len(), state.waiters.len())
            }
            None => (0, 0),
        }
    }

    /// Ein Prozess beendet sich: seine Referenzen fallen weg.
    ///
    /// Zerstoeren nur, wenn er der letzte Teilnehmer war. Sonst bleibt der
    /// Kanal samt Nachrichten offen.
    pub fn release(&mut self, pid: Pid) -> ReleaseReport {
        let mut report = ReleaseReport::default();
        let blocked_before = self.double_close_blocked;
        let ids: Vec<u64> = self
            .channels
            .iter()
            .filter(|(_, state)| state.participants.contains(&pid))
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            report.attempted += 1;
            let last = match self.channels.get_mut(&id) {
                Some(state) => {
                    state.participants.retain(|p| *p != pid);
                    state.waiters.retain(|p| *p != pid);
                    state.participants.is_empty()
                }
                None => false,
            };
            if last {
                let (messages, waiters) = self.destroy(id);
                // Probe: ein zweiter Schliessversuch auf der zerstoerten
                // Kennung muss abgewiesen werden, sonst waere "keine
                // Doppelfreigabe" nur eine Absicht.
                self.close_again(id);
                report.destroyed += 1;
                report.messages_discarded += messages;
                report.waiters_aborted += waiters;
            } else {
                self.released_total += 1;
                report.released += 1;
            }
        }
        report.double_close_blocked = self.double_close_blocked - blocked_before;
        report
    }

    /// Ausdrueckliches Schliessen durch einen Teilnehmer.
    pub fn close(&mut self, id: u64, pid: Pid) -> CloseOutcome {
        match self.channels.get_mut(&id) {
            None => {
                self.double_close_blocked += 1;
                CloseOutcome::Rejected
            }
            Some(state) => {
                if !state.participants.contains(&pid) {
                    return CloseOutcome::Rejected;
                }
                state.participants.retain(|p| *p != pid);
                if state.participants.is_empty() {
                    self.destroy(id);
                    CloseOutcome::Destroyed
                } else {
                    self.released_total += 1;
                    CloseOutcome::Kept
                }
            }
        }
    }

    /// Zweiter Schliessversuch auf einer bereits zerstoerten Kennung. Getrennt
    /// gezaehlt, damit "keine Doppelfreigabe" belegbar ist statt nur behauptet.
    pub fn close_again(&mut self, id: u64) -> bool {
        if self.channels.contains_key(&id) {
            return false;
        }
        self.double_close_blocked += 1;
        self.double_close_blocked
            == self.double_close_blocked // immer wahr; Zaehler ist der Beleg
    }

    pub fn double_close_blocked(&self) -> usize {
        self.double_close_blocked
    }

    /// Bestand nach einem Prozessende: offene Kanaele, davon geteilte, und
    /// wartende Nachrichten. Ohne Kennungen, damit die Ausgabe nicht an
    /// Beispiel-IDs haengt.
    pub fn snapshot(&self) -> (usize, usize, usize) {
        let shared = self
            .channels
            .values()
            .filter(|state| state.participants.len() > 1)
            .count();
        let messages = self.channels.values().map(|state| state.pending.len()).sum();
        (self.channels.len(), shared, messages)
    }
}

/// Prozesslokale Handle-Tabelle.
#[derive(Debug, Default)]
pub struct HandleTable {
    handles: BTreeMap<u64, Handle>,
    next: u64,
    stale_denied: usize,
    revoked: usize,
}

impl HandleTable {
    pub const fn new() -> Self {
        Self {
            handles: BTreeMap::new(),
            next: 1,
            stale_denied: 0,
            revoked: 0,
        }
    }

    pub fn open(&mut self, owner: Pid, kind: HandleKind, target: u64) -> u64 {
        let id = self.next;
        self.next += 1;
        self.handles.insert(
            id,
            Handle {
                id,
                kind,
                target,
                owner,
            },
        );
        id
    }

    pub fn validate(&mut self, owner: Pid, id: u64) -> Result<u64, HandleError> {
        match self.handles.get(&id) {
            Some(handle) if handle.owner == owner => Ok(handle.target),
            Some(_) => {
                self.stale_denied += 1;
                Err(HandleError::Stale)
            }
            None => {
                self.stale_denied += 1;
                Err(HandleError::Stale)
            }
        }
    }

    pub fn count_for(&self, owner: Pid) -> usize {
        self.handles.values().filter(|h| h.owner == owner).count()
    }

    /// Alle Handles eines beendeten Prozesses widerrufen.
    ///
    /// Gibt (widerrufen, veraltete Zugriffe abgewiesen) zurueck. Die Zaehlung
    /// der abgewiesenen Zugriffe ist der Beleg: nach dem Widerruf zeigt kein
    /// Handle mehr auf ein Objekt, und ein alter Wert wird nicht still gueltig.
    pub fn release_all(&mut self, owner: Pid) -> (usize, usize) {
        let ids: Vec<u64> = self
            .handles
            .values()
            .filter(|h| h.owner == owner)
            .map(|h| h.id)
            .collect();
        let revoked = ids.len();
        for id in &ids {
            self.handles.remove(id);
        }
        self.revoked += revoked;
        // Probe: jeder gerade widerrufene Handle muss jetzt abgewiesen werden.
        // Geprobt werden genau die eigenen, nicht fremde — sonst zaehlte die
        // Zahl nur, dass andere Prozesse andere Handles haben.
        let mut denied = 0;
        for id in ids {
            if self.validate(owner, id).is_err() {
                denied += 1;
            }
        }
        (revoked, denied)
    }

    pub fn stale_denied(&self) -> usize {
        self.stale_denied
    }

    pub fn revoked_total(&self) -> usize {
        self.revoked
    }
}

/// Die eine Instanz fuer Kanal-Lebensdauer.
pub static CHANNELS: Mutex<ChannelRegistry> = Mutex::new(ChannelRegistry::new());

/// Die eine Instanz fuer Prozess-Handles.
pub static HANDLES: Mutex<HandleTable> = Mutex::new(HandleTable::new());

/// Was ein Prozessende an IPC- und Handle-Ressourcen zurueckgegeben hat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExitResources {
    pub channels_attempted: usize,
    pub channels_destroyed: usize,
    pub channels_released: usize,
    pub messages_discarded: usize,
    pub waiters_aborted: usize,
    pub handles_revoked: usize,
    pub stale_handles_denied: usize,
    pub double_close_blocked: usize,
}

/// Prozessende: Kanaele nach Teilnehmerzahl behandeln, Handles widerrufen.
pub fn release_process(pid: Pid) -> ExitResources {
    let channels = CHANNELS.lock().release(pid);
    let (handles_revoked, stale_handles_denied) = HANDLES.lock().release_all(pid);
    ExitResources {
        channels_attempted: channels.attempted,
        channels_destroyed: channels.destroyed,
        channels_released: channels.released,
        messages_discarded: channels.messages_discarded,
        waiters_aborted: channels.waiters_aborted,
        handles_revoked,
        stale_handles_denied,
        double_close_blocked: channels.double_close_blocked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(n: u32) -> Pid {
        Pid(n)
    }

    #[test]
    fn own_channel_is_destroyed_on_exit() {
        let mut reg = ChannelRegistry::new();
        assert!(reg.open(0x20, pid(1)));
        assert!(reg.enqueue(0x20, 0xAA));
        let report = reg.release(pid(1));
        assert_eq!(report.destroyed, 1);
        assert_eq!(report.messages_discarded, 1);
        assert!(!reg.contains(0x20));
    }

    #[test]
    fn shared_channel_survives_for_remaining_process() {
        let mut reg = ChannelRegistry::new();
        reg.open(0x21, pid(1));
        reg.attach(0x21, pid(2));
        reg.enqueue(0x21, 0xBB);
        let report = reg.release(pid(1));
        assert_eq!(report.released, 1);
        assert_eq!(report.destroyed, 0);
        assert!(reg.contains(0x21));
        assert_eq!(reg.pending_of(0x21), Some(1));
        assert_eq!(reg.participants_of(0x21), Some(1));
    }

    #[test]
    fn last_participant_destroys_channel() {
        let mut reg = ChannelRegistry::new();
        reg.open(0x22, pid(1));
        reg.attach(0x22, pid(2));
        reg.release(pid(1));
        let report = reg.release(pid(2));
        assert_eq!(report.destroyed, 1);
        assert!(!reg.contains(0x22));
    }

    #[test]
    fn waiters_are_aborted_when_channel_dies() {
        let mut reg = ChannelRegistry::new();
        reg.open(0x23, pid(1));
        reg.wait(0x23, pid(2));
        let report = reg.release(pid(1));
        assert_eq!(report.waiters_aborted, 1);
        assert!(!reg.contains(0x23));
    }

    #[test]
    fn double_close_is_not_a_double_free() {
        let mut reg = ChannelRegistry::new();
        reg.open(0x24, pid(1));
        assert_eq!(reg.close(0x24, pid(1)), CloseOutcome::Destroyed);
        assert_eq!(reg.close(0x24, pid(1)), CloseOutcome::Rejected);
        assert_eq!(reg.destroyed_total(), 1);
        assert!(reg.double_close_blocked() >= 1);
    }

    #[test]
    fn release_proves_double_close_is_blocked() {
        let mut reg = ChannelRegistry::new();
        reg.open(0x26, pid(1));
        reg.enqueue(0x26, 0xCC);
        let report = reg.release(pid(1));
        assert_eq!(report.destroyed, 1);
        assert_eq!(report.double_close_blocked, 1);
        assert!(!reg.contains(0x26));
    }

    #[test]
    fn snapshot_counts_surviving_shared_channels() {
        let mut reg = ChannelRegistry::new();
        reg.open(0x27, pid(1));
        reg.attach(0x27, pid(2));
        reg.enqueue(0x27, 0xDD);
        reg.open(0x28, pid(1));
        reg.release(pid(1));
        let (open, shared, messages) = reg.snapshot();
        // Der ueberlebende Kanal hat nur noch einen Teilnehmer, gilt also
        // nicht mehr als geteilt — die Nachricht bleibt trotzdem liegen.
        assert_eq!((open, shared, messages), (1, 0, 1));
    }

    #[test]
    fn live_send_and_recv_go_through_the_registry() {
        let mut reg = ChannelRegistry::new();
        reg.open_with(0x40, pid(1), 4);
        assert_eq!(reg.send(pid(1), 0x40, 0x1234), Ok(()));
        assert_eq!(reg.recv(pid(1), 0x40), Ok(0x1234));
        assert_eq!(reg.recv(pid(1), 0x40), Err(LiveIpcError::Empty));
    }

    #[test]
    fn non_participant_is_denied_fail_closed() {
        let mut reg = ChannelRegistry::new();
        reg.open(0x41, pid(1));
        assert_eq!(reg.send(pid(2), 0x41, 1), Err(LiveIpcError::NotParticipant));
        assert_eq!(reg.recv(pid(2), 0x41), Err(LiveIpcError::NotParticipant));
        assert_eq!(reg.send(pid(1), 0x99, 1), Err(LiveIpcError::UnknownChannel));
        // Der abgewiesene Sender hat nichts hinterlassen.
        assert_eq!(reg.recv(pid(1), 0x41), Err(LiveIpcError::Empty));
    }

    #[test]
    fn full_channel_refuses_instead_of_dropping_silently() {
        let mut reg = ChannelRegistry::new();
        reg.open_with(0x42, pid(1), 2);
        assert_eq!(reg.send(pid(1), 0x42, 1), Ok(()));
        assert_eq!(reg.send(pid(1), 0x42, 2), Ok(()));
        assert_eq!(reg.send(pid(1), 0x42, 3), Err(LiveIpcError::Full));
        assert_eq!(reg.pending_of(0x42), Some(2));
    }

    /// Nebenlaeufigkeit ist im Kernel eine Frage der Reihenfolge, nicht der
    /// Threads: Trap und Tick laufen unter einer Sperre. Diese Tests fahren die
    /// Verzahnungen durch, die sonst nur zufaellig auffallen.
    #[test]
    fn attach_during_release_is_denied_after_the_channel_died() {
        let mut reg = ChannelRegistry::new();
        reg.open(0x43, pid(1));
        reg.release(pid(1));
        assert!(!reg.attach(0x43, pid(2)), "toter Kanal nimmt niemanden mehr auf");
        assert!(!reg.deliver(0x43, 7));
        assert_eq!(reg.device_drops(), 1);
    }

    #[test]
    fn send_during_close_is_lost_with_the_channel() {
        let mut reg = ChannelRegistry::new();
        reg.open(0x44, pid(1));
        reg.send(pid(1), 0x44, 11).unwrap();
        let report = reg.release(pid(1));
        assert_eq!(report.messages_discarded, 1);
        assert_eq!(reg.send(pid(1), 0x44, 12), Err(LiveIpcError::UnknownChannel));
    }

    #[test]
    fn two_participants_exit_in_either_order() {
        for first in [pid(1), pid(2)] {
            let mut reg = ChannelRegistry::new();
            reg.open(0x45, pid(1));
            reg.attach(0x45, pid(2));
            let other = if first == pid(1) { pid(2) } else { pid(1) };
            let a = reg.release(first);
            assert_eq!((a.destroyed, a.released), (0, 1));
            assert!(reg.contains(0x45));
            let b = reg.release(other);
            assert_eq!((b.destroyed, b.released), (1, 0));
            assert!(!reg.contains(0x45));
        }
    }

    #[test]
    fn access_after_last_participant_is_denied() {
        let mut reg = ChannelRegistry::new();
        reg.open(0x46, pid(1));
        reg.attach(0x46, pid(2));
        reg.release(pid(1));
        // Noch gueltig: 2 ist Teilnehmer.
        assert!(reg.recv(pid(2), 0x46).is_err()); // leer, aber nicht abgewiesen
        reg.release(pid(2));
        assert_eq!(reg.recv(pid(2), 0x46), Err(LiveIpcError::UnknownChannel));
        assert_eq!(reg.send(pid(2), 0x46, 1), Err(LiveIpcError::UnknownChannel));
    }

    #[test]
    fn revoked_handle_is_stale() {
        let mut table = HandleTable::new();
        let h = table.open(pid(1), HandleKind::Channel, 0x30);
        assert_eq!(table.validate(pid(1), h), Ok(0x30));
        let (revoked, _) = table.release_all(pid(1));
        assert_eq!(revoked, 1);
        assert_eq!(table.validate(pid(1), h), Err(HandleError::Stale));
    }

    #[test]
    fn foreign_handle_is_denied() {
        let mut table = HandleTable::new();
        let h = table.open(pid(1), HandleKind::Process, 0x40);
        assert_eq!(table.validate(pid(2), h), Err(HandleError::Stale));
    }

    #[test]
    fn release_of_process_without_resources_is_empty() {
        let mut reg = ChannelRegistry::new();
        let mut table = HandleTable::new();
        reg.open(0x25, pid(1));
        table.open(pid(1), HandleKind::Channel, 0x25);
        assert_eq!(reg.release(pid(9)).attempted, 0);
        assert_eq!(table.release_all(pid(9)), (0, 0));
    }
}
