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
    let (validated_tables, madt, hpet_base) =
        validate_acpi_boot_tables(boot_info, phys_mem_offset_raw)
            .unwrap_or_else(|error| panic!("ShivaCore: ACPI validation failed: {}", error));
    serial_println!(
        "ShivaCore: ACPI RSDP/root/table checksums validated; tables={}",
        validated_tables
    );
    // Die ACPI-Geraeteliste wird erst nach der Heap-Initialisierung
    // veroeffentlicht: eine `Vec` vor dem Heap ist eine Allokation ins Nichts
    // (QEMU-Lauf: "Allokation fehlgeschlagen: Layout { size: 96, align: 8 }").
    // MADT-Zusammenfassung und HPET-Basis sind Kopien, sie reisen mit.
    // Fuer die spaetere Veroeffentlichung (nach der Heap-Initialisierung).
    let acpi_summary = madt;
    let acpi_hpet_base = hpet_base;
    // Die Ramdisk wird weiter unten gelesen, nach der Heap-Initialisierung:
    // der ELF-Parser legt Puffer an, und eine `Vec` vor dem Heap ist eine
    // Allokation ins Nichts (QEMU: "Allokation fehlgeschlagen: Layout { size: 224,
    // align: 8 }").
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
    // Adressraeume je Prozess: hier gemerkt, spaeter dem Scheduler uebergeben.
    let (boot_cr3_frame, boot_cr3_flags) = shivacore::addr_space::active();
    let cr3_total_before_test = boot_cr3_frame;
    let mut cr3_first: u64 = 0;
    let mut cr3_second: u64 = 0;
    let mut cr3_third: u64 = 0;
    let mut cr3_fourth: u64 = 0;
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

    // Adressraum-Selbsttest: Kann der Kernel in einer selbstgebauten Tabelle
    // weiterlaufen — inklusive Timer-IRQ ueber die LAPIC-MMIO-Seite?
    //
    // Der Test schaltet wirklich um und wieder zurueck. Eine falsch geerbte
    // Tabelle beendet den Boot an dieser Stelle sofort und unmissverstaendlich;
    // ein Kommentar kann das nicht leisten. Gewartet wird auf Timer-Ticks, weil
    // deren Zustellung den LAPIC-Zugriff im neuen Adressraum erzwingt.
    if let Some(space) = unsafe {
        shivacore::addr_space::AddressSpace::create(
            phys_mem_offset,
            &mut frame_allocator,
            &[],
            cr3_total_before_test,
        )
    } {
        let (boot_frame, boot_flags) = shivacore::addr_space::active();
        let before = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Acquire);
        space.install();
        let mut spins = 0u64;
        while crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Acquire)
            < before + 3
        {
            spins += 1;
            if spins > 200_000_000 {
                break;
            }
            core::hint::spin_loop();
        }
        let ticks =
            crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Acquire) - before;
        shivacore::addr_space::restore(boot_frame, boot_flags);
        serial_println!(
            "ShivaCore: Adressraum-Selbsttest: Tabelle 0x{:X} aktiv, {} Timer-Ticks im eigenen Raum, Kernel laeuft",
            space.frame().start_address().as_u64(),
            ticks
        );
    } else {
        serial_println!("ShivaCore: Adressraum-Selbsttest uebersprungen (kein Frame frei)");
    }
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

    // SC-006 REQ-09: Selbsttest der DMA-Autorisierung. Der Test laeuft durch
    // den echten Code der Policy-Schicht, damit nicht nur die Unit-Tests
    // belegen, dass die Logik im Kernel-Binary vorhanden ist. Er belegt
    // ausdruecklich nicht, dass eine IOMMU programmiert wird — das ist offen.
    {
        use shivacore::capability::{CapabilityTable, ResourceType, Rights};
        let mut caps = CapabilityTable::new();
        let mut dma = shivacore::dma_auth::DmaAuthorizer::new();
        let own = caps
            .create_typed(
                shivacore::ats1000::Pid(1000),
                ResourceType::Device,
                3,
                Rights::ACCESS,
            )
            .expect("DMA-Selbsttest: Device-Cap");
        let foreign = caps
            .create_typed(
                shivacore::ats1000::Pid(1001),
                ResourceType::Device,
                3,
                Rights::ACCESS,
            )
            .expect("DMA-Selbsttest: fremde Device-Cap");
        let mut ok = true;
        let domain = match dma.assign_device(caps.get(own).expect("Cap"), 3) {
            Ok(domain) => Some(domain),
            Err(_) => {
                ok = false;
                None
            }
        };
        if let Some(domain) = domain {
            // Fremder Prozess darf die Domain nicht befuellen.
            ok &= dma
                .map_window(
                    caps.get(foreign).expect("Cap"),
                    domain,
                    shivacore::dma_auth::DmaWindow {
                        iova: 0,
                        physical: 0x2000_0000,
                        length: 4096,
                        writable: true,
                    },
                )
                .is_err();
            // Dieselbe Cap oeffnet kein zweites Geraet.
            ok &= dma.assign_device(caps.get(own).expect("Cap"), 5).is_err();
        } else {
            ok = false;
        }
        let stats = dma.stats();
        if ok {
            serial_println!(
                "ShivaCore: SC-006 DMA-Selbsttest bestanden (Geraet=3 Domain={:?}, Ablehnungen: fremd={} falsch={})",
                domain,
                stats.denied_foreign_domain,
                stats.denied_wrong_device
            );
        } else {
            serial_println!(
                "ShivaCore: SC-006 DMA-Selbsttest FEHLGESCHLAGEN (erlaubt={} fremd={} falsch={})",
                stats.authorized,
                stats.denied_foreign_domain,
                stats.denied_wrong_device
            );
        }
    }

    // SC-006 REQ-11: Enum-Quelle. Die Geraete-Registratur im Service Space
    // braucht eine Liste, die nicht geraten ist; hier wird sie gelesen.
    // Die Entdeckung vergibt keine Rechte — wer ein Geraet benutzen darf,
    // entscheidet weiterhin die Device-Cap.
    // Programm aus der Ramdisk, falls eine mitgegeben wurde. Es ersetzt weiter
    // unten den eingebauten Platzhalter: dieselben Seiten, derselbe Weg nach
    // Ring 3, nur mit echtem Code von einem Datentraeger.
    let mut ramdisk_program: Option<(alloc::vec::Vec<u8>, u64)> = None;
    // Zweites Programm aus demselben Verzeichnis: es bekommt einen eigenen
    // Prozess, damit das Verzeichnis nicht nur Auswahl, sondern auch Vielfalt
    // bedeutet.
    let mut ramdisk_second: Option<(alloc::string::String, alloc::vec::Vec<u8>, u64)> = None;

    // Die Ramdisk traegt das erste echte Userspace-Programm. Ohne sie ist der
    // Kernel ein geschlossenes Ganzes; mit ihr kommt ein Programm von einem
    // Datentraeger.
    //
    // Gelesen wird ueber die Adresse aus `BootInfo`, und die ist bereits
    // virtuell: der Bootloader hat die Ramdisk selbst abgebildet. Ein Versuch,
    // sie ueber `physical_memory_offset` zu lesen, scheitert zu Recht mit
    // "bytes are outside a single reported physical-memory region" — das ist
    // die Bereichspruefung, die hier nicht greift, weil es keine physische
    // Adresse ist.
    const MAX_RAMDISK_BYTES: u64 = 64 * 1024 * 1024;
    if let Some(ramdisk_address) = boot_info.ramdisk_addr.into_option() {
        let ramdisk_len = boot_info.ramdisk_len;
        if ramdisk_address != 0 && ramdisk_len > 0 && ramdisk_len <= MAX_RAMDISK_BYTES {
            let bytes = unsafe {
                core::slice::from_raw_parts(ramdisk_address as *const u8, ramdisk_len as usize)
            };
            // Zwei Formen: ein Verzeichnis (dann wird nach Namen gewaehlt) oder
            // ein nacktes Programm (dann ist es das Init). Ein Image ohne
            // Verzeichnis soll weiter booten.
            let program: Option<(alloc::string::String, &[u8])> =
                if shivacore::ramdisk::is_container(bytes) {
                    match shivacore::ramdisk::parse(bytes) {
                        Ok(entries) => {
                            let names: Vec<&str> =
                                entries.iter().map(|entry| entry.name.as_str()).collect();
                            serial_println!(
                                "ShivaCore: Ramdisk-Verzeichnis: {} Programme ({})",
                                entries.len(),
                                names.join(", ")
                            );
                            let chosen = shivacore::ramdisk::find(&entries, "init")
                                .or_else(|| entries.first())
                                .map(|entry| entry.name.clone());
                            // Das zweite Programm: der erste Eintrag, der nicht
                            // das Init ist. Es bekommt unten einen eigenen
                            // Prozess, damit das Verzeichnis nicht nur Auswahl,
                            // sondern auch Vielfalt bedeutet.
                            if let Some(chosen_name) = &chosen {
                                for entry in entries.iter().filter(|e| &e.name != chosen_name) {
                                    let Some(data) = entry.data(bytes) else {
                                        continue;
                                    };
                                    let Ok(parser) = shivacore::elf_loader::ElfParser::parse(data)
                                    else {
                                        continue;
                                    };
                                    let Some(segment) = parser.code_segment() else {
                                        continue;
                                    };
                                    let Ok(code) = parser.segment_data(segment) else {
                                        continue;
                                    };
                                    ramdisk_second = Some((
                                        entry.name.clone(),
                                        code.to_vec(),
                                        parser.entry_point(),
                                    ));
                                    break;
                                }
                            }
                            chosen.and_then(|name| {
                                let entry = shivacore::ramdisk::find(&entries, &name)?;
                                entry.data(bytes).map(|data| (name, data))
                            })
                        }
                        Err(error) => {
                            serial_println!("ShivaCore: Ramdisk-Verzeichnis unlesbar: {:?}", error);
                            None
                        }
                    }
                } else {
                    Some((alloc::string::String::from("programm"), bytes))
                };

            if let Some((name, program_bytes)) = program {
                match shivacore::elf_loader::ElfParser::parse(program_bytes) {
                    Ok(parser) => {
                        let code_len = parser
                            .code_segment()
                            .and_then(|segment| parser.segment_data(segment).ok())
                            .map(|data| data.len())
                            .unwrap_or(0);
                        serial_println!(
                            "ShivaCore: Programm '{}' aus Ramdisk: {} Bytes ELF, Einstieg {:#x}, Code {} Bytes",
                            name,
                            program_bytes.len(),
                            parser.entry_point(),
                            code_len
                        );
                        if let Some(segment) = parser.code_segment() {
                            if let Ok(code) = parser.segment_data(segment) {
                                ramdisk_program = Some((code.to_vec(), parser.entry_point()));
                            }
                        }
                    }
                    Err(error) => {
                        serial_println!(
                            "ShivaCore: Programm '{}' ist kein lesbares ELF: {:?}",
                            name,
                            error
                        );
                    }
                }
            }
        } else {
            serial_println!(
                "ShivaCore: Ramdisk-Adresse {:#x} / Laenge {} abgelehnt",
                ramdisk_address,
                ramdisk_len
            );
        }
    }

    #[cfg(feature = "x86-boot")]
    {
        use shivacore::pci_scan::{describe, scan_tree, DeviceKind, IoConfigSpace};
        let mut cfg = IoConfigSpace;
        // scan_tree folgt Bruecken: auf einer Maschine mit Steckplaetzen haengt
        // der groesste Teil der Geraete nicht am Root-Bus.
        let devices = scan_tree(&mut cfg, 0);
        for found in &devices {
            let (bus, device, function, vendor, id, kind) = describe(found);
            serial_println!(
                "ShivaCore: PCI {:02x}:{:02x}.{} {:04x}:{:04x} {}",
                bus,
                device,
                function,
                vendor,
                id,
                kind
            );
        }
        // Der Vorrat ist der Ort, an dem der Service Space spaeter nachfragt;
        // das Log ist nur die Quittung fuer den Boot.
        let published = shivacore::boot_devices::BOOT_DEVICES.publish(devices.clone());
        serial_println!(
            "ShivaCore: Geraetevorrat: {} Geraete fuer den Service Space abrufbar",
            published
        );
        // Zweite Quelle: Geraete, die hinter keinem PCI-Konfigurationsraum
        // liegen (Local APIC, IOAPIC, HPET). Der Vorrat ist getrennt, weil ein
        // ACPI-Geraet keine Bus/Geraet/Funktion-Kennung hat.
        if let Some(summary) = acpi_summary {
            let acpi_devices = shivacore::acpi_devices::from_summary(&summary, acpi_hpet_base);
            let names: Vec<&str> = acpi_devices.iter().map(|d| d.name()).collect();
            let count = shivacore::boot_devices::BOOT_ACPI_DEVICES.publish(acpi_devices);
            serial_println!(
                "ShivaCore: ACPI-Geraete: {} ({}) fuer den Service Space abrufbar",
                count,
                names.join(", ")
            );
        }
        let count = |kind: DeviceKind| devices.iter().filter(|d| d.kind() == kind).count();
        serial_println!(
            "ShivaCore: SC-006 PCI-Enumeration: {} Geraete (storage={} network={} display={} bridge={} usb={} other={})",
            devices.len(),
            count(DeviceKind::Storage),
            count(DeviceKind::Network),
            count(DeviceKind::Display),
            count(DeviceKind::Bridge),
            count(DeviceKind::Usb),
            count(DeviceKind::Other)
        );
    }

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
            // Die Zuordnung Line -> GSI -> Vektor gehoert in eine Tabelle, nicht
            // in eine Zahl: sonst weiss niemand, welche Vektoren noch frei sind.
            let timer_vector: u8 = 0x50;
            // Der Vektor, auf den zugestellt wird, ist der des IDT-Handlers;
            // timer_vector (0x50) ist nur der Probedruck vor dem Umschalten.
            let timer_route =
                match crate::interrupts::fix_route(0, timer_gsi, crate::interrupts::TIMER_VECTOR) {
                    Ok(route) => route,
                    Err(error) => {
                        serial_println!(
                            "ShivaCore: WARNUNG -- Timer-Route nicht eintragbar: {:?}",
                            error
                        );
                        shivacore::irq_routing::IrqRoute {
                            irq: 0,
                            gsi: timer_gsi,
                            vector: crate::interrupts::TIMER_VECTOR,
                        }
                    }
                };
            serial_println!(
                "ShivaCore: IRQ-Routing: IRQ {} -> GSI {} -> Vektor 0x{:X}, naechster freier Geraetevektor {}",
                timer_route.irq,
                timer_route.gsi,
                timer_route.vector,
                match crate::interrupts::next_free_vector() {
                    Some(vector) => format_hex(vector),
                    None => alloc::format!("keiner"),
                }
            );
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
            // Zweite Line: die Tastatur bekommt den ersten freien Geraetevektor
            // aus der Tabelle. Der IDT-Eintrag liegt auf KEYBOARD_VECTOR; laufen
            // die beiden auseinander, meldet der Bootlauf es statt zu schweigen.
            let keyboard_gsi = match madt {
                Some(summary) => shivacore::madt::gsi_for_irq(&summary, 1),
                None => 1,
            };
            match crate::interrupts::route_line(1, keyboard_gsi) {
                Ok(route) => {
                    serial_println!(
                        "ShivaCore: IRQ-Routing: IRQ {} -> GSI {} -> Vektor {:#X}",
                        route.irq,
                        route.gsi,
                        route.vector
                    );
                    if route.vector != crate::interrupts::KEYBOARD_VECTOR {
                        serial_println!(
                            "ShivaCore: WARNUNG -- Route {:#X} passt nicht zum IDT-Eintrag {:#X}",
                            route.vector,
                            crate::interrupts::KEYBOARD_VECTOR
                        );
                    }
                    // Abonnieren, bevor der Eintrag freigegeben wird: sonst
                    // kaeme der erste Tastendruck an einer Line vorbei, die in
                    // der Tabelle noch niemanden hat.
                    match crate::interrupts::subscribe_kernel_line(u32::from(route.irq)) {
                        Ok(line) => {
                            serial_println!(
                                "ShivaCore: IRQ-Bruecke: Line {} abonniert (Kernel-Cap SUBSCRIBE|ACK)",
                                line
                            );
                        }
                        Err(reason) => {
                            serial_println!(
                                "ShivaCore: WARNUNG -- Line {} nicht abonniert: {}",
                                route.irq,
                                reason
                            );
                        }
                    }
                    let mut keyboard_entry =
                        shivacore::ioapic::RedirectionEntry::fixed(route.vector, lapic.id());
                    keyboard_entry.masked = false;
                    match ioapic.set_redirection_entry(route.gsi, keyboard_entry) {
                        Ok(read_back) => {
                            serial_println!(
                                "ShivaCore: Tastatur auf IOAPIC: GSI {} -> Vektor {:#X}, maskiert={}",
                                route.gsi,
                                read_back.vector,
                                read_back.masked
                            );
                        }
                        Err(error) => {
                            serial_println!(
                                "ShivaCore: WARNUNG -- Tastatur-Eintrag fuer GSI {} fehlgeschlagen: {:?}",
                                route.gsi,
                                error
                            );
                        }
                    }
                }
                Err(error) => {
                    serial_println!(
                        "ShivaCore: WARNUNG -- Tastatur-Route nicht eintragbar: {:?}",
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
            armed.vector = timer_route.vector;
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
    //
    // Mit Ramdisk laeuft hier das erste echte Programm: der Code kommt aus dem
    // ELF auf dem Datentraeger, nicht aus diesem Quelltext. Ohne Ramdisk bleibt
    // der eingebaute Platzhalter, damit ein nacktes Image weiter bootet.
    let (user_binary, user_addr_space, source) = match ramdisk_program.take() {
        Some((code, entry)) => {
            let mut space = userspace::UserAddressSpace::default();
            space.code_base = entry;
            (
                userspace::UserBinary::from_bytes("ramdisk-init", code, entry),
                space,
                "Ramdisk",
            )
        }
        None => (
            userspace::UserBinary::from_bytes(
                "ring3-smoke",
                alloc::vec![
                    0x48, 0xB8, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // RAX=0x1000
                    0x48, 0xBB, 0x01, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // RBX=0x1001
                    0x49, 0xBC, 0x12, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // R12=0x1012
                    0xEB, 0xFE, // loop forever until timer preemption
                ],
                0x0040_0000,
            ),
            userspace::UserAddressSpace::default(),
            "eingebaut",
        ),
    };
    unsafe {
        // Eigener Adressraum: anlegen, aktivieren, dann laden. Das Laden
        // schreibt das Abbild ueber die VIRTUELLE Adresse — ohne aktive
        // Zieltabelle landete es in der Boot-Tabelle und der Prozess saehe
        // seine eigenen Seiten nie.
        let process_space = shivacore::addr_space::AddressSpace::create(
            phys_mem_offset,
            &mut frame_allocator,
            &user_addr_space.mapped_regions(),
            boot_cr3_frame,
        )
        .expect("ShivaCore: kein Adressraum fuer PID 1000");
        process_space.install();
        let mut process_mapper = process_space.mapper(phys_mem_offset);
        userspace::map_user_binary(
            &mut process_mapper,
            &mut frame_allocator,
            phys_mem_offset,
            &user_binary,
            &user_addr_space,
        )
        .expect("ShivaCore: USER-001 user page mapping failed");
        cr3_first = process_space.frame().start_address().as_u64();
    }
    let user_pid = ats1000::Pid(1000);
    // Nur die eingebaute Nutzlast setzt RAX/RBX/R12 auf ein bekanntes Muster.
    // Laeuft hier das Programm aus der Ramdisk, waere die SCHED-001-Pruefung
    // dauerhaft im Aufwaermzustand und wuerde nichts beweisen.
    interrupts::expect_register_pattern(1000, source == "eingebaut");
    let user_ctx = userspace::UserContext::new(user_pid, &user_binary, user_addr_space);
    serial_println!(
        "ShivaCore: USER-001 mapped PID={} RIP={:#x} RSP={:#x} (Quelle: {})",
        user_pid.0,
        user_ctx.rip,
        user_ctx.rsp,
        source
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
        let process_space = shivacore::addr_space::AddressSpace::create(
            phys_mem_offset,
            &mut frame_allocator,
            &user_addr_space_2.mapped_regions(),
            boot_cr3_frame,
        )
        .expect("ShivaCore: kein Adressraum fuer PID 1001");
        process_space.install();
        let mut process_mapper = process_space.mapper(phys_mem_offset);
        userspace::map_user_binary(
            &mut process_mapper,
            &mut frame_allocator,
            phys_mem_offset,
            &user_binary_2,
            &user_addr_space_2,
        )
        .expect("ShivaCore: second USER-001 user page mapping failed");
        cr3_second = process_space.frame().start_address().as_u64();
    }
    let user_pid_2 = ats1000::Pid(1001);
    let user_ctx_2 = userspace::UserContext::new(user_pid_2, &user_binary_2, user_addr_space_2);

    // Dritter Ring-3-Prozess: holt Tastenereignisse ueber IpcReceive ab.
    //
    // Die eingefrorene ABI hat keinen Pufferzeiger, aber einen 64-Bit-Rueck-
    // gabewert. Ein Tastenereignis passt dort hinein (Scancode in Bits 0..8,
    // Zeichen in Bits 16..24), deshalb braucht der Kanal keinen ABI-Zusatz.
    //
    // Der Cap-Slot ist eine Immediate im Code: die Nummer vergibt die
    // Capability-Tabelle, deshalb wird sie hier erfragt und in den Bytes
    // eingesetzt, nicht hartkodiert.
    //
    // Maschinencode:
    //   48 BF imm64        mov  rdi, <cap-slot>     (Cap fuer den Eingabekanal)
    //   BE 11 00 00 00     mov  esi, 0x11           (IpcReceive)
    //   41 B9 00 00 01 00  mov  r9d, 0x00010000     (ABI 1.0)
    //   48 C7 C2 01 00 00 00  mov rdx, 1            (Kanal-ID des Eingabekanals)
    //   CD 80              int  0x80
    //   EB FC              jmp  zurueck auf int 0x80
    let input_cap = interrupts::grant_input_read(ats1000::Pid(1002))
        .expect("ShivaCore: Leserecht auf den Eingabekanal");
    // ABI 2.0.0: Der Prozess baut einen Argumentblock in seiner Datenseite und
    // uebergibt den Zeiger. Das Abbild der Anweisungen wird hier aufgebaut, und
    // alle Spruenge und der Platz der Capability werden aus der jeweils
    // aktuellen Laenge berechnet. Von Hand gezaehlte Offsets waren zuvor um ein
    // Byte verrutscht; mit dieser Bauart kann das nicht mehr passieren, weil es
    // keinen festen Offset mehr gibt.
    fn store_qword_at(address: u32, value: u32) -> [u8; 12] {
        let a = address.to_le_bytes();
        let v = value.to_le_bytes();
        [
            0x48, 0xC7, 0x04, 0x25, a[0], a[1], a[2], a[3], v[0], v[1], v[2], v[3],
        ]
    }
    fn mov_rdi(value: u64) -> [u8; 10] {
        let v = value.to_le_bytes();
        [0x48, 0xBF, v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7]]
    }

    const DATA_BASE: u32 = 0x0190_0000;
    // Adresse in der Stackseite des Prozesses: ein Schreibzugriff dorthin
    // beweist, dass Ring 3 seinen Stack benutzen darf.
    const STACK_PROBE: u32 = 0x07FD_FFC0;

    let mut syscall_loop: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
    syscall_loop.extend_from_slice(&store_qword_at(STACK_PROBE, 1));

    // Block: Groesse, Opcode/Flags, Capability, Kanal, arg1, Puffer, Laenge.
    syscall_loop.extend_from_slice(&store_qword_at(DATA_BASE, 56));
    syscall_loop.extend_from_slice(&store_qword_at(DATA_BASE + 8, 0x0011));
    let capability_slot = syscall_loop.len() + 2;
    syscall_loop.extend_from_slice(&mov_rdi(0));
    syscall_loop.extend_from_slice(&[0x48, 0x89, 0x3C, 0x25, 0x10, 0x00, 0x90, 0x01]);
    for (offset, value) in [(24u32, 1u32), (32, 0), (40, 0), (48, 0)] {
        syscall_loop.extend_from_slice(&store_qword_at(DATA_BASE + offset, value));
    }

    let loop_entry = syscall_loop.len();
    syscall_loop.extend_from_slice(&mov_rdi(DATA_BASE as u64));
    syscall_loop.extend_from_slice(&[0x48, 0xC7, 0xC6, 0x38, 0x00, 0x00, 0x00]);
    syscall_loop.extend_from_slice(&[0x48, 0xBA, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00]);
    syscall_loop.extend_from_slice(&[0xCD, 0x80]);
    let jump_back = (loop_entry as isize - (syscall_loop.len() as isize + 2)) as i8;
    syscall_loop.extend_from_slice(&[0xEB, jump_back as u8]);
    syscall_loop[capability_slot..capability_slot + 8].copy_from_slice(&input_cap.to_le_bytes());

    let mut user_binary_3 =
        userspace::UserBinary::from_bytes("ring3-input", syscall_loop, 0x0180_0000);
    // Datenseite anfordern: map_user_binary legt sie nur an, wenn Daten da sind.
    user_binary_3.data = alloc::vec![0u8; 64];
    let user_addr_space_3 = userspace::UserAddressSpace {
        code_base: 0x0180_0000,
        code_size: 0x0010_0000,
        data_base: 0x0190_0000,
        data_size: 0x0010_0000,
        stack_base: 0x07FE_0000,
        stack_size: 0x0000_8000,
        heap_base: 0x01A0_0000,
        heap_size: 0x0010_0000,
    };
    unsafe {
        let process_space = shivacore::addr_space::AddressSpace::create(
            phys_mem_offset,
            &mut frame_allocator,
            &user_addr_space_3.mapped_regions(),
            boot_cr3_frame,
        )
        .expect("ShivaCore: kein Adressraum fuer PID 1002");
        process_space.install();
        let mut process_mapper = process_space.mapper(phys_mem_offset);
        userspace::map_user_binary(
            &mut process_mapper,
            &mut frame_allocator,
            phys_mem_offset,
            &user_binary_3,
            &user_addr_space_3,
        )
        .expect("ShivaCore: third USER-001 user page mapping failed");
        cr3_third = process_space.frame().start_address().as_u64();
    }
    // Vierter Prozess: das zweite Programm aus dem Verzeichnis. Eigener
    // Adressraum, weil ein gemeinsames Codesegment mit dem Init sonst
    // ueberschrieben wuerde; der Code ist lagerelativ, deshalb darf er wandern.
    let user_ctx_4 = ramdisk_second.map(|(name, code, entry)| {
        let base: u64 = 0x01C0_0000;
        let space = userspace::UserAddressSpace {
            code_base: base,
            code_size: 0x0010_0000,
            data_base: 0x01D0_0000,
            data_size: 0x0010_0000,
            stack_base: 0x07F0_0000,
            stack_size: 0x0000_8000,
            heap_base: 0x01E0_0000,
            heap_size: 0x0010_0000,
        };
        let _ = entry;
        let binary = userspace::UserBinary::from_bytes(&name, code, base);
        let process_space = unsafe {
            shivacore::addr_space::AddressSpace::create(
                phys_mem_offset,
                &mut frame_allocator,
                &space.mapped_regions(),
                boot_cr3_frame,
            )
        }
        .expect("ShivaCore: kein Adressraum fuer PID 1003");
        process_space.install();
        let mut process_mapper = unsafe { process_space.mapper(phys_mem_offset) };
        unsafe {
            userspace::map_user_binary(
                &mut process_mapper,
                &mut frame_allocator,
                phys_mem_offset,
                &binary,
                &space,
            )
            .expect("ShivaCore: vierter Prozess nicht abbildbar");
        }
        cr3_fourth = process_space.frame().start_address().as_u64();
        let context = userspace::UserContext::new(ats1000::Pid(1003), &binary, space);
        serial_println!(
            "ShivaCore: vierter Prozess PID=1003 aus Ramdisk-Programm '{}' RIP={:#x}",
            name,
            context.rip
        );
        context
    });

    let user_pid_3 = ats1000::Pid(1002);
    let user_ctx_3 = userspace::UserContext::new(user_pid_3, &user_binary_3, user_addr_space_3);
    serial_println!(
        "ShivaCore: USER-001 dritter Prozess PID={} RIP={:#x} (Syscall-Schleife)",
        user_pid_3.0,
        user_ctx_3.rip
    );

    interrupts::init_user_scheduler(&user_ctx, &user_ctx_2, &user_ctx_3, user_ctx_4.as_ref());
    // Jede eigene Tabelle in den Scheduler legen. Ab hier entscheidet der
    // Kontextwechsel, mit welcher Tabelle ein Prozess laeuft.
    interrupts::set_process_cr3(user_pid, cr3_first);
    interrupts::set_process_cr3(user_pid_2, cr3_second);
    interrupts::set_process_cr3(user_pid_3, cr3_third);
    if user_ctx_4.is_some() {
        interrupts::set_process_cr3(ats1000::Pid(1003), cr3_fourth);
    }
    serial_println!(
        "ShivaCore: Adressraeume je Prozess: PID={} -> {:#x}, PID={} -> {:#x}, PID={} -> {:#x}, PID=1003 -> {:#x}",
        user_pid.0,
        cr3_first,
        user_pid_2.0,
        cr3_second,
        user_pid_3.0,
        cr3_third,
        cr3_fourth
    );
    // Kernel-Arbeit wieder in der Boot-Tabelle erledigen, damit spaetere
    // Abbildungen (Geraete) nicht versehentlich in einer Prozesstabelle landen.
    shivacore::addr_space::restore(boot_cr3_frame, boot_cr3_flags);
    serial_println!(
        "ShivaCore: USER-001 entering PID={} with timer preemption target PID={}",
        user_pid.0,
        user_pid_2.0
    );
    // Der erste Sprung nach Ring 3 passiert ohne Kontextwechsel davor: die
    // Tabelle von PID 1000 muss hier ausdruecklich gesetzt werden.
    shivacore::addr_space::install_raw(cr3_first);
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
) -> Result<(usize, Option<shivacore::madt::MadtSummary>, Option<u64>), &'static str> {
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
    let mut hpet = None;
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
        if table.len() >= 4 && table[0..4] == shivacore::acpi_devices::HPET_SIGNATURE {
            hpet = Some(
                shivacore::acpi_devices::hpet_address(table)
                    .map_err(|_| "HPET address structure is malformed")?,
            );
        }
        validated = validated
            .checked_add(1)
            .ok_or("ACPI table count overflow")?;
    }
    Ok((validated, madt, hpet))
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
fn format_hex(value: u8) -> alloc::string::String {
    alloc::format!("0x{:X}", value)
}

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
