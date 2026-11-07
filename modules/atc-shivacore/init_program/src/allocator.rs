// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! Zeiger-Allocator fuer Programme ohne Betriebssystem.
//!
//! `libshivacore` zieht `globus-ipc` mit, und das verlangt einen globalen
//! Allocator. Bewusst kein Freigeben: ein Programm, das spaeter Puffer fuer
//! Syscalls stellt, braucht Speicher, aber noch keine Verwaltung.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

const ARENA_BYTES: usize = 64 * 1024;

pub struct Arena {
    memory: UnsafeCell<[u8; ARENA_BYTES]>,
    used: AtomicUsize,
}

// Der Allocator vergibt Bereiche ueber einen atomaren Zeiger; kein anderer Pfad
// greift auf `memory` zu, solange `used` ihn nicht freigibt.
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
                Ok(_) => return unsafe { (self.memory.get() as *mut u8).add(aligned) },
                Err(current) => offset = current,
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

/// Der eine Vorrat beider Programme.
#[global_allocator]
#[used]
pub static ALLOCATOR: Arena = Arena {
    memory: UnsafeCell::new([0u8; ARENA_BYTES]),
    used: AtomicUsize::new(0),
};
