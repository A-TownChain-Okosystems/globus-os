// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// ShivaCore Probe — das zweite Programm aus dem Verzeichnis
//
// Zweck: beweisen, dass das Ramdisk-Verzeichnis mehr als ein Programm traegt und
// der Kernel zwei verschiedene Prozesse daraus macht. Deshalb setzt dieses
// Programm einen anderen Syscall ab als `init`: `MonotonicTime` statt `Yield`.
// Am Opcode im Syscall-Log ist damit unterscheidbar, welcher Prozess spricht.
// ─────────────────────────────────────────────────────────────────────────

#![no_std]
#![no_main]

use core::panic::PanicInfo;

// Jedes Binary des Crates braucht den Allocator selbst: ein `#[global_allocator]`
// gilt je Programm, nicht je Crate. Der Pfad zeigt auf die gemeinsame Datei,
// damit es nur eine Fassung gibt.
#[path = "../allocator.rs"]
mod allocator;

/// Baut einen Argumentblock und ruft den Gate. Derselbe Weg wie in `init`.
#[inline(always)]
fn syscall(number: u16) {
    let mut block = [0u64; 7];
    block[0] = libshivacore::SYSCALL_BLOCK_MIN_SIZE as u64;
    block[1] = u64::from(number);
    unsafe {
        core::arch::asm!(
            "int 0x80",
            in("rdi") block.as_ptr(),
            in("rsi") libshivacore::SYSCALL_BLOCK_MIN_SIZE,
            in("rdx") u64::from(libshivacore::ABI_VERSION),
            lateout("rax") _,
            clobber_abi("sysv64"),
            options(nostack)
        );
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    loop {
        syscall(libshivacore::Syscall::MonotonicTime.id());
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
