// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// ShivaCore Crash-UD — ein Programm mit ungueltigem Opcode
//
// `ud2` ist die Anweisung, die es per Definition nicht gibt. Sie loest einen
// ungueltigen Opcode aus, und den legt die CPU **ohne** Fehlercode auf den
// Stapel — deshalb braucht dieser Vektor einen eigenen Trampolin.
// ─────────────────────────────────────────────────────────────────────────

#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[path = "../allocator.rs"]
mod allocator;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        core::arch::asm!("ud2", options(nomem, nostack, preserves_flags));
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
