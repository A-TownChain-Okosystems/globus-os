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

/// Startzustand der Daten je Dienst: virtuelle Basisadresse und die Bytes des
/// Datensegments (vorbelegte Daten plus bss-Nullen).
///
/// Getrennt von `Service`, weil `get` einen Eintrag klont; ein 6 KiB-Abbild
/// wuerde bei jedem Blick auf den Dienst im Trap-Kontext neu angelegt. Hier
/// liegt es einmal, und `restore_image` kopiert direkt an die Adresse.
static IMAGES: Mutex<Vec<Image>> = Mutex::new(Vec::new());

/// Hoechstgroesse eines hinterlegten Startabbilds.
///
/// Das Abbild liegt im Kernel-Haufen (512 KiB, siehe `allocator::HEAP_SIZE`).
/// 64 KiB je Dienst erlauben acht Dienste und decken das Anmeldefenster
/// (6160 Byte) weit. Ein Dienst mit groesserem Datensegment laeuft weiter,
/// wird aber beim Neustart nicht zurueckgesetzt — das wird gemeldet, nicht
/// verschwiegen.
pub const MAX_SERVICE_IMAGE_BYTES: usize = 64 * 1024;

/// Ein hinterlegtes Datensegment-Abbild.
struct Image {
    /// Prozess, zu dem das Abbild gehoert.
    pid: u64,
    /// Virtuelle Adresse, an die der Kernel das Datensegment kopiert hat.
    base: u64,
    /// Die Bytes genau so, wie sie beim Start abgebildet wurden.
    bytes: Vec<u8>,
}

/// Hinterlegt den Startzustand der Daten eines Dienstes.
///
/// Ein zweiter Aufruf fuer dieselbe PID ersetzt das Abbild: zwei Wahrheiten
/// ueber denselben Startzustand gibt es nicht.
/// Liefert `false`, wenn das Abbild die Obergrenze ueberschreitet; dann wird
/// nichts hinterlegt und der Dienst laeuft ohne Zuruecksetzen.
#[must_use]
pub fn store_image(pid: u64, base: u64, bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() > MAX_SERVICE_IMAGE_BYTES {
        return false;
    }
    let mut images = IMAGES.lock();
    if let Some(slot) = images.iter_mut().find(|image| image.pid == pid) {
        slot.base = base;
        slot.bytes.clear();
        slot.bytes.extend_from_slice(bytes);
    } else {
        images.push(Image {
            pid,
            base,
            bytes: Vec::from(bytes),
        });
    }
    true
}

/// Laenge des hinterlegten Abbilds einer PID, ohne zu kopieren.
pub fn image_len(pid: u64) -> Option<usize> {
    IMAGES
        .lock()
        .iter()
        .find(|image| image.pid == pid)
        .map(|image| image.bytes.len())
}

/// Schreibt das hinterlegte Datensegment an seine Adresse zurueck.
///
/// Laeuft im Trap des Dienstes, also mit dessen Seitentabelle: die Seiten sind
/// ueber die virtuelle Adresse erreichbar, es wird nichts neu abgebildet und
/// nichts angefordert. Vorher blieb der Inhalt des vorigen Laufs stehen — ein
/// Zaehler in `.data` ueberlebte den Neustart (belegt: "Start 3" nach `Esc`).
pub fn restore_image(pid: u64) -> Option<usize> {
    let images = IMAGES.lock();
    let image = images.iter().find(|image| image.pid == pid)?;
    let len = image.bytes.len();
    // Nur kopieren, wenn die Bytes in die abgebildete Seite passen wuerden;
    // die Laenge stammt aus demselben Abbild wie die Adresse, deshalb ist die
    // Pruefung eine Sicherung gegen kuenftige Aenderungen, keine Heuristik.
    if image.base == 0 || len == 0 {
        return None;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(image.bytes.as_ptr(), image.base as *mut u8, len);
    }
    Some(len)
}


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
    fn abbild_wird_hinterlegt_und_ersetzt() {
        assert!(store_image(7_777, 0x01D0_0000, &[1, 2, 3, 4]));
        assert_eq!(image_len(7_777), Some(4));
        // Zweiter Aufruf ersetzt, er haengt nicht an: keine zweite Wahrheit.
        assert!(store_image(7_777, 0x01D0_0000, &[9, 8]));
        assert_eq!(image_len(7_777), Some(2));
        assert!(image_len(7_778).is_none());
    }

    #[test]
    fn leeres_abbild_wird_nicht_hinterlegt() {
        assert!(!store_image(7_779, 0x01D0_0000, &[]));
        assert!(image_len(7_779).is_none());
        assert!(restore_image(7_779).is_none(), "ohne Abbild kein Schreiben");
    }

    #[test]
    fn zu_grosses_abbild_wird_abgelehnt() {
        let zu_gross = alloc::vec![0u8; MAX_SERVICE_IMAGE_BYTES + 1];
        assert!(!store_image(7_780, 0x01D0_0000, &zu_gross));
        assert!(image_len(7_780).is_none(), "nichts hinterlegt");
        let grenze = alloc::vec![0u8; MAX_SERVICE_IMAGE_BYTES];
        assert!(store_image(7_781, 0x01D0_0000, &grenze), "genau die Grenze ist zulaessig");
        assert_eq!(image_len(7_781), Some(MAX_SERVICE_IMAGE_BYTES));
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
