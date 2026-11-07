// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// ShivaCore Peek — liest eine Adresse, die ihm nicht gehoert
//
// Dieses Programm ist kein Angriff, sondern eine Messung. Es liest eine
// Adresse im Stackbereich von Init (0x7FFE_F100), in die Init beim Start eine
// Marke schreibt. Peek hat seinen eigenen Stack bei 0x07F0_0000. Diese Adresse liegt
// in der Codeseite von Init, nicht in jeder eigenen Region von Peek.
//
// Gemeldet wird ueber den Opcode des Syscalls, damit die Aussage im Boot-Log
// steht und nicht in einem Kommentar:
//   Yield (0x0001)         -> die Marke war lesbar, die Trennung fehlt
//   MonotonicTime (0x0040) -> die Marke war nicht da, getrennte Raeume
//
// Das ist die Vorher-Messung zur CR3-Arbeit. Solange alle Prozesse eine
// Seitentabelle teilen, muss hier 0x0001 stehen.
// ─────────────────────────────────────────────────────────────────────────

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::sync::atomic::{compiler_fence, Ordering};

#[path = "../allocator.rs"]
mod allocator;

/// Dieselbe Marke wie in `main.rs`; das Programm ist eigenstaendig und darf
/// nicht auf das andere Binary zugreifen.
const SPACE_MARK: u64 = 0x4B41_4931_5F4D_4152;
const FOREIGN_ADDR: u64 = 0x7FFE_F100;

#[inline(always)]
fn syscall(opcode: u16) {
    let mut block = [0u64; 7];
    block[0] = libshivacore::SYSCALL_BLOCK_MIN_SIZE as u64;
    block[1] = u64::from(opcode);
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
    // Volatil lesen, damit der Uebersetzer die Messung nicht wegoptimiert.
    let seen = unsafe { core::ptr::read_volatile(FOREIGN_ADDR as *const u64) };

    let opcode = if seen == SPACE_MARK {
        libshivacore::Syscall::Yield.id()
    } else {
        libshivacore::Syscall::MonotonicTime.id()
    };

    loop {
        syscall(opcode);
        compiler_fence(Ordering::SeqCst);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
