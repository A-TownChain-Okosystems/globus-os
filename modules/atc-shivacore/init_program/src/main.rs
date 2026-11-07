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

// ─────────────────────────────────────────────────────────────────────────
// Speicher
//
// `libshivacore` zieht `globus-ipc` mit, und das braucht einen Allocator. Ein
// Programm, das spaeter Puffer fuer Syscalls stellt, braucht ihn ohnehin.
// Deshalb ein einfacher Zeiger-Allocator ueber einem statischen Bereich im
// eigenen `.bss`: kein Freigeben, keine Fragmentierung, keine Ueberraschung.
//
// Bewusst keine Seitenanforderung beim Kernel: solange das Programm nur
// arbeitet und nicht waechst, ist ein fester Vorrat die ehrlichere Loesung.
// ─────────────────────────────────────────────────────────────────────────

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;

const ARENA_BYTES: usize = 64 * 1024;

struct Arena {
    memory: UnsafeCell<[u8; ARENA_BYTES]>,
    used: core::sync::atomic::AtomicUsize,
}

// Der Allocator haelt den Speicher exklusiv; die Zugriffe laufen ueber
// `allocate`, das den Zeiger atomar hochsetzt.
unsafe impl Sync for Arena {}

unsafe impl GlobalAlloc for Arena {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(1);
        let mut offset = self.used.load(Ordering::Relaxed);
        loop {
            let aligned = (offset + align - 1) & !(align - 1);
            let end = match aligned.checked_add(layout.size()) {
                Some(end) if end <= ARENA_BYTES => end,
                _ => return core::ptr::null_mut(),
            };
            match self
                .used
                .compare_exchange_weak(offset, end, Ordering::AcqRel, Ordering::Relaxed)
            {
                Ok(_) => {
                    let base = self.memory.get() as *mut u8;
                    return unsafe { base.add(aligned) };
                }
                Err(current) => offset = current,
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Zurueckgeben gibt es nicht. Ein Init-Prozess, der Speicher
        // freigibt, ist ein spaeterer Schritt mit eigener Rechnung.
    }
}

#[global_allocator]
#[used] // ohne `used` wirft der Linker den Vorrat weg, weil das Programm nichts allokiert
static ALLOCATOR: Arena = Arena {
    memory: UnsafeCell::new([0u8; ARENA_BYTES]),
    used: core::sync::atomic::AtomicUsize::new(0),
};

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

/// Einstiegspunkt. Der Name ist der des Linkerskripts, kein Rust-Komfort.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Die Adresse wird vom Loader gesetzt; wir notieren sie nur im Register,
    // damit ein Abschneiden der ersten Anweisung im Log auffaellt.
    unsafe {
        core::arch::asm!("nop", options(nomem, nostack, preserves_flags));
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
