// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// ShivaCore Init — das erste Programm, das nicht im Kernel steckt
//
// Bisher liefen die Ring-3-Prozesse als eingebauter Maschinencode im Kernel:
// kein ELF, keine Datei, nichts, was ein Betriebssystem ausmachen wuerde. Dieses
// Crate ist der Anfang vom Gegenteil: ein eigenstaendiges Programm mit eigenem
// Einstiegspunkt, festem Ladebereich und echtem ELF.
//
// Was es tut, ist absichtlich wenig: `Yield` im Kreis. Ein Init-Prozess, der
// nichts tut ausser zu existieren, ist der ehrlichste erste Schritt — wenn er
// laeuft, laeuft der Weg vom Datentraeger bis in den Ring 3.
//
// Der Syscall geht ueber `int 0x80`, denselben Weg wie der eingebaute
// Testprozess im Kernel. Die Nummer steht in `libshivacore`, damit Kernel und
// Programm nicht getrennt voneinander raten.
// ─────────────────────────────────────────────────────────────────────────

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::sync::atomic::{compiler_fence, Ordering};

// Der Allocator liegt in `allocator.rs`, weil beide Programme des Crates einen
// brauchen: `libshivacore` zieht `globus-ipc` mit, das einen globalen Allocator
// verlangt. Ein zweites `#[global_allocator]` waere ein Linkfehler.
mod allocator;

/// Ein `Yield` als echter ABI-2.0-Aufruf.
///
/// Der Gate verlangt seit ABI 2.0 keinen nackten Opcode im Register, sondern
/// einen Argumentblock im Userspace-Speicher: Groesse, Opcode, Cap, Argumente.
/// Ohne Block antwortet der Kernel mit `AbiVersionMismatch` und zaehlt den
/// Aufruf nicht einmal — genau das war beim ersten Lauf zu sehen.
///
/// Der Block liegt auf dem Stack, weil `map_user_binary` nur Code- und
/// Datenseite abbildet; ein `.bss`-Vorrat waere nicht erreichbar.
#[inline(always)]
fn syscall_yield() {
    // 7 Woerter = 56 Bytes = SYSCALL_BLOCK_MIN_SIZE.
    let mut block = [0u64; 7];
    block[0] = libshivacore::SYSCALL_BLOCK_MIN_SIZE as u64;
    // Offset 8 traegt Opcode (u16) und Flags (u16) in einem Wort.
    block[1] = u64::from(libshivacore::Syscall::Yield.id());
    // Capability, Argumente, Puffer bleiben null: Yield braucht keins davon.

    unsafe {
        core::arch::asm!(
            "int 0x80",
            in("rdi") block.as_ptr(),
            in("rsi") libshivacore::SYSCALL_BLOCK_MIN_SIZE,
            in("rdx") u64::from(libshivacore::ABI_VERSION),
            // RBX ist bei LLVM intern belegt und darf hier nicht als Operand
            // stehen. Der Kernel schreibt den Status dorthin; gelesen wird er
            // erst, wenn das Programm Fehler auswertet.
            lateout("rax") _,
            clobber_abi("sysv64"),
            options(nostack)
        );
    }
}

/// Die Marke, die Init in seine eigene Codeseite schreibt.
///
/// Sie ist der Beweis fuer eine Messung, nicht fuer eine Funktion: das
/// Programm `peek` liest dieselbe Adresse aus seinem eigenen Adressraum und
/// meldet ueber den Syscall-Opcode, ob es sie gesehen hat. Solange alle
/// Prozesse eine Seitentabelle teilen, sieht es sie. Nach dem CR3-Wechsel je
/// Prozess darf es sie nicht mehr sehen.
pub const SPACE_MARK: u64 = 0x4B41_4931_5F4D_4152; // "RAI1_MAK"

/// Adresse der Marke: im eigenen Stackbereich, weit unterhalb des Zeigers.
///
/// Die Codeseite waere der naheliegende Ort, ist aber NUR LESBAR abgebildet —
/// der erste Versuch dort endete in einem Seitenfehler beim Schreiben, und
/// Init wurde voellig korrekt beendet. Ein Programm darf seinen Code nicht
/// ueberschreiben; der Stack ist beschreibbar und trotzdem pro Prozess.
pub const SPACE_MARK_ADDR: u64 = 0x7FFE_F100;

/// Einstiegspunkt. Der Name ist der des Linkerskripts, kein Rust-Komfort.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Die Adresse wird vom Loader gesetzt; wir notieren sie nur im Register,
    // damit ein Abschneiden der ersten Anweisung im Log auffaellt.
    unsafe {
        core::arch::asm!("nop", options(nomem, nostack, preserves_flags));
        // Einmal schreiben, dann nur noch Yield: die Marke darf nicht von
        // jedem Durchlauf abhaengen, sonst waere ihr Fehlen nicht deutbar.
        core::ptr::write_volatile(SPACE_MARK_ADDR as *mut u64, SPACE_MARK);
    }
    loop {
        syscall_yield();
        compiler_fence(Ordering::SeqCst);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // Ein Programm ohne Ausgabe: im Fehlerfall anhalten, nicht weiterrechnen.
    loop {
        core::hint::spin_loop();
    }
}
