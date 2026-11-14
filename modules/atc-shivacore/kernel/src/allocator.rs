// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ShivaCore — Heap-Allokator.
// K-Sprint 2: Mappt einen festen virtuellen Adressbereich als Heap und
// initialisiert einen Linked-List-Allokator darueber, damit `alloc`
// (Box, Vec, String, ...) im Kernel nutzbar wird.

#[cfg(feature = "x86-boot")]
use x86_64::{
    structures::paging::{
        mapper::MapToError, FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
    },
    VirtAddr,
};

/// Beginn des Heap-Bereichs im virtuellen Speicher.
pub const HEAP_START: usize = 0x_4444_4444_0000;
/// Groesse des Heaps in Byte.
/// Groesse des Kernel-Haufens.
///
/// 100 KiB reichten fuer den Boot. Seit der Supervisor den Startzustand der
/// Daten eines Dienstes haelt (Datensegment inklusive bss, beim Anmeldefenster
/// 6160 Byte), braucht der Haufen mehr Platz: acht Dienste zu je
/// `MAX_SERVICE_IMAGE_BYTES` (64 KiB) sind 512 KiB. Der Wert ist damit
/// begruendet und nicht geraten; ein zu kleiner Haufen faellt beim Boot als
/// Allokationsfehler auf, nicht still.
pub const HEAP_SIZE: usize = 512 * 1024;

/// Freier Speicher laut Allokator (Byte).
///
/// Wird gebraucht, um eine Allokation zu belegen statt ihr zu glauben: ein
/// Zeiger ungleich null sagt nicht, dass der Speicher auch abgebildet ist.
pub fn free_bytes() -> usize {
    ALLOCATOR.lock().free()
}

#[cfg_attr(all(not(test), feature = "x86-boot"), global_allocator)] // [AD-028] Nur im echten Boot-Binary aktiv — als Abhaengigkeit stellt das Ziel-Binary (std) den Allokator
pub static ALLOCATOR: linked_list_allocator::LockedHeap = linked_list_allocator::LockedHeap::empty();

/// Erweitert den Haufen um `bytes` aus dem Frame-Vorrat.
///
/// Grund (IDENT-001): Argon2id mit 19 MiB braucht 19 MiB **am Stueck**. Der
/// Haufen ist auf 512 KiB begruendet (acht Dienst-Startabbilder); fuer die KDF
/// wird er einmalig erweitert. Der neue Bereich schliesst direkt an und wird
/// dem Allokator uebergeben, nicht selbst verwaltet — sonst gaebe es zwei
/// Vorraete.
///
/// Die Erweiterung ist einbahnig: danach gehoert der Speicher dem Haufen. Das
/// ist ehrlicher als "temporaer", weil eine Rueckgabe an den Frame-Vorrat ein
/// zweites Buch ueber denselben Speicher waere.
#[cfg(feature = "x86-boot")]
pub fn extend_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    bytes: usize,
) -> Result<usize, MapToError<Size4KiB>> {
    let pages = bytes.div_ceil(4096) as u64;
    let start = VirtAddr::new((HEAP_START + HEAP_SIZE) as u64);
    let first = Page::containing_address(start);
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
    for index in 0..pages {
        let page = first + index as u64;
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        unsafe {
            mapper.map_to(page, frame, flags, frame_allocator)?.flush();
        }
    }
    let added = (pages * 4096) as usize;
    // Safety: der Bereich [HEAP_START+HEAP_SIZE, +added) ist oben abgebildet
    // und gehoert noch niemandem; der Allokator bekommt ihn genau einmal.
    unsafe { ALLOCATOR.lock().extend(added) };
    Ok(added)
}

/// Bildet den Heap-Bereich ab und macht ihn dem Allokator bekannt.
#[cfg(feature = "x86-boot")]
pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE as u64 - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe {
            mapper.map_to(page, frame, flags, frame_allocator)?.flush();
        }
    }

    unsafe {
        ALLOCATOR.lock().init(HEAP_START as *mut u8, HEAP_SIZE);
    }

    Ok(())
}
