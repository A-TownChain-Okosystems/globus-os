// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// ShivaCore Crash-GP — ein Programm, das eine privilegierte Anweisung versucht
//
// `hlt` ist Ring 0 vorbehalten. Aus Ring 3 loest es eine Schutzverletzung aus.
// Vorher hatte dieser Vektor keinen Handler: der Fehler landete in einer
// Endlosschleife und nahm die Maschine mit.
// ─────────────────────────────────────────────────────────────────────────

#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[path = "../allocator.rs"]
mod allocator;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        // hlt in Ring 3 -> #GP(0)
        core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
    }
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
