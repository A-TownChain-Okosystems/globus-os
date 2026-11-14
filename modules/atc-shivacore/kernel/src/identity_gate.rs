// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// Das Prueftor im Kernel (Bindeglied zwischen Boot, Ramdisk und Anmeldung)
//
// Der Datensatz wird beim Boot aus der Ramdisk gelesen und hier abgelegt; das
// Anmeldefenster fragt ueber den Syscall `IdentityCheck` nach dem Ergebnis.
// Damit gibt es eine Stelle, an der ein Passwort geprueft wird — nicht eine im
// Fenster und eine im Kernel.
// ─────────────────────────────────────────────────────────────────────────

use globus_identity_core::CredentialRecord;
use shivacore::identity_check::{CheckOutcome, IdentityGate};
use spin::Mutex;

/// Der Datensatz des Systems. Ohne ihn gibt es keine Anmeldung.
pub static GATE: Mutex<IdentityGate> = Mutex::new(IdentityGate::empty());

/// Uebernimmt den beim Boot gelesenen Datensatz.
pub fn publish(record: CredentialRecord) {
    GATE.lock().publish(record);
}

/// Steht ein Datensatz bereit?
pub fn has_record() -> bool {
    GATE.lock().has_record()
}

/// Prueft ein Passwort gegen den Datensatz.
pub fn check(password: &[u8]) -> CheckOutcome {
    GATE.lock().check(password)
}
