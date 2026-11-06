// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ShivaCore — Kernel-Einstiegspunkt.
// K-Sprint 0: Boot (BIOS+UEFI via `bootloader` 0.11), serielle Debug-Konsole,
// Framebuffer-Textausgabe.
// K-Sprint 1: GDT + TSS (Double-Fault-Stack), IDT (Breakpoint/Double-Fault/
// Page-Fault), PIC-Remapping (0x20-0x2F), Timer+Keyboard-Interrupts aktiv.
// K-Sprint 2: Paging-Mapper (OffsetPageTable), Frame-Allocator, Heap-
// Allokator (linked_list_allocator) -- `alloc` (Box/Vec/String) nutzbar.
// Kein Linux-Unterbau, kein Fremdcode jenseits des minimalen Boot-Protokolls.
#![no_std]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![no_main]

extern crate alloc;

mod allocator;
mod framebuffer;
mod gdt;
mod hal;
mod interrupts;
mod memory;
mod serial;

use alloc::{boxed::Box, vec::Vec};
use bootloader_api::{
    config::{BootloaderConfig, Mapping},
    entry_point, BootInfo,
};
use core::panic::PanicInfo;
use shivacore::ats1000;
use shivacore::userspace;

// Bootloader anweisen, das gesamte physische RAM linear ins virtuelle
// Adressvolumen zu mappen (Voraussetzung fuer den Paging-Mapper in memory.rs).
pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial_println!("ShivaCore: Kernel-Einstiegspunkt erreicht.");

    let cpu = hal::CpuInfo::detect();
    serial_println!(
        "ShivaCore: CPU vendor={} family={} model={} stepping={} apic={} x2apic={} nx={} invariant_tsc={}",
        cpu.vendor_name(), cpu.family, cpu.model, cpu.stepping,
        cpu.features.apic, cpu.features.x2apic, cpu.features.nx, cpu.features.invariant_tsc
    );
    if !cpu.boot_compatible() {
        panic!("ShivaCore: CPU lacks required x86-64 boot features (SSE2/NX/APIC)");
    }
    serial_println!("ShivaCore: CPU HAL compatibility check OK.");

    let phys_mem_offset_raw = boot_info
        .physical_memory_offset
        .into_option()
        .expect("Bootloader hat physical_memory_offset nicht gesetzt (Config fehlt?)");
    let (validated_tables, madt) = validate_acpi_boot_tables(boot_info, phys_mem_offset_raw)
        .unwrap_or_else(|error| panic!("ShivaCore: ACPI validation failed: {}", error));
    serial_println!(
        "ShivaCore: ACPI RSDP/root/table checksums validated; tables={}",
        validated_tables
    );
    let ioapic_info = madt.and_then(|summary| summary.ioapic);
    let lapic_base = match madt {
        Some(summary) => {
            let ioapic = match summary.ioapic {
                Some(ioapic) => ioapic,
                None => shivacore::madt::IoApic {
                    id: 0xFF,
                    address: 0,
                    gsi_base: 0,
                },
            };
            serial_println!(
                "ShivaCore: MADT local_apic=0x{:X} ioapic=0x{:X} gsi_base={} cpus={}/{} overrides={} x2apic={} unknown={}",
                summary.local_apic_address,
                ioapic.address,
                ioapic.gsi_base,
                summary.processors_enabled,
                summary.processors,
                summary.overrides,
                summary.x2apic_processors,
                summary.unknown_entries
            );
            if summary.local_apic_address == 0 {
                shivacore::apic::LAPIC_DEFAULT_BASE
            } else {
                summary.local_apic_address
            }
        }
        None => {
            serial_println!("ShivaCore: WARNUNG -- keine MADT im ACPI-Baum gefunden.");
            shivacore::apic::LAPIC_DEFAULT_BASE
        }
    };
    let phys_mem_offset = x86_64::VirtAddr::new(phys_mem_offset_raw);

    if let Some(fb) = boot_info.framebuffer.as_mut() {
        framebuffer::init(fb);
        println!("ShivaCore Kernel v0.0.3 -- K-Sprint 2");
        println!("Boot: OK | Serial: OK | Framebuffer: OK");
    } else {
        serial_println!("ShivaCore: WARNUNG -- kein Framebuffer vom Bootloader erhalten.");
    }

    gdt::init();
    interrupts::init_idt();
    x86_64::instructions::interrupts::int3();
    interrupts::init_pics();
    serial_println!("ShivaCore: GDT/IDT/PIC OK (K-Sprint 1).");

    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_regions) };

    allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("Heap-Initialisierung fehlgeschlagen");
    serial_println!("ShivaCore: Paging-Mapper + Heap initialisiert (100 KiB).");

    // SC-006 REQ-03: Die LAPIC-Registerschicht an das MMIO-Fenster anbinden.
    // Ohne diesen Schritt blieb `apic.rs` folgenlos.
    let lapic_address = map_mmio_page(&mut mapper, &mut frame_allocator, lapic_base);
    let mut lapic = unsafe {
        shivacore::apic::LocalApic::new(shivacore::apic::MappedRegisters::new(
            lapic_address.as_u64() as *mut u32,
        ))
    };
    let spurious = lapic.enable();
    serial_println!(
        "ShivaCore: LAPIC aktiv (id={}, spurious=0x{:X}).",
        lapic.id(),
        spurious
    );
    let probe_vector = 0x40u32;
    lapic
        .mask(probe_vector)
        .expect("LAPIC-Maske fehlgeschlagen");
    let masked = lapic.is_masked(probe_vector).unwrap_or(false);
    lapic
        .unmask(probe_vector)
        .expect("LAPIC-Freigabe fehlgeschlagen");
    let released = !lapic.is_masked(probe_vector).unwrap_or(true);
    serial_println!(
        "ShivaCore: LAPIC-Maskentest Vektor 0x{:X}: maskiert={} freigegeben={}",
        probe_vector,
        masked,
        released
    );

    // SC-006 REQ-03: IOAPIC programmieren. Der Treiber liest die Eintragszahl
    // aus dem Versionsregister und beschreibt die Redirection-Tabelle.
    match ioapic_info {
        Some(info) if info.address != 0 => {
            serial_println!(
                "ShivaCore: IOAPIC-Mapping base=0x{:X} startet.",
                info.address
            );
            let ioapic_base =
                map_mmio_page(&mut mapper, &mut frame_allocator, u64::from(info.address));
            serial_println!(
                "ShivaCore: IOAPIC-Fenster bei 0x{:X}.",
                ioapic_base.as_u64()
            );
            let mut ioapic = shivacore::ioapic::IoApic::new(
                unsafe {
                    shivacore::ioapic::MappedRegisters::new(ioapic_base.as_u64() as *mut u32)
                },
                info.gsi_base,
            );
            serial_println!(
                "ShivaCore: IOAPIC id={} version=0x{:X} gsi_base={} entries={}",
                ioapic.id(),
                ioapic.version(),
                ioapic.gsi_base(),
                ioapic.max_entries()
            );

            // IRQ 0 (PIT) auf seine GSI abbilden: die MADT-Overrides entscheiden,
            // nicht die Annahme GSI == IRQ.
            let timer_gsi = match madt {
                Some(summary) => shivacore::madt::gsi_for_irq(&summary, 0),
                None => 0,
            };
            let timer_vector: u8 = 0x50;
            let entry = shivacore::ioapic::RedirectionEntry::fixed(timer_vector, lapic.id());
            match ioapic.set_redirection_entry(timer_gsi, entry) {
                Ok(read_back) => {
                    serial_println!(
                        "ShivaCore: IOAPIC GSI {} -> Vektor 0x{:X} programmiert (maskiert={}, Ruecklesung={})",
                        timer_gsi,
                        read_back.vector,
                        read_back.masked,
                        read_back.vector == timer_vector
                    );
                }
                Err(error) => {
                    serial_println!(
                        "ShivaCore: WARNUNG -- IOAPIC-Eintrag fuer GSI {} nicht schreibbar: {:?}",
                        timer_gsi,
                        error
                    );
                }
            }
            // Vor dem Umschalten messen, wie lange 20 zugestellte Ticks unter
            // der PIC dauern. Gleiche PIT-Frequenz (100 Hz), also ist der
            // TSC-Abstand direkt mit der IOAPIC-Phase vergleichbar.
            const PROBE_TICKS: u64 = 40;
            let tsc_per_second = calibrate_tsc(20);
            let pic_probe = measure_ticks(PROBE_TICKS);
            let pic_delta = pic_probe.map(|(tsc, _)| tsc);
            let pic_rate = tsc_per_second.and_then(|tsc| measure_tick_rate(PROBE_TICKS, tsc));

            // SC-006: Zustellung von der PIC auf den IOAPIC umschalten.
            // Reihenfolge ist bindend: erst die PIC verstummen lassen, dann den
            // Bestaetigungsweg umstellen, erst danach den Eintrag freigeben.
            unsafe {
                crate::interrupts::PICS.lock().write_masks(0xFF, 0xFF);
            }
            serial_println!("ShivaCore: PIC maskiert (0xFF/0xFF), nur noch ein Zustellweg.");
            shivacore::apic::switch_acknowledge_to_apic(lapic_base);
            serial_println!("ShivaCore: Bestaetigung laeuft ueber den Local APIC (EOI 0x0B0).");

            let mut armed = entry;
            armed.vector = crate::interrupts::TIMER_VECTOR;
            armed.masked = false;
            match ioapic.set_redirection_entry(timer_gsi, armed) {
                Ok(read_back) => {
                    serial_println!(
                        "ShivaCore: Zustellung auf IOAPIC umgestellt: GSI {} -> Vektor 0x{:X}, maskiert={}",
                        timer_gsi,
                        read_back.vector,
                        read_back.masked
                    );
                }
                Err(error) => {
                    serial_println!(
                        "ShivaCore: WARNUNG -- Freigabe fuer GSI {} fehlgeschlagen: {:?}",
                        timer_gsi,
                        error
                    );
                }
            }
            let (masks_before, unmasks_before) = shivacore::apic::mask_unmask_counts();
            let apic_probe = measure_ticks(PROBE_TICKS);
            let apic_delta = apic_probe.map(|(tsc, _)| tsc);
            // Ab hier ist der IOAPIC der Zustellweg: die Sperren und
            // Freigaben dieses Fensters zeigen, ob der Handler wirklich
            // maskiert und erst im ACK freigibt.
            let (masks_after, unmasks_after) = shivacore::apic::mask_unmask_counts();
            serial_println!(
                "ShivaCore: IRQ-Pfad: {} Ticks ueber IOAPIC, {} Sperren, {} Freigaben",
                masks_after.saturating_sub(masks_before),
                masks_after.saturating_sub(masks_before),
                unmasks_after.saturating_sub(unmasks_before)
            );
            let apic_rate = tsc_per_second.and_then(|tsc| measure_tick_rate(PROBE_TICKS, tsc));
            match (pic_delta, apic_delta) {
                (Some(pic), Some(apic)) if apic > 0 => {
                    serial_println!(
                        "ShivaCore: Tick-Messung {} Ticks: PIC={} TSC, IOAPIC={} TSC, Verhaeltnis={}.{}x",
                        PROBE_TICKS,
                        pic,
                        apic,
                        pic / apic,
                        (pic % apic) * 10 / apic
                    );
                }
                _ => {
                    serial_println!("ShivaCore: WARNUNG -- Tick-Messung ohne Ergebnis.");
                }
            }
            // Beide Zaehler werden an derselben Flanke gelesen, deshalb ist der
            // Vergleich exakt und nicht um einen Tick verschoben.
            match apic_probe {
                Some((_, steps)) => {
                    let drift = i128::from(steps) - i128::from(PROBE_TICKS);
                    serial_println!(
                        "ShivaCore: Zeitquelle: {} Ticks zugestellt, {} MonotonicTime-Schritte (Drift {:+})",
                        PROBE_TICKS,
                        steps,
                        drift
                    );
                }
                None => {
                    serial_println!("ShivaCore: WARNUNG -- Zeitquellenmessung ohne Ergebnis.");
                }
            }
            match (pic_rate, apic_rate) {
                (Some(pic), Some(apic)) => {
                    serial_println!(
                        "ShivaCore: Tick-Rate (PIT soll 100 Hz): PIC={} Hz, IOAPIC={} Hz",
                        pic,
                        apic
                    );
                }
                _ => {
                    serial_println!("ShivaCore: WARNUNG -- PIT-Kalibrierung fehlgeschlagen.");
                }
            }
        }
        _ => {
            serial_println!("ShivaCore: WARNUNG -- kein IOAPIC in der MADT gemeldet.");
        }
    }

    // Heap live testen: Box + Vec muessen funktionieren, ohne zu crashen.
    let boxed = Box::new(41);
    serial_println!("ShivaCore: Box-Test -- Wert: {}", *boxed);

    let mut vec = Vec::new();
    for i in 0..10 {
        vec.push(i);
    }
    serial_println!(
        "ShivaCore: Vec-Test -- Summe 0..10: {}",
        vec.iter().sum::<i32>()
    );

    println!("K-Sprint 2: Paging/Heap OK (Box+Vec getestet)");
    // Real kernel integration gate: execute the canonical kernel initialization
    // from the same UEFI -> kernel_main path after paging/heap are live.
    let kernel_state = shivacore::kernel_init::KernelState::boot()
        .expect("ShivaCore: canonical KernelState::boot() failed");
    serial_println!("ShivaCore: KernelState::boot() -> Done.");

    // Real USER-001 smoke path: install executable user pages and enter CPL3
    // through IRETQ. The payload loops forever so timer IRQs can observe a
    // genuine ring-3 CPU context without executing privileged instructions.
    let user_binary = userspace::UserBinary::from_bytes(
        "ring3-smoke",
        alloc::vec![
            0x48, 0xB8, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // RAX=0x1000
            0x48, 0xBB, 0x01, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // RBX=0x1001
            0x49, 0xBC, 0x12, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // R12=0x1012
            0xEB, 0xFE, // loop forever until timer preemption
        ],
        0x0040_0000,
    );
    let user_addr_space = userspace::UserAddressSpace::default();
    unsafe {
        userspace::map_user_binary(
            &mut mapper,
            &mut frame_allocator,
            phys_mem_offset,
            &user_binary,
            &user_addr_space,
        )
        .expect("ShivaCore: USER-001 user page mapping failed");
    }
    let user_pid = ats1000::Pid(1000);
    let user_ctx = userspace::UserContext::new(user_pid, &user_binary, user_addr_space);
    serial_println!(
        "ShivaCore: USER-001 mapped PID={} RIP={:#x} RSP={:#x}",
        user_pid.0,
        user_ctx.rip,
        user_ctx.rsp
    );

    // Second real Ring-3 image. Both processes intentionally share the current
    // kernel page table for this first scheduler gate; the scheduler switches
    // the CPU IRET frame while CR3/address-space switching remains a later gate.
    let user_binary_2 = userspace::UserBinary::from_bytes(
        "ring3-smoke-2",
        alloc::vec![
            0x48, 0xB8, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // RAX=0x2000
            0x48, 0xBB, 0x01, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // RBX=0x2001
            0x49, 0xBC, 0x12, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // R12=0x2012
            0xEB, 0xFE, // loop forever until timer preemption
        ],
        0x0040_1000,
    );
    // Keep the first scheduler gate on the current shared CR3, but allocate
    // a disjoint virtual region for PID 1001. This prevents the second mapping
    // from colliding with PID 1000's code/data/stack pages. Full CR3 switching
    // remains the next isolation gate.
    let user_addr_space_2 = userspace::UserAddressSpace {
        code_base: 0x0140_0000,
        code_size: 0x0010_0000,
        data_base: 0x0150_0000,
        data_size: 0x0010_0000,
        stack_base: 0x07FF_F000,
        stack_size: 0x0001_0000,
        heap_base: 0x0160_0000,
        heap_size: 0x0020_0000,
    };
    let user_binary_2 = userspace::UserBinary {
        entry_point: 0x0140_0000,
        ..user_binary_2
    };
    unsafe {
        userspace::map_user_binary(
            &mut mapper,
            &mut frame_allocator,
            phys_mem_offset,
            &user_binary_2,
            &user_addr_space_2,
        )
        .expect("ShivaCore: second USER-001 user page mapping failed");
    }
    let user_pid_2 = ats1000::Pid(1001);
    let user_ctx_2 = userspace::UserContext::new(user_pid_2, &user_binary_2, user_addr_space_2);

    interrupts::init_user_scheduler(&user_ctx, &user_ctx_2);
    serial_println!(
        "ShivaCore: USER-001 entering PID={} with timer preemption target PID={}",
        user_pid.0,
        user_pid_2.0
    );
    unsafe { userspace::enter_ring3(&user_ctx) }

    serial_println!("{}", kernel_state.boot_log());
    serial_println!("ShivaCore: kernel init chain connected to kernel_main.");

    serial_println!("ShivaCore: K-Sprint 2 abgeschlossen. Uebergabe an Idle-Loop.");

    loop {
        x86_64::instructions::hlt();
    }
}

/// Validate firmware-provided ACPI data before any ACPI address is used by a driver.
/// Every read is bounded by a physical-memory region reported in BootInfo; the
/// accepted table size is capped to avoid unbounded firmware-controlled reads.
fn validate_acpi_boot_tables(
    boot_info: &BootInfo,
    physical_memory_offset: u64,
) -> Result<(usize, Option<shivacore::madt::MadtSummary>), &'static str> {
    const RSDP_V1_LEN: usize = 20;
    const RSDP_V2_MIN_LEN: usize = 36;
    const MAX_RSDP_LEN: usize = 4096;
    const SDT_HEADER_LEN: usize = 36;
    const MAX_SDT_LEN: usize = 1024 * 1024;

    let rsdp_address = boot_info
        .rsdp_addr
        .into_option()
        .ok_or("bootloader did not provide an RSDP address")?;
    let rsdp_v1 = read_physical(boot_info, physical_memory_offset, rsdp_address, RSDP_V1_LEN)?;
    if &rsdp_v1[0..8] != b"RSD PTR " {
        return Err("invalid RSDP signature");
    }
    if !acpi_checksum_valid(rsdp_v1) {
        return Err("invalid RSDP v1 checksum");
    }

    let revision = rsdp_v1[15];
    let (root_address, root_signature) = if revision >= 2 {
        let prefix = read_physical(
            boot_info,
            physical_memory_offset,
            rsdp_address,
            RSDP_V2_MIN_LEN,
        )?;
        let rsdp_length =
            u32::from_le_bytes([prefix[20], prefix[21], prefix[22], prefix[23]]) as usize;
        if !(RSDP_V2_MIN_LEN..=MAX_RSDP_LEN).contains(&rsdp_length) {
            return Err("RSDP extended length is outside supported bounds");
        }
        let extended = read_physical(boot_info, physical_memory_offset, rsdp_address, rsdp_length)?;
        if !acpi_checksum_valid(extended) {
            return Err("invalid RSDP extended checksum");
        }
        let xsdt = u64::from_le_bytes([
            extended[24],
            extended[25],
            extended[26],
            extended[27],
            extended[28],
            extended[29],
            extended[30],
            extended[31],
        ]);
        if xsdt != 0 {
            (xsdt, b"XSDT")
        } else {
            let rsdt =
                u32::from_le_bytes([extended[16], extended[17], extended[18], extended[19]]) as u64;
            (rsdt, b"RSDT")
        }
    } else {
        let rsdt = u32::from_le_bytes([rsdp_v1[16], rsdp_v1[17], rsdp_v1[18], rsdp_v1[19]]) as u64;
        (rsdt, b"RSDT")
    };

    if root_address == 0 {
        return Err("RSDP contains a null root-table address");
    }
    let root_header = read_physical(
        boot_info,
        physical_memory_offset,
        root_address,
        SDT_HEADER_LEN,
    )?;
    if &root_header[0..4] != root_signature {
        return Err("root-table signature does not match RSDP revision");
    }
    let root_length = sdt_length(root_header, MAX_SDT_LEN)?;
    let root_table = read_physical(boot_info, physical_memory_offset, root_address, root_length)?;
    if !acpi_checksum_valid(root_table) {
        return Err("root-table checksum invalid");
    }

    let entry_size = if root_signature == b"XSDT" { 8 } else { 4 };
    if (root_length - SDT_HEADER_LEN) % entry_size != 0 {
        return Err("root-table entries are misaligned");
    }

    let entry_count = (root_length - SDT_HEADER_LEN) / entry_size;
    if entry_count == 0 {
        return Err("root table contains no ACPI table entries");
    }
    let mut validated = 1usize; // RSDT/XSDT
    let mut madt = None;
    for entry in root_table[SDT_HEADER_LEN..root_length].chunks_exact(entry_size) {
        let address = if entry_size == 8 {
            u64::from_le_bytes([
                entry[0], entry[1], entry[2], entry[3], entry[4], entry[5], entry[6], entry[7],
            ])
        } else {
            u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]) as u64
        };
        if address == 0 {
            return Err("root table contains a null child-table address");
        }
        let header = read_physical(boot_info, physical_memory_offset, address, SDT_HEADER_LEN)?;
        let length = sdt_length(header, MAX_SDT_LEN)?;
        let table = read_physical(boot_info, physical_memory_offset, address, length)?;
        if !acpi_checksum_valid(table) {
            return Err("referenced ACPI table checksum invalid");
        }
        if table.len() >= 4 && table[0..4] == shivacore::madt::MADT_SIGNATURE {
            let summary =
                shivacore::madt::summarize(table).map_err(|_| "MADT entries are malformed")?;
            madt = Some(summary);
        }
        validated = validated
            .checked_add(1)
            .ok_or("ACPI table count overflow")?;
    }
    Ok((validated, madt))
}

fn sdt_length(header: &[u8], maximum: usize) -> Result<usize, &'static str> {
    if header.len() < 8 {
        return Err("truncated ACPI table header");
    }
    let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if !(36..=maximum).contains(&length) {
        return Err("ACPI table length is outside supported bounds");
    }
    Ok(length)
}

fn acpi_checksum_valid(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) == 0
}

fn read_physical(
    boot_info: &BootInfo,
    physical_memory_offset: u64,
    physical_address: u64,
    length: usize,
) -> Result<&'static [u8], &'static str> {
    let end = physical_address
        .checked_add(length as u64)
        .ok_or("physical ACPI address overflow")?;
    if !boot_info
        .memory_regions
        .iter()
        .any(|region| region.start <= physical_address && end <= region.end)
    {
        return Err("ACPI bytes are outside a single reported physical-memory region");
    }
    let virtual_address = physical_memory_offset
        .checked_add(physical_address)
        .ok_or("ACPI virtual address overflow")?;
    let virtual_address = x86_64::VirtAddr::try_new(virtual_address)
        .map_err(|_| "ACPI virtual address is non-canonical")?;
    // SAFETY: the bootloader maps physical memory at physical_memory_offset.
    // The complete read range is checked against one firmware memory-map region
    // and all lengths are bounded by the RSDP/SDT limits above.
    Ok(unsafe { core::slice::from_raw_parts(virtual_address.as_ptr(), length) })
}

#[alloc_error_handler]
fn alloc_error_handler(layout: core::alloc::Layout) -> ! {
    panic!("Allokation fehlgeschlagen: {:?}", layout)
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("ShivaCore: KERNEL PANIC -- {}", info);
    loop {
        x86_64::instructions::hlt();
    }
}

/// Mappt ein MMIO-Fenster (LAPIC oder IOAPIC) als Geraetespeicher.
/// Eine 4-KiB-Seite deckt LAPIC (bis 0x3F0) wie IOAPIC (bis 0x10) ab.
/// Liest den PIT-Kanal 0 (latch, dann LSB/MSB). Der Zaehler laeuft rueckwaerts
/// und wird bei 0 mit dem Divisor neu geladen.
fn pit_read_counter() -> u16 {
    use x86_64::instructions::port::Port;
    unsafe {
        let mut command: Port<u8> = Port::new(0x43);
        let mut channel0: Port<u8> = Port::new(0x40);
        command.write(0x00); // Kanal 0 festhalten
        let low = channel0.read() as u16;
        let high = channel0.read() as u16;
        (high << 8) | low
    }
}

/// Kalibriert den TSC gegen den PIT. Der PIT laeuft mit der in `init_pics`
/// programmierten Frequenz; damit wird der TSC zu einer absoluten Zeitbasis,
/// die unabhaengig von der Interrupt-Zustellung ist.
fn calibrate_tsc(periods: u32) -> Option<u64> {
    const PIT_HZ: u64 = 100;
    let mut previous = pit_read_counter();
    let start = unsafe { core::arch::x86_64::_rdtsc() };
    let mut seen = 0u32;
    let mut guard: u64 = 0;
    while seen < periods {
        let current = pit_read_counter();
        // Rueckwaertslaufender Zaehler: ein Sprung nach oben ist ein Neuladen.
        if current > previous && current - previous > 1000 {
            seen += 1;
        }
        previous = current;
        guard += 1;
        if guard > 200_000_000 {
            return None;
        }
    }
    let end = unsafe { core::arch::x86_64::_rdtsc() };
    let elapsed = end.saturating_sub(start);
    let periods = u64::from(periods);
    // TSC pro Sekunde = elapsed / (periods / PIT_HZ)
    Some(elapsed.saturating_mul(PIT_HZ) / periods)
}

/// Misst, wie viele Ticks pro Sekunde tatsaechlich zugestellt werden.
/// `tsc_per_second` stammt aus der PIT-Kalibrierung.
fn measure_tick_rate(target: u64, tsc_per_second: u64) -> Option<u64> {
    let (delta, _) = measure_ticks(target)?;
    if delta == 0 {
        return None;
    }
    Some(target.saturating_mul(tsc_per_second) / delta)
}

/// Misst den TSC-Abstand fuer eine feste Zahl zugestellter Timer-Ticks.
/// Ohne bekannte TSC-Frequenz ist nur der Vergleich zweier Wege aussagekraeftig.
fn measure_ticks(target: u64) -> Option<(u64, u64)> {
    use core::sync::atomic::Ordering;

    // Erst an einer Tick-Flanke ausrichten. Ohne diesen Schritt enthaelt das
    // gemessene Intervall bis zu eine volle Periode Wartezeit auf den ersten
    // Tick und die Rate wird systematisch zu niedrig geschaetzt (bei 20 Ticks
    // bis zu 5 Prozent).
    let mut seen = crate::interrupts::TIMER_TICKS.load(Ordering::Acquire);
    let mut guard: u64 = 0;
    while crate::interrupts::TIMER_TICKS.load(Ordering::Acquire) == seen {
        guard += 1;
        if guard > 50_000_000_000 {
            return None;
        }
        core::hint::spin_loop();
    }
    seen = crate::interrupts::TIMER_TICKS.load(Ordering::Acquire);
    let mono_start = shivacore::syscall::monotonic_ticks();
    let start = unsafe { core::arch::x86_64::_rdtsc() };
    while crate::interrupts::TIMER_TICKS.load(Ordering::Acquire) < seen + target {
        guard += 1;
        if guard > 50_000_000_000 {
            return None;
        }
        core::hint::spin_loop();
    }
    let end = unsafe { core::arch::x86_64::_rdtsc() };
    let mono_end = shivacore::syscall::monotonic_ticks();
    Some((end.saturating_sub(start), mono_end.wrapping_sub(mono_start)))
}

fn map_mmio_page(
    mapper: &mut x86_64::structures::paging::mapper::OffsetPageTable,
    frame_allocator: &mut crate::memory::BootInfoFrameAllocator,
    base: u64,
) -> x86_64::VirtAddr {
    use x86_64::structures::paging::{
        mapper::MapToError, page::Page, FrameAllocator, Mapper, PageTableFlags, PhysFrame, Size4KiB,
    };
    use x86_64::{PhysAddr, VirtAddr};

    let virt = VirtAddr::new(base);
    let page = Page::<Size4KiB>::containing_address(virt);
    let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(base));
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
    match unsafe { mapper.map_to(page, frame, flags, frame_allocator) } {
        Ok(flush) => flush.flush(),
        // Der Bootloader kann die Seite bereits identity-gemappt haben.
        Err(MapToError::PageAlreadyMapped(_)) => {}
        Err(error) => panic!("MMIO-Mapping bei 0x{:X} fehlgeschlagen: {:?}", base, error),
    }
    virt
}
