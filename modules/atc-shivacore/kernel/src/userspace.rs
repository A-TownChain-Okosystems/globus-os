// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ShivaCore — K-Sprint 30: Userspace / Ring-3 Implementation
// Copyright (c) Michael Wroblewski. All rights reserved.
//
// User-Level Prozesse (Ring 3): Privilege-Level-Wechsel, User-Address-Spaces,
// Binary-Loader, User-Context-Verwaltung, Syscall-Entry aus Ring 3.

use crate::ats1000::{ExitCode, Pid};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::arch::asm;
#[cfg(feature = "x86-boot")]
use x86_64::{
    VirtAddr,
    structures::paging::OffsetPageTable,
    structures::paging::{FrameAllocator, Mapper, Page, PageSize, PageTableFlags, Size4KiB},
};

// ═══════════════════════════════════════════════════════════════════════════════
// Privilege Levels
// ═══════════════════════════════════════════════════════════════════════════════

/// CPU Privilege Levels (Protection Rings)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PrivilegeLevel {
    /// Ring 0 — Kernel mode (full hardware access)
    Kernel = 0,
    /// Ring 3 — User mode (restricted access, syscalls via interrupt gate)
    User = 3,
}

impl PrivilegeLevel {
    /// True fuer Ring 0.
    pub fn is_kernel(&self) -> bool {
        matches!(self, PrivilegeLevel::Kernel)
    }
    /// True fuer Ring 3.
    pub fn is_user(&self) -> bool {
        matches!(self, PrivilegeLevel::User)
    }
    /// Ringnummer als Zahl.
    pub fn dpl(&self) -> u8 {
        *self as u8
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// User Address Space
// ═══════════════════════════════════════════════════════════════════════════════

/// Layout eines User-Address-Space (flat model, 4 GiB window)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserAddressSpace {
    /// Anfang des Codesegments.
    pub code_base: u64,
    /// Groesse des Codesegments.
    pub code_size: u64,
    /// Anfang des Datensegments.
    pub data_base: u64,
    /// Groesse des Datensegments.
    pub data_size: u64,
    /// Oberes Ende des Stapels; er waechst nach unten.
    pub stack_base: u64, // Top of stack (grows downward)
    /// Groesse des Stapels.
    pub stack_size: u64,
    /// Anfang des Haufens.
    pub heap_base: u64,
    /// Groesse des Haufens.
    pub heap_size: u64,
    /// Autoritatives ELF-Speicherlayout; None bedeutet Legacy-Fenster.
    pub memory_layout: Option<UserMemoryLayout>,
}

impl Default for UserAddressSpace {
    fn default() -> Self {
        Self {
            code_base: 0x00400000,  // 4 MiB
            code_size: 0x00100000,  // 1 MiB code region
            data_base: 0x00500000,  // 5 MiB
            data_size: 0x00100000,  // 1 MiB data region
            stack_base: 0x7FFFF000, // ~2 GiB (top, grows down)
            stack_size: 0x00010000, // 64 KiB stack
            heap_base: 0x00600000,  // 6 MiB
            heap_size: 0x00200000,  // 2 MiB heap
            memory_layout: None,
        }
    }
}

impl UserAddressSpace {
    /// Check if an address falls within any user region
    pub fn contains(&self, addr: u64) -> bool {
        if let Some(layout) = &self.memory_layout {
            return layout
                .segments
                .iter()
                .any(|s| addr >= s.virtual_start && addr < s.virtual_end)
                || self.in_stack(addr)
                || self.in_heap(addr);
        }
        self.in_code(addr) || self.in_data(addr) || self.in_stack(addr) || self.in_heap(addr)
    }
    /// True innerhalb eines ausfuehrbaren ELF-Segments oder des Legacy-Codefensters.
    pub fn in_code(&self, addr: u64) -> bool {
        if let Some(layout) = &self.memory_layout {
            return layout
                .segments
                .iter()
                .any(|s| s.flags & 0x1 != 0 && addr >= s.virtual_start && addr < s.virtual_end);
        }
        addr >= self.code_base && addr < self.code_base.saturating_add(self.code_size)
    }
    /// True innerhalb eines nicht ausfuehrbaren ELF-Segments oder des Legacy-Datenfensters.
    pub fn in_data(&self, addr: u64) -> bool {
        if let Some(layout) = &self.memory_layout {
            return layout
                .segments
                .iter()
                .any(|s| s.flags & 0x1 == 0 && addr >= s.virtual_start && addr < s.virtual_end);
        }
        addr >= self.data_base && addr < self.data_base.saturating_add(self.data_size)
    }
    /// True innerhalb des Stapels; obere Grenze einbegriffen, untere nicht.
    pub fn in_stack(&self, addr: u64) -> bool {
        addr > self.stack_base.saturating_sub(self.stack_size) && addr <= self.stack_base
    }
    /// True innerhalb des reservierten Haufens.
    pub fn in_heap(&self, addr: u64) -> bool {
        addr >= self.heap_base && addr < self.heap_base.saturating_add(self.heap_size)
    }
    /// Die tatsaechlich abgebildeten Regionen als [start, ende)-Paare.
    ///
    /// Der Adressraum-Bau braucht sie, um zu entscheiden, welche 1-GiB-Fenster
    /// er NICHT erben darf. Wichtig ist der Stack: er liegt standardmaessig
    /// knapp unter 2 GiB und damit nicht dort, wo man Nutzerseiten vermutet.
    pub fn mapped_regions(&self) -> Vec<(u64, u64)> {
        if let Some(layout) = &self.memory_layout {
            let mut regions = Vec::with_capacity(layout.segments.len() + 2);
            regions.extend(layout.segments.iter().map(|s| (s.page_start, s.page_end)));
            regions.push((
                layout.heap_base,
                layout.heap_base.saturating_add(layout.heap_size),
            ));
            regions.push((
                layout.stack_base.saturating_sub(layout.stack_size),
                layout.stack_base,
            ));
            return regions;
        }
        let stack_pages = self.stack_size.div_ceil(0x1000);
        let stack_start = self.stack_base.saturating_sub(stack_pages * 0x1000);
        alloc::vec![
            (
                self.code_base,
                self.code_base.saturating_add(self.code_size)
            ),
            (
                self.data_base,
                self.data_base.saturating_add(self.data_size)
            ),
            (stack_start, self.stack_base),
            (
                self.heap_base,
                self.heap_base.saturating_add(self.heap_size)
            ),
        ]
    }

    /// Stack pointer initial value (top of stack)
    pub fn initial_rsp(&self) -> u64 {
        self.stack_base
    }

    /// Prueft, ob ein ganzer Bereich in EINEM beschreibbaren User-Bereich liegt.
    ///
    /// Ein Zeiger aus dem Userspace ist ein Vorschlag, keine Zusage: der Kernel
    /// darf erst lesen oder schreiben, wenn der Bereich vollstaendig im Abbild
    /// des aufrufenden Prozesses liegt. Absichtlich nicht `in_code`: Code ist
    /// zwar lesbar, aber ein Puffer dort ist ein Fehler des Aufrufers.
    /// Ueberlauf wird abgelehnt, nicht umgerechnet.
    pub fn range_writable(&self, ptr: u64, len: usize) -> bool {
        if len == 0 {
            return true;
        }
        let Some(end) = ptr.checked_add(len as u64) else {
            return false;
        };
        let in_region = |base: u64, size: u64| ptr >= base && end <= base.saturating_add(size);
        if let Some(layout) = &self.memory_layout {
            return layout
                .segments
                .iter()
                .any(|s| s.flags & 0x2 != 0 && ptr >= s.virtual_start && end <= s.virtual_end)
                || in_region(layout.heap_base, layout.heap_size)
                || in_region(
                    layout.stack_base.saturating_sub(layout.stack_size),
                    layout.stack_size,
                );
        }
        in_region(self.data_base, self.data_size)
            || in_region(self.heap_base, self.heap_size)
            || (ptr >= self.stack_base.saturating_sub(self.stack_size) && end <= self.stack_base)
    }

    /// Liegt `[ptr, ptr+len)` vollstaendig in einem Bereich, den der Prozess
    /// lesen darf?
    ///
    /// Anders als `range_writable` schliesst das Codesegment ein: ein Programm
    /// darf seinen eigenen Code lesen (literale Zeichenketten liegen dort),
    /// aber nicht schreiben. Fuer jede Kopie aus dem Userspace in den Kernel
    /// ist das die richtige Frage.
    pub fn range_readable(&self, ptr: u64, len: usize) -> bool {
        if len == 0 {
            return false;
        }
        let Some(end) = ptr.checked_add(len as u64) else {
            return false;
        };
        let in_region = |base: u64, size: u64| ptr >= base && end <= base.saturating_add(size);
        if let Some(layout) = &self.memory_layout {
            return layout
                .segments
                .iter()
                .any(|s| s.flags & 0x4 != 0 && ptr >= s.virtual_start && end <= s.virtual_end)
                || in_region(layout.heap_base, layout.heap_size)
                || in_region(
                    layout.stack_base.saturating_sub(layout.stack_size),
                    layout.stack_size,
                );
        }
        in_region(self.code_base, self.code_size)
            || in_region(self.data_base, self.data_size)
            || in_region(self.heap_base, self.heap_size)
            || (ptr >= self.stack_base.saturating_sub(self.stack_size) && end <= self.stack_base)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// User Binary (simplified ELF-like format for ShivaCore)
// ═══════════════════════════════════════════════════════════════════════════════

/// Ein validiertes PT_LOAD-Abbild. `bytes` enthaelt exakt p_filesz Bytes;
/// der Mapper muss den Bereich bis p_memsz anschliessend mit Null auffuellen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserLoadSegment {
    /// Urspruenglicher ELF-Dateiversatz (p_offset).
    pub file_offset: u64,
    /// Urspruengliche virtuelle Ladeadresse (p_vaddr).
    pub virtual_address: u64,
    /// Dateigroesse (p_filesz).
    pub file_size: u64,
    /// Speicherbedarf einschliesslich BSS (p_memsz).
    pub memory_size: u64,
    /// ELF PF_R/PF_W/PF_X-Bits; keine bereits abgeleiteten PTE-Flags.
    pub flags: u32,
    /// ELF Alignment (p_align).
    pub alignment: u64,
    /// Unveraenderte Dateibytes des Segments.
    pub bytes: Vec<u8>,
}

/// Loadable user image. `code` and `data` remain compatibility projections
/// for existing manager/tests; `segments` is the authoritative ELF contract.
#[derive(Clone, Debug)]
pub struct UserBinary {
    /// Einsprungadresse.
    pub entry_point: u64,
    /// Kompatibilitaetsprojektion des ersten ausfuehrbaren Segments.
    pub code: Vec<u8>,
    /// Kompatibilitaetsprojektion des ersten beschreibbaren Datensegments inkl. BSS.
    pub data: Vec<u8>,
    /// Alle PT_LOAD-Segmente mit ihren urspruenglichen Adressen und Rechten.
    pub segments: Vec<UserLoadSegment>,
    /// Anzeigename.
    pub name: String,
}

impl UserBinary {
    /// Create a minimal "hello world" binary (just a HLT instruction)
    pub fn hello_world() -> Self {
        Self::from_bytes("hello", vec![0xF4], 0x00400000)
    }
    /// Create a binary from raw bytes. This compatibility constructor models
    /// one RX segment at the supplied virtual address.
    pub fn from_bytes(name: &str, code: Vec<u8>, entry: u64) -> Self {
        let segment = UserLoadSegment {
            file_offset: 0,
            virtual_address: entry,
            file_size: code.len() as u64,
            memory_size: code.len() as u64,
            flags: 0x5, // PF_R | PF_X
            alignment: 1,
            bytes: code.clone(),
        };
        Self {
            entry_point: entry,
            code,
            data: vec![],
            segments: vec![segment],
            name: name.to_string(),
        }
    }
    /// Laenge des Codes in Byte.
    pub fn code_len(&self) -> usize {
        self.code.len()
    }
    /// Laenge der Daten in Byte.
    pub fn data_len(&self) -> usize {
        self.data.len()
    }
}

/// Page-granular placement of validated ELF PT_LOAD segments and private regions.
///
/// Segment virtual addresses remain the ELF's original addresses. Heap and stack
/// are selected around those ranges; the process slot is deliberately absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserSegmentMapping {
    /// Original ELF segment start address (p_vaddr).
    pub virtual_start: u64,
    /// Exclusive end of the original in-memory segment range.
    pub virtual_end: u64,
    /// Page-aligned start used by the future page mapper.
    pub page_start: u64,
    /// Page-aligned exclusive end used by the future page mapper.
    pub page_end: u64,
    /// Original ELF PF_R/PF_W/PF_X permission bits.
    pub flags: u32,
}

/// Complete process memory placement calculated from one validated ELF image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserMemoryLayout {
    /// Original-address mappings for every loadable ELF segment.
    pub segments: Vec<UserSegmentMapping>,
    /// First virtual address of the private heap region.
    pub heap_base: u64,
    /// Reserved heap capacity in bytes.
    pub heap_size: u64,
    /// Exclusive upper boundary of the downward-growing stack.
    pub stack_base: u64,
    /// Reserved stack capacity in bytes.
    pub stack_size: u64,
}

const USER_PAGE_SIZE: u64 = 0x1000;
const USER_MIN_ADDRESS: u64 = 0x10000;
// AddressSpace rebuilds PML4 entry 0 for user mappings: 0..512 GiB.
const USER_LAYOUT_CEILING: u64 = 0x0000_0080_0000_0000;
const USER_DEFAULT_HEAP_SIZE: u64 = 0x20_0000;
const USER_DEFAULT_STACK_SIZE: u64 = 0x1_0000;

/// Plans an ELF image without rebasing its PT_LOAD virtual addresses.
///
/// The planner is intentionally pure: no frames or page tables are allocated.
/// Page-rounded overlaps are rejected so a later mapper cannot accidentally
/// map one virtual page twice with ambiguous ownership or permissions.
pub fn plan_user_memory(binary: &UserBinary) -> Result<UserMemoryLayout, UserspaceError> {
    if binary.segments.is_empty() {
        return Err(UserspaceError::InvalidBinary);
    }
    let mut segments = Vec::with_capacity(binary.segments.len());
    for segment in &binary.segments {
        if segment.memory_size == 0
            || segment.file_size > segment.memory_size
            || segment.bytes.len() as u64 != segment.file_size
            || segment.flags & !0x7 != 0
            || segment.flags & 0x2 != 0 && segment.flags & 0x1 != 0
        {
            return Err(UserspaceError::InvalidBinary);
        }
        let end = segment
            .virtual_address
            .checked_add(segment.memory_size)
            .ok_or(UserspaceError::InvalidAddress)?;
        if segment.virtual_address < USER_MIN_ADDRESS || end > USER_LAYOUT_CEILING {
            return Err(UserspaceError::InvalidAddress);
        }
        let page_start = segment.virtual_address & !(USER_PAGE_SIZE - 1);
        let page_end = end
            .checked_add(USER_PAGE_SIZE - 1)
            .ok_or(UserspaceError::InvalidAddress)?
            & !(USER_PAGE_SIZE - 1);
        segments.push(UserSegmentMapping {
            virtual_start: segment.virtual_address,
            virtual_end: end,
            page_start,
            page_end,
            flags: segment.flags,
        });
    }
    segments.sort_by_key(|segment| segment.page_start);
    for pair in segments.windows(2) {
        if pair[0].page_end > pair[1].page_start {
            return Err(UserspaceError::InvalidAddress);
        }
    }
    if !segments.iter().any(|segment| {
        segment.flags & 0x1 != 0
            && binary.entry_point >= segment.virtual_start
            && binary.entry_point < segment.virtual_end
    }) {
        return Err(UserspaceError::InvalidBinary);
    }

    let heap_size = USER_DEFAULT_HEAP_SIZE;
    let stack_size = USER_DEFAULT_STACK_SIZE;
    let stack_pages = stack_size.div_ceil(USER_PAGE_SIZE) * USER_PAGE_SIZE;
    let mut stack_base = USER_LAYOUT_CEILING - USER_PAGE_SIZE;
    loop {
        let stack_start = stack_base
            .checked_sub(stack_pages)
            .ok_or(UserspaceError::InvalidAddress)?;
        let collides = segments
            .iter()
            .any(|segment| stack_start < segment.page_end && segment.page_start < stack_base);
        if !collides {
            break;
        }
        stack_base = stack_start
            .checked_sub(USER_PAGE_SIZE)
            .ok_or(UserspaceError::InvalidAddress)?;
    }

    let heap_size = heap_size.div_ceil(USER_PAGE_SIZE) * USER_PAGE_SIZE;
    let mut heap_base = USER_MIN_ADDRESS;
    loop {
        let heap_end = heap_base
            .checked_add(heap_size)
            .ok_or(UserspaceError::InvalidAddress)?;
        if heap_end >= stack_base - stack_pages {
            return Err(UserspaceError::InvalidAddress);
        }
        let collision = segments
            .iter()
            .find(|segment| heap_base < segment.page_end && segment.page_start < heap_end);
        match collision {
            Some(segment) => heap_base = segment.page_end,
            None => break,
        }
    }

    Ok(UserMemoryLayout {
        segments,
        heap_base,
        heap_size,
        stack_base,
        stack_size: stack_pages,
    })
}

// ═══════════════════════════════════════════════════════════════════════════════
// Real user-page installation / CPL3 entry
// ═══════════════════════════════════════════════════════════════════════════════

impl UserAddressSpace {
    /// Builds the compatibility view and syscall-validation view from one ELF layout.
    pub fn from_memory_layout(layout: UserMemoryLayout) -> Self {
        let code = layout.segments.iter().find(|s| s.flags & 0x1 != 0);
        let data = layout
            .segments
            .iter()
            .find(|s| s.flags & 0x2 != 0 && s.flags & 0x1 == 0);
        Self {
            code_base: code.map_or(0, |s| s.virtual_start),
            code_size: code.map_or(0, |s| s.virtual_end.saturating_sub(s.virtual_start)),
            data_base: data.map_or(0, |s| s.virtual_start),
            data_size: data.map_or(0, |s| s.virtual_end.saturating_sub(s.virtual_start)),
            stack_base: layout.stack_base,
            stack_size: layout.stack_size,
            heap_base: layout.heap_base,
            heap_size: layout.heap_size,
            memory_layout: Some(layout),
        }
    }

    /// Passt ein Datenanteil dieser Groesse in das Datenfenster?
    ///
    /// Reine Rechnung, damit die Grenze ohne Paging pruefbar ist: `data_size`
    /// ist die Fenstergroesse dieses Prozesses. Die Pruefung steht hier und
    /// nicht im ELF-Parser, weil sie von der Prozesslayout abhaengt — derselbe
    /// Datensatz kann fuer den einen Prozess passen und fuer den anderen nicht.
    pub fn data_fits(&self, bytes: u64) -> bool {
        bytes <= self.data_size
    }
}

#[cfg(feature = "x86-boot")]
/// Nutzer-Code: waehrend des Ladens schreibbar, danach per update_flags auf
/// PRESENT|USER_ACCESSIBLE reduziert (lesbar, ausfuehrbar, nicht schreibbar).
pub fn code_flags() -> PageTableFlags {
    PageTableFlags::USER_ACCESSIBLE | PageTableFlags::WRITABLE
}

#[cfg(feature = "x86-boot")]
/// Nutzer-Daten: lesbar und schreibbar, ausdruecklich **nicht** ausfuehrbar.
/// Ohne NO_EXECUTE waere jedes Datensegment ein ausfuehrbarer Bereich und W^X
/// auf Hardware nicht erzwungen.
pub fn data_flags() -> PageTableFlags {
    PageTableFlags::USER_ACCESSIBLE | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE
}

#[cfg(feature = "x86-boot")]
/// Nutzer-Stack: wie Daten, zusaetzlich nie ausfuehrbar. Ein ausfuehrbarer
/// Stack ist das klassische Ziel fuer eingeschleusten Code.
pub fn stack_flags() -> PageTableFlags {
    PageTableFlags::USER_ACCESSIBLE | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE
}

#[cfg(feature = "x86-boot")]
/// W^X-Praedikat: schreibbar und ausfuehrbar zugleich ist verboten.
pub fn is_write_execute(flags: PageTableFlags) -> bool {
    flags.contains(PageTableFlags::WRITABLE) && !flags.contains(PageTableFlags::NO_EXECUTE)
}

/// Installs the authoritative ELF layout at its original virtual addresses.
///
/// Every page is zeroed before file bytes are copied, so the p_memsz-p_filesz
/// tail (BSS) is guaranteed zero. Segment permissions are applied only after
/// population; the final PTEs enforce PF_W/PF_X and never permit W+X.
#[cfg(feature = "x86-boot")]
unsafe fn map_user_memory_layout(
    mapper: &mut OffsetPageTable<'static>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    physical_memory_offset: VirtAddr,
    binary: &UserBinary,
    layout: &UserMemoryLayout,
) -> Result<(), UserspaceError> {
    fn map_zeroed_region(
        mapper: &mut OffsetPageTable<'static>,
        frame_allocator: &mut impl FrameAllocator<Size4KiB>,
        physical_memory_offset: VirtAddr,
        base: u64,
        size: u64,
    ) -> Result<(), UserspaceError> {
        if size == 0 {
            return Ok(());
        }
        let end = base
            .checked_add(size)
            .ok_or(UserspaceError::InvalidAddress)?;
        let first = Page::<Size4KiB>::containing_address(VirtAddr::new(base));
        let last = Page::<Size4KiB>::containing_address(VirtAddr::new(end - 1));
        let loading_flags = PageTableFlags::PRESENT
            | PageTableFlags::USER_ACCESSIBLE
            | PageTableFlags::WRITABLE
            | PageTableFlags::NO_EXECUTE;
        for page in Page::range_inclusive(first, last) {
            let frame = frame_allocator
                .allocate_frame()
                .ok_or(UserspaceError::InvalidAddress)?;
            let flush = unsafe { mapper.map_to(page, frame, loading_flags, frame_allocator) }
                .map_err(|_| UserspaceError::InvalidAddress)?;
            unsafe { flush.flush() };
            let phys = physical_memory_offset + frame.start_address().as_u64();
            unsafe { core::ptr::write_bytes(phys.as_mut_ptr::<u8>(), 0, Size4KiB::SIZE as usize) };
        }
        Ok(())
    }

    // UserMemoryLayout is public data; never trust a caller-constructed or mutated plan.
    if plan_user_memory(binary)? != *layout {
        return Err(UserspaceError::InvalidBinary);
    }
    if layout.segments.is_empty()
        || !layout.segments.iter().any(|s| {
            s.flags & 0x1 != 0
                && binary.entry_point >= s.virtual_start
                && binary.entry_point < s.virtual_end
        })
    {
        return Err(UserspaceError::InvalidBinary);
    }
    for segment in &layout.segments {
        map_zeroed_region(
            mapper,
            frame_allocator,
            physical_memory_offset,
            segment.page_start,
            segment.page_end - segment.page_start,
        )?;
    }
    map_zeroed_region(
        mapper,
        frame_allocator,
        physical_memory_offset,
        layout.heap_base,
        layout.heap_size,
    )?;
    map_zeroed_region(
        mapper,
        frame_allocator,
        physical_memory_offset,
        layout
            .stack_base
            .checked_sub(layout.stack_size)
            .ok_or(UserspaceError::InvalidAddress)?,
        layout.stack_size,
    )?;
    for segment in &layout.segments {
        let source = binary
            .segments
            .iter()
            .find(|source| {
                source.virtual_address == segment.virtual_start
                    && source.memory_size == segment.virtual_end - segment.virtual_start
                    && source.flags == segment.flags
            })
            .ok_or(UserspaceError::InvalidBinary)?;
        if source.bytes.len() as u64 != source.file_size || source.file_size > source.memory_size {
            return Err(UserspaceError::InvalidBinary);
        }
        if !source.bytes.is_empty() {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    source.bytes.as_ptr(),
                    segment.virtual_start as *mut u8,
                    source.bytes.len(),
                );
            }
        }
        let mut final_flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
        if segment.flags & 0x2 != 0 {
            final_flags |= PageTableFlags::WRITABLE;
        }
        if segment.flags & 0x1 == 0 {
            final_flags |= PageTableFlags::NO_EXECUTE;
        }
        let first = Page::<Size4KiB>::containing_address(VirtAddr::new(segment.page_start));
        let last = Page::<Size4KiB>::containing_address(VirtAddr::new(segment.page_end - 1));
        for page in Page::range_inclusive(first, last) {
            let flush = unsafe { mapper.update_flags(page, final_flags) }
                .map_err(|_| UserspaceError::InvalidAddress)?;
            unsafe { flush.flush() };
        }
    }
    Ok(())
}

/// Map a user binary into the active x86-64 page table.
///
/// This is the first real bridge from the modelled UserAddressSpace to CPU-visible
/// user memory. Code/data/stack receive USER_ACCESSIBLE PTEs; code is
/// executable and finally read-only, data and stack are writable **and**
/// non-executable (NO_EXECUTE), so W^X holds for every user region. The helper
/// uses the existing boot frame allocator and mapper instead of introducing
/// another VM.
#[cfg(feature = "x86-boot")]
pub unsafe fn map_user_binary(
    mapper: &mut OffsetPageTable<'static>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    physical_memory_offset: VirtAddr,
    binary: &UserBinary,
    addr_space: &UserAddressSpace,
) -> Result<(), UserspaceError> {
    if let Some(layout) = &addr_space.memory_layout {
        return unsafe { map_user_memory_layout(mapper, frame_allocator, physical_memory_offset, binary, layout) };
    }
    if binary.code.is_empty()
        || binary.code.len() as u64 > addr_space.code_size
        || binary.entry_point < addr_space.code_base
        || binary.entry_point >= addr_space.code_base + addr_space.code_size
    {
        return Err(UserspaceError::InvalidBinary);
    }

    fn map_region(
        mapper: &mut OffsetPageTable<'static>,
        frame_allocator: &mut impl FrameAllocator<Size4KiB>,
        physical_memory_offset: VirtAddr,
        base: u64,
        size: u64,
        flags: PageTableFlags,
    ) -> Result<(), UserspaceError> {
        let first = Page::<Size4KiB>::containing_address(VirtAddr::new(base));
        let last_addr = base
            .checked_add(size.saturating_sub(1))
            .ok_or(UserspaceError::InvalidAddress)?;
        let last = Page::<Size4KiB>::containing_address(VirtAddr::new(last_addr));
        for page in Page::range_inclusive(first, last) {
            let frame = frame_allocator
                .allocate_frame()
                .ok_or(UserspaceError::InvalidAddress)?;
            // Edition 2024: unsichere Operationen explizit markieren.
            // map_to programmiert die Seitentabelle; flush invalidiert den
            // TLB-Eintrag; write_bytes nullt den neuen Frame.
            let flush = unsafe {
                mapper.map_to(
                    page,
                    frame,
                    flags | PageTableFlags::PRESENT,
                    frame_allocator,
                )
            }
            .map_err(|_| UserspaceError::InvalidAddress)?;
            unsafe { flush.flush() };
            let phys = physical_memory_offset + frame.start_address().as_u64();
            unsafe { core::ptr::write_bytes(phys.as_mut_ptr::<u8>(), 0, Size4KiB::SIZE as usize) };
        }
        Ok(())
    }

    let code_base = binary.entry_point & !0xFFF;
    let code_end = binary
        .entry_point
        .checked_add(binary.code.len() as u64)
        .ok_or(UserspaceError::InvalidAddress)?;
    // Vergleich auf Seitenbasis: `code_base` ist absichtlich seitenbuendig
    // abgerundet, `addr_space.code_base` kann durch ein ELF mit nicht
    // seitenbuendigem Einstiegspunkt (typisch: 0x400010) unbuendig sein. Ein
    // Byte-Vergleich wies in diesem Fall jedes gueltige Ramdisk-Programm ab.
    let space_code_page = addr_space.code_base & !0xFFF;
    if code_base < space_code_page || code_end > addr_space.code_base + addr_space.code_size {
        return Err(UserspaceError::InvalidAddress);
    }
    let code_pages = ((code_end - code_base + 0xFFF) / 0x1000).max(1);
    map_region(
        mapper,
        frame_allocator,
        physical_memory_offset,
        code_base,
        code_pages * 0x1000,
        code_flags(),
    )?;

    // Erst pruefen, dann abbilden: ohne diese Grenze waechst ein zu grosser
    // Datenanteil in Stapel oder Haufen des Prozesses hinein.
    if !addr_space.data_fits(binary.data.len() as u64) {
        return Err(UserspaceError::InvalidAddress);
    }

    // Die Datenregion wird immer abgebildet, auch bei leerem Datensegment:
    // sonst waere "Daten ohne NX" im Betrieb nicht messbar, weil die Seite
    // schlicht fehlt. Der Inhalt wird weiterhin nur bei Bedarf kopiert.
    let data_pages = ((binary.data.len() as u64 + 0xFFF) / 0x1000).max(1);
    map_region(
        mapper,
        frame_allocator,
        physical_memory_offset,
        addr_space.data_base,
        data_pages * 0x1000,
        data_flags(),
    )?;

    let stack_pages = (addr_space.stack_size + 0xFFF) / 0x1000;
    let stack_base = addr_space.stack_base - stack_pages * 0x1000;
    map_region(
        mapper,
        frame_allocator,
        physical_memory_offset,
        stack_base,
        stack_pages * 0x1000,
        stack_flags(),
    )?;

    // Install the user image through the already active address space.
    // The target virtual addresses are now backed by real user PTEs.
    core::ptr::copy_nonoverlapping(
        binary.code.as_ptr(),
        binary.entry_point as *mut u8,
        binary.code.len(),
    );
    if !binary.data.is_empty() {
        core::ptr::copy_nonoverlapping(
            binary.data.as_ptr(),
            addr_space.data_base as *mut u8,
            binary.data.len(),
        );
    }

    // Code is writable only during image installation. Drop W before
    // Ring-3 entry, so user code pages are strictly read-only+user (W^X).
    for page in Page::range_inclusive(
        Page::<Size4KiB>::containing_address(VirtAddr::new(code_base)),
        Page::<Size4KiB>::containing_address(VirtAddr::new(code_base + code_pages * 0x1000 - 1)),
    ) {
        let flush = unsafe {
            mapper.update_flags(
                page,
                PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE,
            )
        }
        .map_err(|_| UserspaceError::InvalidAddress)?;
        unsafe { flush.flush() };
    }

    Ok(())
}

/// Enter a validated user context through the CPU's real IRETQ path.
#[cfg(feature = "x86-boot")]
pub unsafe fn enter_ring3(ctx: &UserContext) -> ! {
    debug_assert!(ctx.is_user_mode());
    debug_assert!(ctx.valid_address(ctx.rip));
    debug_assert!(ctx.valid_address(ctx.rsp));

    asm!(
        "push rax", // SS
        "push r10", // RSP
        "push rcx", // RFLAGS
        "push rdx", // CS
        "push rsi", // RIP
        "iretq",
        in("rax") ctx.ss as u64,
        in("r10") ctx.rsp,
        in("rcx") ctx.rflags,
        in("rdx") ctx.cs as u64,
        in("rsi") ctx.rip,
        options(noreturn)
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// User CPU Context (saved state for ring switches)
// ═══════════════════════════════════════════════════════════════════════════════

/// Saved CPU state for a user process (what IRET needs to restore)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserContext {
    /// Prozesskennung.
    pub pid: Pid,
    /// Ring, in dem der Prozess laeuft.
    pub privilege: PrivilegeLevel,
    /// Befehlszeiger.
    pub rip: u64, // Instruction Pointer
    /// Stapelzeiger.
    pub rsp: u64, // Stack Pointer
    /// Rahmenzeiger.
    pub rbp: u64, // Base Pointer
    /// Rueckgaberegister.
    pub rax: u64, // Return value register
    /// Code-Segment; 0x1B steht fuer Ring 3.
    pub cs: u16, // Code Segment selector (ring 3 = 0x1B, ring 0 = 0x08)
    /// Stapel-Segment; 0x23 steht fuer Ring 3.
    pub ss: u16, // Stack Segment selector (ring 3 = 0x23, ring 0 = 0x10)
    /// Flag-Register; fuer den Userspace muss das Interrupt-Flag gesetzt sein.
    pub rflags: u64, // CPU flags (IF must be set for user mode)
    /// Layout des Adressraums.
    pub addr_space: UserAddressSpace,
    /// Rueckgabewert, sobald der Prozess beendet ist.
    pub exit_code: Option<ExitCode>,
}

impl UserContext {
    /// Create initial context for a new user process
    pub fn new(pid: Pid, binary: &UserBinary, addr_space: UserAddressSpace) -> Self {
        Self {
            pid,
            privilege: PrivilegeLevel::User,
            rip: binary.entry_point,
            rsp: addr_space.initial_rsp(),
            rbp: addr_space.initial_rsp(),
            rax: 0,
            cs: 0x1B,      // GDT entry 3, ring 3 (0x1B = (3 << 3) | 3)
            ss: 0x23,      // GDT entry 4, ring 3 (0x23 = (4 << 3) | 3)
            rflags: 0x202, // IF=1 (interrupts enabled), reserved bit 1
            addr_space,
            exit_code: None,
        }
    }

    /// Check if this context is in user mode
    pub fn is_user_mode(&self) -> bool {
        self.privilege.is_user()
    }
    /// Check if an address is within this process's address space
    pub fn valid_address(&self, addr: u64) -> bool {
        self.addr_space.contains(addr)
    }
    /// Set syscall return value
    pub fn set_return(&mut self, value: u64) {
        self.rax = value;
    }
    /// Mark process as exited
    pub fn exit(&mut self, code: ExitCode) {
        self.exit_code = Some(code);
    }
    /// Check if process has exited
    pub fn is_exited(&self) -> bool {
        self.exit_code.is_some()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// GDT Segment Selectors for Ring 3
// ═══════════════════════════════════════════════════════════════════════════════

/// GDT layout for ShivaCore with user segments:
///   Entry 0: Null
///   Entry 1: Kernel Code (ring 0)  → selector 0x08
///   Entry 2: Kernel Data (ring 0)  → selector 0x10
///   Entry 3: User Code (ring 3)    → selector 0x1B
///   Entry 4: User Data (ring 3)    → selector 0x23
///   Entry 5: TSS                    → selector 0x2B
pub struct GdtSelectors {
    /// Kernel-Code, 0x08.
    pub kernel_cs: u16, // 0x08
    /// Kernel-Daten, 0x10.
    pub kernel_ds: u16, // 0x10
    /// User-Code, 0x1B.
    pub user_cs: u16, // 0x1B
    /// User-Daten, 0x23.
    pub user_ds: u16, // 0x23
    /// Task-State-Segment, 0x2B.
    pub tss: u16, // 0x2B
}

impl Default for GdtSelectors {
    fn default() -> Self {
        Self {
            kernel_cs: 0x08,
            kernel_ds: 0x10,
            user_cs: 0x1B, // (3 << 3) | 3 = 24 + 3 = 0x1B
            user_ds: 0x23, // (4 << 3) | 3 = 32 + 3 = 0x23
            tss: 0x2B,     // (5 << 3) | 3 = 40 + 3 = 0x2B
        }
    }
}

impl GdtSelectors {
    /// Verify that user selectors have ring 3 DPL
    pub fn verify(&self) -> bool {
        (self.user_cs & 0x03) == 3
            && (self.user_ds & 0x03) == 3
            && (self.kernel_cs & 0x03) == 0
            && (self.kernel_ds & 0x03) == 0
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Userspace Errors
// ═══════════════════════════════════════════════════════════════════════════════

/// Ablehnungsgruende der Prozessverwaltung.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UserspaceError {
    /// Kein Prozess mit dieser Kennung.
    ProcessNotFound,
    /// Der Prozess ist bereits beendet.
    ProcessAlreadyExited,
    /// Die Adresse liegt ausserhalb des Adressraums.
    InvalidAddress,
    /// Der Zugriff faellt in kein Segment.
    SegmentFault,
    /// Der Stapel ist erschoepft.
    StackOverflow,
    /// Die Datei ist nicht ladbar, insbesondere wenn der Code leer ist.
    InvalidBinary,
    /// Der Zugriff verletzt die Rechte.
    PermissionDenied,
    /// Die Hoechstzahl gleichzeitiger Prozesse ist erreicht.
    MaxProcessesReached,
}

impl core::fmt::Display for UserspaceError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            UserspaceError::ProcessNotFound => write!(f, "user process not found"),
            UserspaceError::ProcessAlreadyExited => write!(f, "process already exited"),
            UserspaceError::InvalidAddress => write!(f, "invalid memory address"),
            UserspaceError::SegmentFault => write!(f, "segmentation fault"),
            UserspaceError::StackOverflow => write!(f, "stack overflow"),
            UserspaceError::InvalidBinary => write!(f, "invalid binary format"),
            UserspaceError::PermissionDenied => write!(f, "permission denied"),
            UserspaceError::MaxProcessesReached => write!(f, "max user processes reached"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Userspace Manager
// ═══════════════════════════════════════════════════════════════════════════════

/// Maximum number of concurrent user processes
const MAX_USER_PROCESSES: usize = 64;

/// Verwaltung der User-Prozesse mit Kennungsvergabe und hinterlegten Segmentselektoren.
pub struct UserspaceManager {
    users: Vec<UserContext>,
    next_pid: u32,
    syscall_count: u64,
    gdt: GdtSelectors,
}

impl Default for UserspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl UserspaceManager {
    /// Leere Verwaltung; Prozesskennungen beginnen bei 1000.
    pub fn new() -> Self {
        Self {
            users: Vec::new(),
            next_pid: 1000, // User PIDs start at 1000
            syscall_count: 0,
            gdt: GdtSelectors::default(),
        }
    }

    /// Load a user binary and create a user process (ring 3)
    pub fn load_binary(&mut self, binary: UserBinary) -> Result<Pid, UserspaceError> {
        if self.users.len() >= MAX_USER_PROCESSES {
            return Err(UserspaceError::MaxProcessesReached);
        }
        if binary.code.is_empty() {
            return Err(UserspaceError::InvalidBinary);
        }
        let pid = Pid(self.next_pid);
        self.next_pid += 1;
        let addr_space = UserAddressSpace::default();
        let ctx = UserContext::new(pid, &binary, addr_space);
        self.users.push(ctx);
        Ok(pid)
    }

    /// Get the context for a user process
    pub fn get_context(&self, pid: Pid) -> Option<&UserContext> {
        self.users.iter().find(|u| u.pid == pid)
    }
    /// Veraenderbarer Zugriff auf den Kontext eines Prozesses.
    pub fn get_context_mut(&mut self, pid: Pid) -> Option<&mut UserContext> {
        self.users.iter_mut().find(|u| u.pid == pid)
    }

    /// Prepare ring switch: validate context and return selectors
    pub fn enter_userspace(&self, pid: Pid) -> Result<&UserContext, UserspaceError> {
        let ctx = self
            .users
            .iter()
            .find(|u| u.pid == pid)
            .ok_or(UserspaceError::ProcessNotFound)?;
        if ctx.is_exited() {
            return Err(UserspaceError::ProcessAlreadyExited);
        }
        if !ctx.privilege.is_user() {
            return Err(UserspaceError::PermissionDenied);
        }
        if !ctx.valid_address(ctx.rip) {
            return Err(UserspaceError::SegmentFault);
        }
        if !ctx.valid_address(ctx.rsp) {
            return Err(UserspaceError::StackOverflow);
        }
        Ok(ctx)
    }

    /// Handle a syscall from user mode
    pub fn handle_syscall(
        &mut self,
        pid: Pid,
        num: u32,
        _args: &[u64],
    ) -> Result<u64, UserspaceError> {
        self.syscall_count += 1;
        let ctx = self
            .users
            .iter_mut()
            .find(|u| u.pid == pid)
            .ok_or(UserspaceError::ProcessNotFound)?;
        if ctx.is_exited() {
            return Err(UserspaceError::ProcessAlreadyExited);
        }
        // Simulate syscall execution
        match num {
            0 => {
                ctx.exit(0);
                Ok(0)
            } // exit(0)
            1 => Ok(ctx.rsp),                 // get_stack_ptr
            2 => Ok(ctx.rip),                 // get_instruction_ptr
            3 => Ok(self.users.len() as u64), // get_process_count
            _ => Err(UserspaceError::PermissionDenied),
        }
    }

    /// Exit a user process
    pub fn exit_process(&mut self, pid: Pid, code: ExitCode) -> bool {
        if let Some(ctx) = self.get_context_mut(pid) {
            if ctx.is_exited() {
                return false;
            }
            ctx.exit(code);
            return true;
        }
        false
    }

    /// Remove exited processes (cleanup)
    pub fn reap_dead(&mut self) -> usize {
        let before = self.users.len();
        self.users.retain(|u| !u.is_exited());
        before - self.users.len()
    }

    /// List all user processes
    pub fn list_users(&self) -> Vec<&UserContext> {
        self.users.iter().collect()
    }
    /// Anzahl der eingetragenen Prozesse, beendete einbegriffen.
    pub fn user_count(&self) -> usize {
        self.users.len()
    }
    /// Anzahl der nicht beendeten Prozesse.
    pub fn active_count(&self) -> usize {
        self.users.iter().filter(|u| !u.is_exited()).count()
    }
    /// Anzahl der gezaehlten Systemaufrufe.
    pub fn syscall_count(&self) -> u64 {
        self.syscall_count
    }
    /// Die hinterlegten Segmentselektoren.
    pub fn gdt_selectors(&self) -> &GdtSelectors {
        &self.gdt
    }

    /// Validate a memory access from user space (bounds check)
    pub fn check_memory_access(
        &self,
        pid: Pid,
        addr: u64,
        size: u64,
    ) -> Result<(), UserspaceError> {
        let ctx = self
            .get_context(pid)
            .ok_or(UserspaceError::ProcessNotFound)?;
        if ctx.is_exited() {
            return Err(UserspaceError::ProcessAlreadyExited);
        }
        let end = addr
            .checked_add(size)
            .ok_or(UserspaceError::InvalidAddress)?;
        if !ctx.valid_address(addr) || !ctx.valid_address(end - 1) {
            return Err(UserspaceError::SegmentFault);
        }
        Ok(())
    }

    /// Simulate stack push from user mode
    pub fn push_stack(&mut self, pid: Pid, value: u64) -> Result<(), UserspaceError> {
        let ctx = self
            .get_context_mut(pid)
            .ok_or(UserspaceError::ProcessNotFound)?;
        if ctx.is_exited() {
            return Err(UserspaceError::ProcessAlreadyExited);
        }
        let new_rsp = ctx
            .rsp
            .checked_sub(8)
            .ok_or(UserspaceError::StackOverflow)?;
        if !ctx.addr_space.in_stack(new_rsp) {
            return Err(UserspaceError::StackOverflow);
        }
        ctx.rsp = new_rsp;
        Ok(())
    }

    /// Simulate stack pop from user mode
    pub fn pop_stack(&mut self, pid: Pid) -> Result<u64, UserspaceError> {
        let ctx = self
            .get_context_mut(pid)
            .ok_or(UserspaceError::ProcessNotFound)?;
        if ctx.is_exited() {
            return Err(UserspaceError::ProcessAlreadyExited);
        }
        if !ctx.addr_space.in_stack(ctx.rsp) {
            return Err(UserspaceError::StackOverflow);
        }
        let val = ctx.rax; // Simulated pop goes to rax
        ctx.rsp += 8;
        if ctx.rsp > ctx.addr_space.stack_base {
            ctx.rsp = ctx.addr_space.stack_base;
        }
        Ok(val)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

/// Liest die tatsaechlich programmierten PTE-Flags einer virtuellen Adresse aus
/// einer vorgegebenen Seitentabelle.
///
/// Warum das im Kernel steht: die Absicht des Mappers ist kein Beweis, der
/// Seitentabelleneintrag ist einer. Der W^X-Nachweis beim Boot liest genau
/// diese Flags zurueck, statt den gesetzten Argumenten zu glauben.
///
/// # Safety
/// `table_root` muss eine gueltige, aktive oder inaktive PML4 sein und
/// `physical_memory_offset` den physikalischen Speicher abbilden.
#[cfg(feature = "x86-boot")]
pub unsafe fn page_flags(
    physical_memory_offset: x86_64::VirtAddr,
    table_root: x86_64::PhysAddr,
    virt: x86_64::VirtAddr,
) -> Option<PageTableFlags> {
    use x86_64::structures::paging::page_table::PageTable;

    let mut table_phys = table_root;
    for level in (1u8..=4).rev() {
        let table_virt = physical_memory_offset + table_phys.as_u64();
        let table = unsafe { &*(table_virt.as_ptr::<PageTable>()) };
        let index = match level {
            4 => virt.p4_index(),
            3 => virt.p3_index(),
            2 => virt.p2_index(),
            _ => virt.p1_index(),
        };
        let entry = &table[index];
        let flags = entry.flags();
        if !flags.contains(PageTableFlags::PRESENT) {
            return None;
        }
        if level == 1 || flags.contains(PageTableFlags::HUGE_PAGE) {
            return Some(flags);
        }
        table_phys = entry.addr();
    }
    None
}

#[cfg(all(test, feature = "x86-boot"))]
mod wx_policy_tests {
    use super::{code_flags, data_flags, is_write_execute, stack_flags};
    use x86_64::structures::paging::PageTableFlags;

    #[test]
    fn data_pages_are_not_executable() {
        let flags = data_flags();
        assert!(flags.contains(PageTableFlags::NO_EXECUTE));
        assert!(flags.contains(PageTableFlags::USER_ACCESSIBLE));
        assert!(!is_write_execute(flags));
    }

    #[test]
    fn stack_pages_are_not_executable() {
        let flags = stack_flags();
        assert!(flags.contains(PageTableFlags::NO_EXECUTE));
        assert!(flags.contains(PageTableFlags::USER_ACCESSIBLE));
        assert!(!is_write_execute(flags));
    }

    #[test]
    fn code_is_writable_only_during_install() {
        // Waehrend des Ladens schreibbar (sonst kein Kopieren), danach setzt
        // update_flags PRESENT|USER_ACCESSIBLE und nimmt W heraus.
        let flags = code_flags();
        assert!(flags.contains(PageTableFlags::USER_ACCESSIBLE));
        assert!(flags.contains(PageTableFlags::WRITABLE));
        // Schreibbar und ausfuehrbar zugleich ist nur der Ladezustand: hier
        // bleibt NX aus, weil der Code ausgefuehrt werden soll.
        assert!(!flags.contains(PageTableFlags::NO_EXECUTE));
    }

    #[test]
    fn write_execute_predicate_flags_violation() {
        let rwx = PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
        assert!(is_write_execute(rwx));
        let rw = rwx | PageTableFlags::NO_EXECUTE;
        assert!(!is_write_execute(rw));
    }
}

#[cfg(test)]
mod tests_namespace_guard {
    // Platzhalter, damit dieses Modul nicht mit `mod tests` weiter unten
    // kollidiert, falls eines der beiden entfernt wird.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_check_rejects_half_outside_and_overflow() {
        let space = UserAddressSpace::default();
        let data = space.data_base;
        assert!(space.range_writable(data, 56));
        assert!(space.range_writable(data + space.data_size - 56, 56));
        assert!(!space.range_writable(data + space.data_size - 8, 56));
        assert!(!space.range_writable(u64::MAX - 8, 56));
        assert!(space.range_writable(0, 0));
    }

    #[test]
    fn range_check_accepts_the_stack_but_not_the_code() {
        let space = UserAddressSpace::default();
        let stack = space.stack_base - 64;
        assert!(space.range_writable(stack, 56));
        assert!(!space.range_writable(stack, 65));
        assert!(!space.range_writable(space.code_base, 16));
    }

    // --- PrivilegeLevel tests ---

    #[test]
    fn test_privilege_levels() {
        assert_eq!(PrivilegeLevel::Kernel.dpl(), 0);
        assert_eq!(PrivilegeLevel::User.dpl(), 3);
        assert!(PrivilegeLevel::Kernel.is_kernel());
        assert!(PrivilegeLevel::User.is_user());
        assert!(!PrivilegeLevel::Kernel.is_user());
        assert!(!PrivilegeLevel::User.is_kernel());
    }

    // --- UserAddressSpace tests ---

    #[test]
    fn test_default_address_space() {
        let asp = UserAddressSpace::default();
        assert_eq!(asp.code_base, 0x00400000);
        assert_eq!(asp.stack_base, 0x7FFFF000);
        assert!(asp.code_size > 0);
        assert!(asp.stack_size > 0);
    }

    #[test]
    fn test_address_space_contains() {
        let asp = UserAddressSpace::default();
        assert!(asp.in_code(0x00400000));
        assert!(asp.in_code(0x004FFFFF));
        assert!(!asp.in_code(0x00500000));
        assert!(asp.in_data(0x00500000));
        assert!(asp.in_stack(0x7FFFEFFF));
        assert!(asp.in_stack(0x7FFFF000));
        assert!(asp.in_heap(0x00600000));
        assert!(!asp.in_heap(0x005FFFFF));
    }

    #[test]
    fn test_initial_rsp() {
        let asp = UserAddressSpace::default();
        assert_eq!(asp.initial_rsp(), 0x7FFFF000);
    }

    #[test]
    fn test_address_outside_space() {
        let asp = UserAddressSpace::default();
        assert!(!asp.contains(0x00000000));
        assert!(!asp.contains(0xFFFFFFFF));
        assert!(!asp.contains(0x003FFFFF));
    }

    #[test]
    fn segment_layout_preserves_elf_addresses_and_places_private_regions() {
        let mut binary = UserBinary::from_bytes("elf", vec![0x90], 0x01C0_0000);
        binary.segments = vec![
            UserLoadSegment {
                file_offset: 0,
                virtual_address: 0x01C0_0000,
                file_size: 0x100,
                memory_size: 0x1000,
                flags: 0x5,
                alignment: 0x1000,
                bytes: vec![0x90; 0x100],
            },
            UserLoadSegment {
                file_offset: 0x1000,
                virtual_address: 0x01D0_0000,
                file_size: 0x80,
                memory_size: 0x2000,
                flags: 0x6,
                alignment: 0x1000,
                bytes: vec![0; 0x80],
            },
        ];
        let layout = plan_user_memory(&binary).unwrap();
        assert_eq!(layout.segments[0].virtual_start, 0x01C0_0000);
        assert_eq!(layout.segments[1].virtual_start, 0x01D0_0000);
        assert!(layout.heap_base + layout.heap_size <= layout.stack_base - layout.stack_size);
        assert!(layout.segments.iter().all(|segment| {
            !(layout.heap_base < segment.page_end
                && segment.page_start < layout.heap_base + layout.heap_size)
                && !(layout.stack_base - layout.stack_size < segment.page_end
                    && segment.page_start < layout.stack_base)
        }));
    }

    #[test]
    fn segment_layout_rejects_page_overlap_and_wx() {
        let mut binary = UserBinary::from_bytes("elf", vec![0x90], 0x01C0_0000);
        binary.segments = vec![
            UserLoadSegment {
                file_offset: 0,
                virtual_address: 0x01C0_0000,
                file_size: 0x100,
                memory_size: 0x1000,
                flags: 0x5,
                alignment: 1,
                bytes: vec![0x90; 0x100],
            },
            UserLoadSegment {
                file_offset: 0x100,
                virtual_address: 0x01C0_0800,
                file_size: 0x100,
                memory_size: 0x1000,
                flags: 0x6,
                alignment: 1,
                bytes: vec![0; 0x100],
            },
        ];
        assert_eq!(
            plan_user_memory(&binary),
            Err(UserspaceError::InvalidAddress)
        );
        binary.segments[1].virtual_address = 0x01D0_0000;
        binary.segments[1].flags = 0x7;
        assert_eq!(
            plan_user_memory(&binary),
            Err(UserspaceError::InvalidBinary)
        );
    }

    #[test]
    fn elf_layout_drives_address_and_syscall_range_checks() {
        let mut binary = UserBinary::from_bytes("elf", vec![0x90], 0x01C0_0000);
        binary.segments = vec![
            UserLoadSegment {
                file_offset: 0,
                virtual_address: 0x01C0_0000,
                file_size: 0x100,
                memory_size: 0x1000,
                flags: 0x5,
                alignment: 0x1000,
                bytes: vec![0x90; 0x100],
            },
            UserLoadSegment {
                file_offset: 0x1000,
                virtual_address: 0x01D0_0000,
                file_size: 0x80,
                memory_size: 0x2000,
                flags: 0x6,
                alignment: 0x1000,
                bytes: vec![0; 0x80],
            },
        ];
        let layout = plan_user_memory(&binary).unwrap();
        let space = UserAddressSpace::from_memory_layout(layout.clone());
        assert!(space.in_code(0x01C0_0000));
        assert!(space.in_data(0x01D0_0000));
        assert!(space.range_readable(0x01C0_0000, 0x100));
        assert!(!space.range_writable(0x01C0_0000, 0x10));
        assert!(space.range_writable(0x01D0_0000, 0x80));
        assert!(!space.range_readable(0x01D0_2FF0, 0x20));
        assert_eq!(space.initial_rsp(), layout.stack_base);
        assert!(space.contains(layout.heap_base));
        let regions = space.mapped_regions();
        assert_eq!(regions.len(), 4);
        assert!(regions.contains(&(layout.segments[0].page_start, layout.segments[0].page_end)));
        assert!(regions.contains(&(layout.segments[1].page_start, layout.segments[1].page_end)));
    }

    // --- UserBinary tests ---

    #[test]
    fn test_hello_world_binary() {
        let bin = UserBinary::hello_world();
        assert_eq!(bin.entry_point, 0x00400000);
        assert_eq!(bin.code, vec![0xF4]);
        assert_eq!(bin.name, "hello");
        assert_eq!(bin.code_len(), 1);
        assert_eq!(bin.data_len(), 0);
    }

    #[test]
    fn data_fenster_grenze_wird_eingehalten() {
        let space = UserAddressSpace::default();
        assert!(space.data_fits(space.data_size), "Fenster selbst passt");
        assert!(
            !space.data_fits(space.data_size + 1),
            "ein Byte zu viel passt nicht"
        );
        assert!(space.data_fits(0), "leerer Datenanteil ist zulaessig");
    }

    #[test]
    fn test_binary_from_bytes() {
        let code = vec![0x90, 0xF4]; // NOP, HLT
        let bin = UserBinary::from_bytes("test", code.clone(), 0x00400000);
        assert_eq!(bin.code, code);
        assert_eq!(bin.name, "test");
        assert_eq!(bin.code_len(), 2);
    }

    // --- UserContext tests ---

    #[test]
    fn test_user_context_creation() {
        let bin = UserBinary::hello_world();
        let asp = UserAddressSpace::default();
        let ctx = UserContext::new(Pid(1000), &bin, asp);
        assert_eq!(ctx.pid, Pid(1000));
        assert!(ctx.is_user_mode());
        assert_eq!(ctx.rip, 0x00400000);
        assert_eq!(ctx.rsp, 0x7FFFF000);
        assert_eq!(ctx.cs, 0x1B);
        assert_eq!(ctx.ss, 0x23);
        assert!(!ctx.is_exited());
    }

    #[test]
    fn test_user_context_exit() {
        let bin = UserBinary::hello_world();
        let asp = UserAddressSpace::default();
        let mut ctx = UserContext::new(Pid(1001), &bin, asp);
        assert!(!ctx.is_exited());
        ctx.exit(42);
        assert!(ctx.is_exited());
        assert_eq!(ctx.exit_code, Some(42));
    }

    #[test]
    fn test_user_context_set_return() {
        let bin = UserBinary::hello_world();
        let asp = UserAddressSpace::default();
        let mut ctx = UserContext::new(Pid(1002), &bin, asp);
        assert_eq!(ctx.rax, 0);
        ctx.set_return(0x1234);
        assert_eq!(ctx.rax, 0x1234);
    }

    #[test]
    fn test_user_context_valid_address() {
        let bin = UserBinary::hello_world();
        let asp = UserAddressSpace::default();
        let ctx = UserContext::new(Pid(1003), &bin, asp);
        assert!(ctx.valid_address(0x00400000));
        assert!(ctx.valid_address(0x7FFFEFFF));
        assert!(!ctx.valid_address(0x00000000));
    }

    // --- GdtSelectors tests ---

    #[test]
    fn test_gdt_selectors_default() {
        let gdt = GdtSelectors::default();
        assert_eq!(gdt.kernel_cs, 0x08);
        assert_eq!(gdt.kernel_ds, 0x10);
        assert_eq!(gdt.user_cs, 0x1B);
        assert_eq!(gdt.user_ds, 0x23);
    }

    #[test]
    fn test_gdt_selectors_verify() {
        let gdt = GdtSelectors::default();
        assert!(gdt.verify());
        // User selectors must have ring 3 DPL
        assert_eq!(gdt.user_cs & 0x03, 3);
        assert_eq!(gdt.user_ds & 0x03, 3);
        // Kernel selectors must have ring 0 DPL
        assert_eq!(gdt.kernel_cs & 0x03, 0);
        assert_eq!(gdt.kernel_ds & 0x03, 0);
    }

    // --- UserspaceManager tests ---

    #[test]
    fn test_manager_new() {
        let mgr = UserspaceManager::new();
        assert_eq!(mgr.user_count(), 0);
        assert_eq!(mgr.syscall_count(), 0);
        assert!(mgr.gdt_selectors().verify());
    }

    #[test]
    fn test_load_binary_creates_user() {
        let mut mgr = UserspaceManager::new();
        let bin = UserBinary::hello_world();
        let pid = mgr.load_binary(bin).unwrap();
        assert_eq!(pid, Pid(1000));
        assert_eq!(mgr.user_count(), 1);
        assert_eq!(mgr.active_count(), 1);
    }

    #[test]
    fn test_load_multiple_binaries() {
        let mut mgr = UserspaceManager::new();
        for i in 0..5 {
            let bin = UserBinary::from_bytes(&format!("proc{}", i), vec![0xF4], 0x00400000);
            let pid = mgr.load_binary(bin).unwrap();
            assert_eq!(pid, Pid(1000 + i));
        }
        assert_eq!(mgr.user_count(), 5);
        assert_eq!(mgr.active_count(), 5);
    }

    #[test]
    fn test_load_empty_binary_fails() {
        let mut mgr = UserspaceManager::new();
        let bin = UserBinary::from_bytes("empty", vec![], 0);
        assert_eq!(mgr.load_binary(bin), Err(UserspaceError::InvalidBinary));
    }

    #[test]
    fn test_enter_userspace_valid() {
        let mut mgr = UserspaceManager::new();
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        let ctx = mgr.enter_userspace(pid);
        assert!(ctx.is_ok());
        let ctx = ctx.unwrap();
        assert_eq!(ctx.rip, 0x00400000);
        assert_eq!(ctx.rsp, 0x7FFFF000);
    }

    #[test]
    fn test_enter_userspace_not_found() {
        let mgr = UserspaceManager::new();
        assert_eq!(
            mgr.enter_userspace(Pid(9999)),
            Err(UserspaceError::ProcessNotFound)
        );
    }

    #[test]
    fn test_enter_userspace_exited() {
        let mut mgr = UserspaceManager::new();
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        mgr.exit_process(pid, 0);
        assert_eq!(
            mgr.enter_userspace(pid),
            Err(UserspaceError::ProcessAlreadyExited)
        );
    }

    #[test]
    fn test_handle_syscall_exit() {
        let mut mgr = UserspaceManager::new();
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        let result = mgr.handle_syscall(pid, 0, &[]); // syscall 0 = exit
        assert_eq!(result, Ok(0));
        assert_eq!(mgr.syscall_count(), 1);
        let ctx = mgr.get_context(pid).unwrap();
        assert!(ctx.is_exited());
    }

    #[test]
    fn test_handle_syscall_get_info() {
        let mut mgr = UserspaceManager::new();
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        let rsp = mgr.handle_syscall(pid, 1, &[]).unwrap();
        assert_eq!(rsp, 0x7FFFF000);
        let rip = mgr.handle_syscall(pid, 2, &[]).unwrap();
        assert_eq!(rip, 0x00400000);
        let count = mgr.handle_syscall(pid, 3, &[]).unwrap();
        assert_eq!(count, 1);
        assert_eq!(mgr.syscall_count(), 3);
    }

    #[test]
    fn test_handle_syscall_unknown() {
        let mut mgr = UserspaceManager::new();
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        assert_eq!(
            mgr.handle_syscall(pid, 99, &[]),
            Err(UserspaceError::PermissionDenied)
        );
    }

    #[test]
    fn test_handle_syscall_exited_process() {
        let mut mgr = UserspaceManager::new();
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        mgr.exit_process(pid, 0);
        assert_eq!(
            mgr.handle_syscall(pid, 1, &[]),
            Err(UserspaceError::ProcessAlreadyExited)
        );
    }

    #[test]
    fn test_exit_process() {
        let mut mgr = UserspaceManager::new();
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        assert!(mgr.exit_process(pid, 42));
        let ctx = mgr.get_context(pid).unwrap();
        assert!(ctx.is_exited());
        assert_eq!(ctx.exit_code, Some(42));
        assert_eq!(mgr.active_count(), 0);
    }

    #[test]
    fn test_exit_process_twice() {
        let mut mgr = UserspaceManager::new();
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        assert!(mgr.exit_process(pid, 0));
        assert!(!mgr.exit_process(pid, 0)); // Already exited
    }

    #[test]
    fn test_exit_nonexistent() {
        let mut mgr = UserspaceManager::new();
        assert!(!mgr.exit_process(Pid(9999), 0));
    }

    #[test]
    fn test_reap_dead() {
        let mut mgr = UserspaceManager::new();
        let p1 = mgr.load_binary(UserBinary::hello_world()).unwrap();
        let p2 = mgr.load_binary(UserBinary::hello_world()).unwrap();
        let _p3 = mgr.load_binary(UserBinary::hello_world()).unwrap();
        mgr.exit_process(p1, 0);
        mgr.exit_process(p2, 1);
        assert_eq!(mgr.user_count(), 3);
        assert_eq!(mgr.active_count(), 1);
        let reaped = mgr.reap_dead();
        assert_eq!(reaped, 2);
        assert_eq!(mgr.user_count(), 1);
        assert_eq!(mgr.active_count(), 1);
    }

    #[test]
    fn test_reap_no_dead() {
        let mut mgr = UserspaceManager::new();
        let _p1 = mgr.load_binary(UserBinary::hello_world()).unwrap();
        let _p2 = mgr.load_binary(UserBinary::hello_world()).unwrap();
        assert_eq!(mgr.reap_dead(), 0);
        assert_eq!(mgr.user_count(), 2);
    }

    #[test]
    fn test_list_users() {
        let mut mgr = UserspaceManager::new();
        let _p1 = mgr.load_binary(UserBinary::hello_world()).unwrap();
        let _p2 = mgr
            .load_binary(UserBinary::from_bytes("b", vec![0x90], 0x400000))
            .unwrap();
        let users = mgr.list_users();
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].pid, Pid(1000));
        assert_eq!(users[1].pid, Pid(1001));
    }

    #[test]
    fn test_check_memory_access_valid() {
        let mut mgr = UserspaceManager::new();
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        assert!(mgr.check_memory_access(pid, 0x00400000, 4).is_ok());
        assert!(mgr.check_memory_access(pid, 0x7FFFEFFF, 1).is_ok());
    }

    #[test]
    fn test_check_memory_access_invalid() {
        let mut mgr = UserspaceManager::new();
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        assert_eq!(
            mgr.check_memory_access(pid, 0x00000000, 4),
            Err(UserspaceError::SegmentFault)
        );
        assert_eq!(
            mgr.check_memory_access(pid, 0xFFFFFFFF, 1),
            Err(UserspaceError::SegmentFault)
        );
    }

    #[test]
    fn test_check_memory_access_exited() {
        let mut mgr = UserspaceManager::new();
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        mgr.exit_process(pid, 0);
        assert_eq!(
            mgr.check_memory_access(pid, 0x00400000, 4),
            Err(UserspaceError::ProcessAlreadyExited)
        );
    }

    #[test]
    fn test_push_stack() {
        let mut mgr = UserspaceManager::new();
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        let ctx = mgr.get_context(pid).unwrap();
        let initial_rsp = ctx.rsp;
        assert!(mgr.push_stack(pid, 0x1234).is_ok());
        let ctx = mgr.get_context(pid).unwrap();
        assert_eq!(ctx.rsp, initial_rsp - 8);
    }

    #[test]
    fn test_pop_stack() {
        let mut mgr = UserspaceManager::new();
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        mgr.push_stack(pid, 0x5678).unwrap();
        let val = mgr.pop_stack(pid).unwrap();
        let ctx = mgr.get_context(pid).unwrap();
        assert_eq!(ctx.rsp, 0x7FFFF000); // Back to original
    }

    #[test]
    fn test_stack_overflow_protection() {
        let mut mgr = UserspaceManager::new();
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        // Exhaust stack by pushing past the limit
        let ctx = mgr.get_context(pid).unwrap();
        let stack_bottom = ctx.addr_space.stack_base - ctx.addr_space.stack_size;
        // Set rsp to just above stack bottom
        if let Some(c) = mgr.get_context_mut(pid) {
            c.rsp = stack_bottom + 8;
        }
        // This push should overflow
        assert_eq!(mgr.push_stack(pid, 0), Err(UserspaceError::StackOverflow));
    }

    #[test]
    fn test_max_user_processes() {
        let mut mgr = UserspaceManager::new();
        // Load MAX_USER_PROCESSES (64) processes
        for _ in 0..MAX_USER_PROCESSES {
            let bin = UserBinary::hello_world();
            assert!(mgr.load_binary(bin).is_ok());
        }
        assert_eq!(mgr.user_count(), MAX_USER_PROCESSES);
        // Next one should fail
        let bin = UserBinary::hello_world();
        assert_eq!(
            mgr.load_binary(bin),
            Err(UserspaceError::MaxProcessesReached)
        );
    }

    #[test]
    fn test_pid_increment() {
        let mut mgr = UserspaceManager::new();
        let p1 = mgr.load_binary(UserBinary::hello_world()).unwrap();
        let p2 = mgr.load_binary(UserBinary::hello_world()).unwrap();
        let p3 = mgr.load_binary(UserBinary::hello_world()).unwrap();
        assert_eq!(p1, Pid(1000));
        assert_eq!(p2, Pid(1001));
        assert_eq!(p3, Pid(1002));
    }

    #[test]
    fn test_get_context_mut() {
        let mut mgr = UserspaceManager::new();
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        let ctx = mgr.get_context_mut(pid).unwrap();
        ctx.rax = 0xDEAD;
        let ctx = mgr.get_context(pid).unwrap();
        assert_eq!(ctx.rax, 0xDEAD);
    }

    #[test]
    fn test_full_lifecycle() {
        let mut mgr = UserspaceManager::new();
        // 1. Load binary
        let pid = mgr.load_binary(UserBinary::hello_world()).unwrap();
        assert_eq!(mgr.active_count(), 1);
        // 2. Enter userspace (validate)
        let ctx = mgr.enter_userspace(pid).unwrap();
        assert!(ctx.is_user_mode());
        let ctx = ctx.clone(); // Borrow-Ende: handle_syscall nimmt &mut self
        // 3. Handle syscalls
        let rsp = mgr.handle_syscall(pid, 1, &[]).unwrap();
        assert!(rsp > 0);
        // 4. Check memory
        assert!(mgr.check_memory_access(pid, ctx.rip, 1).is_ok());
        // 5. Exit
        assert!(mgr.exit_process(pid, 0));
        assert_eq!(mgr.active_count(), 0);
        // 6. Reap
        assert_eq!(mgr.reap_dead(), 1);
        assert_eq!(mgr.user_count(), 0);
    }

    #[test]
    fn test_userspace_error_display() {
        assert_eq!(
            format!("{}", UserspaceError::ProcessNotFound),
            "user process not found"
        );
        assert_eq!(
            format!("{}", UserspaceError::SegmentFault),
            "segmentation fault"
        );
        assert_eq!(
            format!("{}", UserspaceError::StackOverflow),
            "stack overflow"
        );
        assert_eq!(
            format!("{}", UserspaceError::InvalidBinary),
            "invalid binary format"
        );
        assert_eq!(
            format!("{}", UserspaceError::MaxProcessesReached),
            "max user processes reached"
        );
    }
}
