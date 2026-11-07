// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// ShivaCore Crash — das Programm, das absichtlich danebengreift
//
// Es schreibt auf eine Adresse, die in seinem Adressraum nicht abgebildet ist.
// Der Kernel muss diesen Prozess beenden und die anderen weiterlaufen lassen.
// Vor FAULT-001 haette diese eine Anweisung die ganze Maschine angehalten.
// ─────────────────────────────────────────────────────────────────────────

#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[path = "../allocator.rs"]
mod allocator;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // 0x0DEAD000 liegt zwischen den belegten Fenstern (Code ab 0x0040_0000,
    // Eingabe ab 0x0080_0000, Heap 0x0700_0000, Init 0x0180_0000, zweites
    // Programm 0x01C0_0000, Stack 0x07F0_0000).
    unsafe {
        core::ptr::write_volatile(0x0DEAD000 as *mut u64, 0xBAD_0BAD);
    }
    // Wird nie erreicht: der Schreibzugriff ist der Fehler.
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
