//! Stellt einen Anmeldedatensatz aus (Host-Werkzeug, nicht im Image).
//!
//! Nutzung: issue-record <passwort> <ausgabedatei>
//!
//! Der Datensatz ist eine eigene Datei auf der Ramdisk (Entscheidung des
//! Owners): keine einkompilierten Zugangsdaten, eine Passwortaenderung ist
//! damit kein neuer Kernel-Build. Das Salz kommt aus /dev/urandom, die
//! Parameter sind die festgelegten (Argon2id, 19 MiB, t = 2, p = 1) und stehen
//! im Datensatz selbst.

use globus_identity_core::{HASH_LEN, KdfParams, MAGIC, RECORD_LEN, SALT_LEN, hash_into};

fn main() {
    let mut args = std::env::args().skip(1);
    let password = args
        .next()
        .expect("Nutzung: issue-record <passwort> <datei>");
    let path = args
        .next()
        .expect("Nutzung: issue-record <passwort> <datei>");

    let mut salt = [0u8; SALT_LEN];
    {
        use std::io::Read;
        let mut source = std::fs::File::open("/dev/urandom").expect("/dev/urandom");
        source.read_exact(&mut salt).expect("Salz");
    }

    let params = KdfParams::OWNER_DEFAULT;
    let mut hash = [0u8; HASH_LEN];
    let started = std::time::Instant::now();
    hash_into(params, password.as_bytes(), &salt, &mut hash).expect("KDF");
    let elapsed = started.elapsed();

    let mut record = [0u8; RECORD_LEN];
    record[0..4].copy_from_slice(&MAGIC);
    record[4] = 1; // Formatfassung
    record[5] = 1; // Argon2id
    record[6..8].copy_from_slice(&(params.memory_kib as u16).to_le_bytes());
    record[8..12].copy_from_slice(&params.iterations.to_le_bytes());
    record[12] = params.parallelism as u8;
    record[16..32].copy_from_slice(&salt);
    record[32..64].copy_from_slice(&hash);

    std::fs::write(&path, record).expect("schreiben");
    println!(
        "Datensatz geschrieben: {} ({} Byte, Argon2id {} KiB t={} p={}, Host-Zeit {} ms)",
        path,
        RECORD_LEN,
        params.memory_kib,
        params.iterations,
        params.parallelism,
        elapsed.as_millis()
    );
}
