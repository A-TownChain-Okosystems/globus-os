//! Diensteverwaltung: ein Boot-Dienst, der sich beendet, wird an seinen
//! Startzustand zurueckgesetzt, statt zu verschwinden.
//!
//! Abgrenzung, damit hier keine zweite Prozessverwaltung entsteht: Das ist
//! **kein Prozessstart**. Es wird kein Adressraum gebaut, kein Programm
//! nachgeladen und kein Frame vergeben. Der Dienst kehrt in das Abbild zurueck,
//! das beim Boot fuer ihn angelegt wurde — mit geleertem Stapel und demselben
//! Startvertrag. Ein echter Neustart aus einem neuen Abbild braucht einen
//! Prozessstart-Syscall und bleibt offen (siehe
//! `modules/atc-shivacore/docs/SPAWN-ANNAHME-2026-11-09.md`).
//!
//! Grenze: Die Zahl der Neustarts ist begrenzt. Ein Dienst, der schon beim
//! Start abstuerzt, liefe sonst in einer Endlosschleife; nach dem Limit bleibt
//! er Zombie, und der Kernel protokolliert den letzten Versuch.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Hoechstzahl der Neustarts, wenn der Aufrufer keine eigene nennt.
pub const DEFAULT_RESTART_LIMIT: u32 = 5;

/// Ein Dienst: genau die Angaben, die ein Neustart braucht.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Service {
    /// Prozesskennung, an die der Startvertrag gebunden ist.
    pub pid: u64,
    /// Programmname aus dem Ramdisk-Verzeichnis, nur fuer die Ausgabe.
    pub name: String,
    /// Einsprungadresse des Programms.
    pub entry_rip: u64,
    /// Stapelzeiger des Startvertrags.
    pub start_rsp: u64,
    /// Untere Grenze des Stapelbereichs. Alles darunter gehoert nicht dem
    /// Dienst und wird beim Leeren nicht angefasst.
    pub stack_low: u64,
    /// Die Cap-Nummern des Startvertrags in der Reihenfolge Konsole, Eingabe.
    pub caps: [u64; 2],
    /// Bisherige Neustarts.
    pub attempts: u32,
    /// Hoechstzahl der Neustarts.
    pub limit: u32,
}

static SERVICES: Mutex<Vec<Service>> = Mutex::new(Vec::new());

/// Traegt einen laufenden Dienst ein.
///
/// Ein zweiter Eintrag fuer dieselbe PID ersetzt den ersten: zwei Wahrheiten
/// ueber denselben Prozess gibt es nicht.
#[allow(clippy::too_many_arguments)]
pub fn register(
    pid: u64,
    name: &str,
    entry_rip: u64,
    start_rsp: u64,
    stack_low: u64,
    caps: [u64; 2],
    limit: u32,
) {
    let mut services = SERVICES.lock();
    let entry = Service {
        pid,
        name: String::from(name),
        entry_rip,
        start_rsp,
        stack_low,
        caps,
        attempts: 0,
        limit,
    };
    if let Some(slot) = services.iter_mut().find(|service| service.pid == pid) {
        *slot = entry;
    } else {
        services.push(entry);
    }
}

/// Liefert den Eintrag einer PID, falls sie ein Dienst ist.
pub fn get(pid: u64) -> Option<Service> {
    SERVICES
        .lock()
        .iter()
        .find(|service| service.pid == pid)
        .cloned()
}

/// Zaehlt einen Neustart und liefert die Angaben dafuer.
///
/// `None` heisst: kein Dienst oder das Limit ist erreicht. Der Aufrufer darf
/// daraus keine zweite Entscheidung ableiten — entweder der Dienst kehrt
/// zurueck oder der Prozess endet wie jeder andere.
pub fn restart(pid: u64) -> Option<Service> {
    let mut services = SERVICES.lock();
    let service = services
        .iter_mut()
        .find(|service| service.pid == pid)?;
    if service.attempts >= service.limit {
        return None;
    }
    service.attempts += 1;
    Some(service.clone())
}

/// Zahl der eingetragenen Dienste.
pub fn count() -> usize {
    SERVICES.lock().len()
}

/// Namen der eingetragenen Dienste, fuer die Boot-Ausgabe.
pub fn names() -> Vec<String> {
    SERVICES
        .lock()
        .iter()
        .map(|service| service.name.clone())
        .collect()
}

/// Leert die Tabelle. Nur fuer Tests und einen Kernel-Neustart gedacht.
pub fn clear() {
    SERVICES.lock().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard() -> spin::MutexGuard<'static, ()> {
        // Die Tabelle ist prozessweit; Tests, die sie anfassen, laufen
        // nacheinander und setzen sie zu Beginn zurueck.
        static LOCK: Mutex<()> = Mutex::new(());
        let guard = LOCK.lock();
        clear();
        guard
    }

    fn eintragen(pid: u64, limit: u32) {
        register(
            pid,
            "greeter",
            0x01C0_0000,
            0x07EF_FFF0,
            0x07EF_8000,
            [5, 6],
            limit,
        );
    }

    #[test]
    fn register_merkt_sich_den_startvertrag() {
        let _guard = guard();
        eintragen(1003, 2);
        let service = get(1003).expect("Dienst fehlt");
        assert_eq!(service.name, "greeter");
        assert_eq!(service.entry_rip, 0x01C0_0000);
        assert_eq!(service.start_rsp, 0x07EF_FFF0);
        assert_eq!(service.stack_low, 0x07EF_8000);
        assert_eq!(service.caps, [5, 6]);
        assert_eq!(service.attempts, 0);
        assert_eq!(service.limit, 2);
    }

    #[test]
    fn zweiter_eintrag_ersetzt_statt_zu_verdoppeln() {
        let _guard = guard();
        eintragen(1003, 2);
        register(1003, "greeter2", 0x01C0_1000, 0x07EF_FFE0, 0x07EF_8000, [7, 8], 1);
        assert_eq!(count(), 1, "zwei Wahrheiten ueber denselben Prozess");
        assert_eq!(get(1003).unwrap().name, "greeter2");
    }

    #[test]
    fn restart_zaehlt_bis_zum_limit_und_dann_nicht_mehr() {
        let _guard = guard();
        eintragen(1003, 2);
        assert_eq!(restart(1003).unwrap().attempts, 1);
        assert_eq!(restart(1003).unwrap().attempts, 2);
        assert!(restart(1003).is_none(), "Limit ueberschritten");
        // Der Eintrag bleibt stehen: der Dienst ist bekannt, nur erschoepft.
        assert_eq!(get(1003).unwrap().attempts, 2);
    }

    #[test]
    fn unbekannte_pid_ist_kein_dienst() {
        let _guard = guard();
        eintragen(1003, 1);
        assert!(restart(1002).is_none());
        assert!(get(1002).is_none());
        assert_eq!(names(), alloc::vec![String::from("greeter")]);
    }

    #[test]
    fn exit_pfad_fragt_zuerst_die_dienste_dann_widerruft_er() {
        // Strukturwaechter: Ein Neustart nach dem Widerruf der Capabilities
        // ergaebe einen Dienst ohne Rechte, und ein Neustart nach
        // `exit_process` kaeme zu spaet. Die Reihenfolge ist die Zusicherung.
        let src = include_str!("interrupts.rs");
        let funktion = src.find("fn exit_current_process").expect("Exit-Funktion fehlt");
        let dienste = src.find("crate::services::restart(u64::from(pid.0))").expect("Dienstpruefung fehlt");
        let widerruf = funktion + src[funktion..].find("cleanup_process(pid)").expect("Widerruf fehlt");
        assert!(funktion < dienste, "Dienstpruefung fehlt im Exit-Pfad");
        assert!(dienste < widerruf, "Dienstpruefung steht nach dem Widerruf");
    }
}
