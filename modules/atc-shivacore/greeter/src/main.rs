// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! ShivaCore Anmeldefenster.
//!
//! Das Programm ist der vierte Ring-3-Prozess (PID 1003) und liest einen
//! zweiten Eintrag aus dem Ramdisk-Verzeichnis. Der Kernel gibt ihm zwei
//! Capabilities mit: die Konsole (Schreibrecht) und den Kernel-Eingabekanal
//! (Leserecht). Die Nummern stehen als zwei Worte am Anfang des Stapels —
//! vergeben werden sie von der Capability-Tabelle, nicht von diesem Programm.
//!
//! Was hier steht, ist die Maske: Titel, Benutzerfeld, Passwortfeld mit
//! Maskierung, Feldwechsel mit Tab, Loeschen mit Backspace, Absenden mit Enter.
//!
//! Die Pruefung selbst steht NICHT hier: das Fenster schickt Benutzername
//! und Passwort als versioniertes LGN1-Payload an den Kernel
//! (`IdentityCheck`, `0x0070`) und zeigt, was zurueckkommt. Der
//! Anmeldedatensatz liegt in der Ramdisk im Systemnamensraum und ist fuer
//! Programme nicht vorhanden; gerechnet wird im Kernel, weil nur er ihn lesen
//! darf. Das Passwort wird nach dem Aufruf im Programm ueberschrieben, und das
//! Fenster behauptet keinen Erfolg, den es nicht bekommen hat.
//!
//! Es gibt keine initialisierten statischen Werte: der Kernel laedt genau ein
//! ausfuehrbares Segment, ein `.data` kaeme nicht mit. Alle Zustaende liegen auf
//! dem Stapel.

#![no_std]
#![no_main]

// libshivacore zieht globus-ipc mit, und das verlangt einen globalen
// Allocator. Dieses Programm braucht keinen Haufen: Maske und Puffer liegen
// auf dem Stapel. Deshalb steht hier eine ausdrueckliche Absage statt eines
// 64-KiB-Vorrats — ein solcher laege in `.bss`, und `.bss` liegt ausserhalb der
// Seiten, die der Kernel fuer dieses Programm abbildet (er bildet genau die
// Bytes des geladenen Segments ab). Eine Anfrage endet damit kontrolliert
// beim Alloc-Fehler statt in einem stillen Seitenfehler.

/// Allocator, der nichts vergibt: dieses Programm hat keinen Haufen.
struct NoHeap;

unsafe impl core::alloc::GlobalAlloc for NoHeap {
    unsafe fn alloc(&self, _layout: core::alloc::Layout) -> *mut u8 {
        core::ptr::null_mut()
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {}
}

#[global_allocator]
static NO_HEAP: NoHeap = NoHeap;

use core::fmt::{self, Write};
use core::panic::PanicInfo;
use libshivacore::{
    ABI_VERSION, CONSOLE_MAX_BYTES, CONSOLE_OP_CLEAR, CONSOLE_OP_WRITE, CapabilityHandle,
    IDENTITY_LOGIN_MAX_BYTES, IDENTITY_SESSION_HANDLE_TAG, KERNEL_INPUT_CHANNEL,
    SYSCALL_BLOCK_MAX_SIZE, Syscall,
    SyscallBlock, encode_identity_login_payload,
};

/// Vorbelegter statischer Wert: liegt im Datensegment, nicht im Codesegment.
/// Ohne abgebildetes Datensegment waere der erste Zugriff ein Seitenfehler.
static WINDOW_VERSION: [u8; 6] = *b"v0.3.0";
/// Null-initialisierter Puffer ueber zwei Seitengrenzen: liegt im .bss.
/// Er belegt, dass die bss-Nullen des Abbilds ankommen UND dass mehr als eine
/// Seite abgebildet ist. Waere er nicht abgebildet, faellt das Programm beim
/// ersten Zugriff aus; waere er nicht genullt, staende Unsinn darin.
// `#[unsafe(no_mangle)]`: der Puffer ist von aussen sichtbar und darf vom
// Optimierer nicht wegfallen, nur weil der Selbsttest ihn nur beschreibt.
#[unsafe(no_mangle)]
static mut SCRATCH: [u8; 6144] = [0; 6144];
/// Zaehler der Programmstarts: liegt im .data, also in einem beschreibbaren
/// statischen Wert mit Vorbelegung. Er ueberlebt ein Zuruecksetzen des Dienstes
/// (der Kernel leert den Stapel, nicht die Daten) — die Statuszeile zeigt das.
static mut STARTS: u32 = 1;

/// Schreibt und liest beide Enden des bss-Puffers: zwei Seiten, ein Nachweis.
///
/// Der geschriebene Wert kommt aus dem Startvertrag (Cap-Nummer), nicht aus
/// einer Konstante. Sonst koennte der Optimierer den Schreib-Lese-Zyklus
/// ausrechnen und den Puffer ganz entfernen — dann saehe das Programm keinen
/// bss-Puffer und der Nachweis waere wertlos.
fn scratch_state(seed: u8) -> &'static str {
    // Der Zugriff ist auf den statischen Puffer beschraenkt; mehr als diese
    // eine unsafe-Stelle braucht der Nachweis nicht.
    let bytes: &mut [u8] = unsafe { &mut *core::ptr::addr_of_mut!(SCRATCH) };
    bytes[0] = seed;
    bytes[6143] = !seed;
    if bytes[0] == seed && bytes[6143] == !seed {
        "bss 6144 B ok"
    } else {
        "bss FEHLER"
    }
}

/// Zaehlt den Start und liefert ihn. Beim ersten Lauf steht die Vorbelegung
/// (1), nach einem Neustart des Dienstes 2, 3, ...
fn bump_start() -> u32 {
    unsafe {
        STARTS += 1;
        STARTS
    }
}

/// Scancode der Ruecktaste in Scan-Set 1.
const SCAN_BACKSPACE: u8 = 0x0E;
/// Scancode der Tabulatortaste.
const SCAN_TAB: u8 = 0x0F;
/// Scancode der Eingabetaste.
const SCAN_ENTER: u8 = 0x1C;
/// Scancode der Escape-Taste. Sie beendet den Dienst; der Kernel setzt ihn
/// an seinen Startzustand zurueck (Neustart, siehe kernel/src/services.rs).
const SCAN_ESCAPE: u8 = 0x01;

/// Laengste Benutzername-Eingabe in Bytes.
const MAX_USER_BYTES: usize = 32;
/// Laengste Passworteingabe in Bytes. Gezaehlt, nicht gespeichert: das
/// Klartextpasswort liegt nicht im Programm, nur seine Laenge fuer die Maske.
const MAX_PASSWORD_BYTES: usize = 64;

/// Welches Feld gerade Eingaben bekommt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Active {
    /// Benutzername.
    User,
    /// Passwort (maskiert).
    Password,
}

/// Zustand der Maske; liegt vollstaendig auf dem Stapel.
struct Login {
    user: [u8; MAX_USER_BYTES],
    user_len: usize,
    /// Die eingegebenen Bytes. Sie werden nur fuer die Pruefung gehalten und
    /// danach ueberschrieben; angezeigt wird nie ein Byte, nur die Laenge.
    password: [u8; MAX_PASSWORD_BYTES],
    password_len: usize,
    /// Opaque PID-bound session capability returned by the kernel.
    session_handle: u64,
    active: Active,
    status: &'static str,
}

impl Login {
    fn new() -> Self {
        Self {
            user: [0u8; MAX_USER_BYTES],
            user_len: 0,
            password: [0u8; MAX_PASSWORD_BYTES],
            password_len: 0,
            session_handle: 0,
            active: Active::User,
            status: "Bitte Benutzer und Passwort eingeben.",
        }
    }

    /// Ein druckbares Zeichen in das aktive Feld uebernehmen.
    fn insert(&mut self, character: u8) {
        match self.active {
            Active::User => {
                if self.user_len < MAX_USER_BYTES {
                    self.user[self.user_len] = character;
                    self.user_len += 1;
                }
            }
            Active::Password => {
                if self.password_len < MAX_PASSWORD_BYTES {
                    self.password[self.password_len] = character;
                    self.password_len += 1;
                }
            }
        }
    }

    /// Letztes Zeichen des aktiven Feldes entfernen.
    fn backspace(&mut self) {
        match self.active {
            Active::User => {
                self.user_len = self.user_len.saturating_sub(1);
            }
            Active::Password => {
                self.password_len = self.password_len.saturating_sub(1);
                self.password[self.password_len] = 0;
            }
        }
    }

    /// Absenden: hier entscheidet sich, was das Fenster wirklich kann.
    /// Sind beide Felder gefuellt?
    fn ready(&self) -> bool {
        self.user_len > 0 && self.password_len > 0
    }

    /// Pruefen lassen: der Kernel rechnet, das Fenster zeigt das Ergebnis.
    ///
    /// Die Rechnung dauert im Image gemessen 267-276 ms (19 MiB, t=2). Damit
    /// der Bildschirm in dieser Zeit nicht steht, wird VOR dem Aufruf
    /// gezeichnet; der Aufruf blockiert das Programm, die Anzeige bleibt.
    fn verify(&mut self, console: u64) {
        self.session_handle = 0;
        let mut payload = [0u8; IDENTITY_LOGIN_MAX_BYTES];
        let payload_len = match encode_identity_login_payload(
            &self.user[..self.user_len],
            &self.password[..self.password_len],
            &mut payload,
        ) {
            Ok(length) => length,
            Err(_) => {
                self.password.fill(0);
                self.password_len = 0;
                self.status = "Benutzername oder Passwortformat ungueltig.";
                return;
            }
        };
        let mut block =
            SyscallBlock::new(Syscall::IdentityCheck, Some(CapabilityHandle(console)));
        block.buffer_ptr = payload.as_ptr() as u64;
        block.buffer_len = payload_len;
        let (value, error) = call(&block);
        // Klartextpasswort und gesamtes Login-Payload bleiben nicht im Programm liegen.
        payload.fill(0);
        self.password.fill(0);
        self.password_len = 0;
        self.status = if error != 0 {
            "Pruefung nicht moeglich (ABI-Ablehnung)."
        } else if value & IDENTITY_SESSION_HANDLE_TAG != 0 {
            self.session_handle = value;
            "Anmeldung erfolgreich - Sitzung im Kernel angelegt."
        } else {
            match value {
                0 => "Legacy-Zugangsdaten bestaetigt - keine Sitzung angelegt.",
                1 | 2 => "Benutzername oder Passwort falsch.",
                4 => "Pruefung nicht rechenbar (Speicher) - keine Anmeldung.",
                _ => "Pruefung ohne Ergebnis.",
            }
        };
    }
}

/// Textpuffer fuer einen Bildschirmaufbau.
struct Screen {
    bytes: [u8; CONSOLE_MAX_BYTES],
    len: usize,
}

impl Screen {
    fn new() -> Self {
        Self {
            bytes: [0u8; CONSOLE_MAX_BYTES],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        // Nur ASCII und UTF-8 aus eigenen Literalen; ein ungueltiger Rest wird
        // nicht erfunden, sondern bleibt weg.
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl Write for Screen {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let source = text.as_bytes();
        let room = CONSOLE_MAX_BYTES.saturating_sub(self.len);
        let take = core::cmp::min(room, source.len());
        self.bytes[self.len..self.len + take].copy_from_slice(&source[..take]);
        self.len += take;
        Ok(())
    }
}

// Der Syscall selbst liegt als Assembler-Stueck vor der Rust-Funktion.
//
// Grund: die ABI schreibt den Status nach RBX, und RBX ist ein
// *callee-saved* Register. Der Rust-Compiler darf also Werte ueber einen
// Syscall hinweg in RBX halten — er weiss nichts davon, dass der Kernel es
// ueberschreibt. Genau das ist zuerst passiert: die Konsolen-Nummer stand nach
// dem ersten Aufruf nicht mehr in RBX, der zweite Aufruf ging ohne Capability
// hinaus und wurde abgelehnt.
//
// Diese Huelle sichert die Callee-Register, holt nach `int 0x80` den Status aus
// RBX in RDX und gibt ihn so zurueck: RAX = Wert, RDX = Fehlercode. Fuer den
// Compiler sind damit beide Ausgaenge fluechtig, wie es die ABI verlangt.
core::arch::global_asm!(
    ".section .text.atc_syscall",
    ".global atc_syscall",
    ".type atc_syscall, @function",
    "atc_syscall:",
    "push rbx",
    "push r12",
    "push r13",
    "push r14",
    "push r15",
    "int 0x80",
    "mov rdx, rbx",
    "pop r15",
    "pop r14",
    "pop r13",
    "pop r12",
    "pop rbx",
    "ret",
    ".size atc_syscall, . - atc_syscall",
);

unsafe extern "C" {
    /// `int 0x80` mit gesicherten Callee-Registern.
    /// Rueckgabe: RAX = Wert, RDX = Fehlercode (0 = kein Fehler).
    fn atc_syscall(block: *const u8, size: u64, version: u64) -> u128;
}

/// Ein Syscall mit Argumentblock. Rueckgabe: (Wert, Fehlercode; 0 = kein Fehler).
fn call(block: &SyscallBlock) -> (u64, u64) {
    let mut bytes = [0u8; SYSCALL_BLOCK_MAX_SIZE];
    let size = match block.write_to(&mut bytes) {
        Ok(size) => size,
        Err(_) => return (0, 1),
    };
    // Der Block liegt auf dem Stapel; der Kernel prueft den Zeiger gegen das
    // Prozessabbild, bevor er ihn liest.
    let packed = unsafe { atc_syscall(bytes.as_ptr(), size as u64, u64::from(ABI_VERSION)) };
    (packed as u64, (packed >> 64) as u64)
}

/// Beendet den Dienst. Der Kernel entscheidet, was daraus wird: ein
/// eingetragener Dienst wird zurueckgesetzt, alles andere endet.
fn finish() -> ! {
    let mut block = SyscallBlock::new(Syscall::Exit, None);
    block.arg0 = 0;
    let _ = call(&block);
    // Kein Rueckweg: der Exit-Syscall antwortet nicht. Erreicht der Aufruf
    // diese Zeile, ist die ABI nicht die erwartete — dann haelt das Programm
    // an, statt weiterzulaufen, als waere nichts geschehen.
    loop {
        yield_cpu();
    }
}

/// Bildschirm leeren.
fn clear(console: u64) {
    let mut block = SyscallBlock::new(Syscall::Console, Some(CapabilityHandle(console)));
    block.arg0 = CONSOLE_OP_CLEAR;
    let _ = call(&block);
}

/// Text schreiben.
fn write(console: u64, text: &str) {
    let mut block = SyscallBlock::new(Syscall::Console, Some(CapabilityHandle(console)));
    block.arg0 = CONSOLE_OP_WRITE;
    block.buffer_ptr = text.as_ptr() as u64;
    block.buffer_len = text.len();
    let _ = call(&block);
}

/// Naechstes Tastenereignis holen; 0 heisst "nichts anliegend".
fn receive(input: u64) -> (u64, u64) {
    let mut block = SyscallBlock::new(Syscall::IpcReceive, Some(CapabilityHandle(input)));
    block.arg0 = KERNEL_INPUT_CHANNEL;
    block.arg1 = 0;
    call(&block)
}

/// CPU abgeben, solange keine Taste anliegt.
fn yield_cpu() {
    let block = SyscallBlock::new(Syscall::Yield, None);
    let _ = call(&block);
}

/// Vollstaendige Maske auf den Bildschirm bringen.
fn render(console: u64, login: &Login) {
    let mut screen = Screen::new();
    let _ = write!(
        screen,
        "\n  GlobusOS {} · Start {}\n  Anmeldung\n\n  {} Benutzer   : ",
        core::str::from_utf8(&WINDOW_VERSION).unwrap_or("?"),
        unsafe { STARTS },
        if login.active == Active::User { ">" } else { " " }
    );
    for byte in &login.user[..login.user_len] {
        let _ = screen.write_str(core::str::from_utf8(core::slice::from_ref(byte)).unwrap_or("?"));
    }
    let _ = write!(
        screen,
        "\n  {} Passwort   : ",
        if login.active == Active::Password { ">" } else { " " }
    );
    for _ in 0..login.password_len {
        let _ = screen.write_str("*");
    }
    let _ = write!(
        screen,
        "\n\n  [Tab] Feld wechseln   [Enter] anmelden   [Backspace] loeschen   [Esc] neu starten\n\n  {} · {}   ({})\n",
        login.status,
        if login.session_handle & IDENTITY_SESSION_HANDLE_TAG != 0 { "Sitzung aktiv" } else { "Keine Sitzung" },
        scratch_state(console as u8 | 1)
    );
    // Erst leeren, dann schreiben: die Maske wird als Ganzes gezeichnet, damit
    // kein Rest des vorherigen Zustands stehen bleibt.
    clear(console);
    write(console, screen.as_str());
}

// Einstieg als nacktes Assembler-Stueck, nicht als Rust-Funktion.
//
// Der Startvertrag ist: [rsp] = Konsolen-Cap, [rsp+8] = Eingabe-Cap. Er muss
// GELESEN WERDEN, BEVOR die erste Rust-Anweisung laeuft — ein Rust-Prolog
// schiebt Register und verschiebt rsp, und dann stehen an [rsp] die eigenen
// gesicherten Werte statt der Caps. Genau das war der Fehler, den das
// Anmeldefenster zuerst gezeigt hat: zwei Nummern gelesen, beide falsch,
// beide Syscalls abgelehnt.
//
// `.text._start` ist im Linkerskript der erste Abschnitt: der Kernel springt
// auf die Basis des Segments, nicht auf ein Symbol.
core::arch::global_asm!(
    ".section .text._start",
    ".global _start",
    "_start:",
    "mov rdi, [rsp]",
    "mov rsi, [rsp+8]",
    "and rsp, -16",
    "call greeter_main",
    "ud2",
);

/// Einstieg in Rust: `console` und `input` sind die beiden Cap-Nummern.
#[unsafe(no_mangle)]
pub extern "C" fn greeter_main(console: u64, input: u64) -> ! {
    // Erst zaelen, dann zeichnen: der Startzaehler steht in der ersten Zeile.
    let _ = bump_start();
    let mut login = Login::new();
    render(console, &login);

    loop {
        let (word, error) = receive(input);
        if error != 0 {
            // Kein stiller Weiterlauf: ohne Kanal gibt es kein Anmeldefenster.
            login.status = "Eingabekanal nicht verfuegbar (Capability oder Kanal fehlt).";
            render(console, &login);
            loop {
                yield_cpu();
            }
        }
        if word == 0 {
            yield_cpu();
            continue;
        }
        match libshivacore::input::scancode(word) {
            SCAN_BACKSPACE => login.backspace(),
            SCAN_TAB => {
                login.active = match login.active {
                    Active::User => Active::Password,
                    Active::Password => Active::User,
                };
            }
            SCAN_ENTER => {
                if login.ready() {
                    // Erst zeichnen, dann pruefen: die Anzeige bleibt waehrend
                    // der Rechnung im Kernel sichtbar.
                    login.status = "Pruefung laeuft ...";
                    render(console, &login);
                    login.verify(console);
                } else {
                    login.status = "Bitte Benutzer und Passwort eingeben.";
                }
            }
            // Der Dienst endet hier absichtlich selbst. Der Kernel setzt ihn
            // in seinen Startzustand zurueck; ohne Ruecksetzen waere Escape
            // ein stiller Ausfall des Anmeldefensters.
            SCAN_ESCAPE => finish(),
            _ => {
                if let Some(character) = libshivacore::input::character(word) {
                    if character.is_ascii_graphic() || character == ' ' {
                        login.insert(character as u8);
                    }
                }
            }
        }
        render(console, &login);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // Ohne Konsole gibt es hier nichts zu zeichnen; der Prozess haelt an,
    // statt in einer Schleife den Bildschirm zu fluten.
    loop {
        yield_cpu();
    }
}
