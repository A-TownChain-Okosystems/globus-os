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
    let boot_apic_id = hal::cpu::current_apic_id();
    serial_println!("ShivaCore: BSP identity APIC_ID={:#x} (identity only; SMP not enabled)", boot_apic_id);
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
    let validated_tables = validate_acpi_boot_tables(boot_info, phys_mem_offset_raw)
        .unwrap_or_else(|error| panic!("ShivaCore: ACPI validation failed: {}", error));
    serial_println!(
        "ShivaCore: ACPI RSDP/root/table checksums validated; tables={}",
        validated_tables
    );
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
    let mut kernel_state = shivacore::kernel_init::KernelState::boot()
        .expect("ShivaCore: canonical KernelState::boot() failed");
    serial_println!("ShivaCore: KernelState::boot() -> Done.");
    serial_println!("{}", kernel_state.boot_log());

    // Trusted boot-time IPC bootstrap for the two Ring-3 smoke identities.
    // PID 1001 owns the channel and receives READ; PID 1000 receives only a
    // delegated WRITE capability. These grants live in the canonical kernel
    // capability table and are not user-created or inferred from the PID.
    let ring3_channel = kernel_state.ipc.create_channel(
        &mut kernel_state.processes.caps,
        ats1000::Pid(1001),
        8,
    );
    let owner_send_cap = kernel_state.processes.caps
        .list_for(ats1000::Pid(1001))
        .iter()
        .find(|cap| {
            cap.resource_type == shivacore::capability::ResourceType::IpcChannel
                && cap.resource_id == ring3_channel.0
                && cap.rights.has(
                    shivacore::capability::Rights::WRITE
                        | shivacore::capability::Rights::DELEGATE,
                )
        })
        .map(|cap| cap.id)
        .expect("Ring-3 IPC bootstrap did not create owner WRITE/DELEGATE capability");
    let ring3_sender_cap = kernel_state.processes.caps
        .delegate(
            ats1000::Pid(1001),
            owner_send_cap,
            ats1000::Pid(1000),
            shivacore::capability::Rights::WRITE,
        )
        .expect("Ring-3 sender capability delegation failed");
    let ring3_receiver_cap = kernel_state.processes.caps
        .list_for(ats1000::Pid(1001))
        .iter()
        .find(|cap| {
            cap.resource_type == shivacore::capability::ResourceType::IpcChannel
                && cap.resource_id == ring3_channel.0
                && cap.rights.has(shivacore::capability::Rights::READ)
        })
        .map(|cap| cap.id)
        .expect("Ring-3 receiver READ capability missing");
    serial_println!(
        "ShivaCore: Ring-3 IPC bootstrap channel={} sender_pid=1000 sender_cap={} receiver_pid=1001 receiver_cap={} capacity=8",
        ring3_channel.0,
        ring3_sender_cap.0,
        ring3_receiver_cap.0
    );
    // Move the canonical state into the syscall runtime before Ring 3 starts.
    // No shadow capability table or IPC registry is created for trap handling.
    interrupts::install_syscall_kernel(kernel_state);

    // Real USER-001 smoke path: install executable user pages and enter CPL3
    // through IRETQ. The payload loops forever so timer IRQs can observe a
    // genuine ring-3 CPU context without executing privileged instructions.
    // Real Ring-3 sender: invoke ABI-v1 IPC_SEND through int 0x80.
    // Constants come from libshivacore's public ABI; payload is in the mapped
    // writable data region so the kernel must validate/copy a real user span.
    let mut sender_code = alloc::vec::Vec::new();
    let sender_payload = b"ring3-ipc-ok";
    // Emit multiple real traps: foreign cap, absent cap, unmapped pointer, then
    // a valid send. The kernel's trap log records each return code.
    let mut emit_send = |capability: u64, payload_ptr: u64, payload_len: usize| {
        sender_code.extend_from_slice(&[0x48, 0xB8]);
        sender_code.extend_from_slice(&(libshivacore::Syscall::IpcSend.id() as u64).to_le_bytes());
        sender_code.extend_from_slice(&[0x48, 0xBF]);
        sender_code.extend_from_slice(&(libshivacore::ABI_VERSION as u64).to_le_bytes());
        sender_code.extend_from_slice(&[0x48, 0xBE]);
        sender_code.extend_from_slice(&capability.to_le_bytes());
        sender_code.extend_from_slice(&[0x48, 0xBA]);
        sender_code.extend_from_slice(&ring3_channel.0.to_le_bytes());
        sender_code.extend_from_slice(&[0x49, 0xB8]);
        sender_code.extend_from_slice(&payload_ptr.to_le_bytes());
        sender_code.extend_from_slice(&[0x49, 0xB9]);
        sender_code.extend_from_slice(&(payload_len as u64).to_le_bytes());
        sender_code.extend_from_slice(&[0xCD, libshivacore::trap_abi::VECTOR]);
    };
    emit_send(ring3_receiver_cap.0, 0x0050_0000, sender_payload.len());
    emit_send(0, 0x0050_0000, sender_payload.len());
    emit_send(ring3_sender_cap.0, 0x0050_1000, sender_payload.len()); // valid region, unmapped page
    emit_send(ring3_sender_cap.0, 0x0050_0000, sender_payload.len());
    // The receiver consumes at most one message. Nine further sends force at
    // least one ResourceExhausted result even if timer scheduling interleaves.
    for _ in 0..9 {
        emit_send(ring3_sender_cap.0, 0x0050_0000, sender_payload.len());
    }
    sender_code.extend_from_slice(&[0xEB, 0xFE]); // park until timer preemption

    let mut user_binary = userspace::UserBinary::from_bytes(
        "ring3-ipc-sender",
        sender_code,
        0x0040_0000,
    );
    user_binary.data = sender_payload.to_vec();
    let user_addr_space = userspace::UserAddressSpace::default();
    let (user_root_1, mut user_mapper_1) = unsafe {
        memory::create_user_mapper(phys_mem_offset, &mut frame_allocator)
            .expect("ShivaCore: could not create isolated PML4 for PID=1000")
    };
    // Image bytes are copied through their user virtual addresses, so make
    // the destination root active while populating the freshly mapped pages.
    unsafe { memory::switch_to_page_table(user_root_1) };
    unsafe {
        userspace::map_user_binary(
            &mut user_mapper_1,
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

    // Second real Ring-3 image. It receives a separate PML4 root with its own
    // lower-half user mappings and the same supervisor-only kernel upper half.
    // Real Ring-3 receiver: invoke ABI-v1 IPC_RECEIVE into its mapped data.
    let mut receiver_code = alloc::vec::Vec::new();
    let mut emit_receive = |capacity: u64| {
        receiver_code.extend_from_slice(&[0x48, 0xB8]);
        receiver_code.extend_from_slice(&(libshivacore::Syscall::IpcReceive.id() as u64).to_le_bytes());
        receiver_code.extend_from_slice(&[0x48, 0xBF]);
        receiver_code.extend_from_slice(&(libshivacore::ABI_VERSION as u64).to_le_bytes());
        receiver_code.extend_from_slice(&[0x48, 0xBE]);
        receiver_code.extend_from_slice(&ring3_receiver_cap.0.to_le_bytes());
        receiver_code.extend_from_slice(&[0x48, 0xBA]);
        receiver_code.extend_from_slice(&ring3_channel.0.to_le_bytes());
        receiver_code.extend_from_slice(&[0x49, 0xB8]);
        receiver_code.extend_from_slice(&0x0150_0000u64.to_le_bytes());
        receiver_code.extend_from_slice(&[0x49, 0xB9]);
        receiver_code.extend_from_slice(&capacity.to_le_bytes());
        receiver_code.extend_from_slice(&[0xCD, libshivacore::trap_abi::VECTOR]);
    };
    // A too-small receive must fail without dequeuing the queued message.
    // Retry while the queue is empty (error code 5); once a message exists,
    // the expected InvalidPayload (4) falls through to the valid receive.
    emit_receive(2);
    drop(emit_receive);
    receiver_code.extend_from_slice(&[0x83, 0xFA, 0x05, 0x74, 0xBD]); // cmp edx,5; je back to first receive
    // Now receive the same message with sufficient capacity.
    let mut emit_valid_receive = |capacity: u64| {
        receiver_code.extend_from_slice(&[0x48, 0xB8]);
        receiver_code.extend_from_slice(&(libshivacore::Syscall::IpcReceive.id() as u64).to_le_bytes());
        receiver_code.extend_from_slice(&[0x48, 0xBF]);
        receiver_code.extend_from_slice(&(libshivacore::ABI_VERSION as u64).to_le_bytes());
        receiver_code.extend_from_slice(&[0x48, 0xBE]);
        receiver_code.extend_from_slice(&ring3_receiver_cap.0.to_le_bytes());
        receiver_code.extend_from_slice(&[0x48, 0xBA]);
        receiver_code.extend_from_slice(&ring3_channel.0.to_le_bytes());
        receiver_code.extend_from_slice(&[0x49, 0xB8]);
        receiver_code.extend_from_slice(&0x0150_0000u64.to_le_bytes());
        receiver_code.extend_from_slice(&[0x49, 0xB9]);
        receiver_code.extend_from_slice(&capacity.to_le_bytes());
        receiver_code.extend_from_slice(&[0xCD, libshivacore::trap_abi::VECTOR]);
    };
    emit_valid_receive(64);
    drop(emit_valid_receive);
    receiver_code.extend_from_slice(&[0xEB, 0xFE]);

    let mut user_binary_2 = userspace::UserBinary::from_bytes(
        "ring3-ipc-receiver",
        receiver_code,
        0x0040_1000,
    );
    user_binary_2.data = alloc::vec![0u8; 64];
    // The roots have independent lower halves, so the processes may reuse the
    // same user virtual layout. Keep distinct addresses here for easier QEMU
    // diagnostics while validating the isolation mechanism.
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
    let (user_root_2, mut user_mapper_2) = unsafe {
        memory::create_user_mapper(phys_mem_offset, &mut frame_allocator)
            .expect("ShivaCore: could not create isolated PML4 for PID=1001")
    };
    unsafe { memory::switch_to_page_table(user_root_2) };
    unsafe {
        userspace::map_user_binary(
            &mut user_mapper_2,
            &mut frame_allocator,
            phys_mem_offset,
            &user_binary_2,
            &user_addr_space_2,
        )
        .expect("ShivaCore: second USER-001 user page mapping failed");
    }
    let user_pid_2 = ats1000::Pid(1001);
    let user_ctx_2 = userspace::UserContext::new(user_pid_2, &user_binary_2, user_addr_space_2);

    // Return to the sender root before registering both spaces; all later
    // kernel execution uses shared supervisor mappings, and timer preemption
    // performs the per-process CR3 switch.
    unsafe { memory::switch_to_page_table(user_root_1) };
    interrupts::init_user_scheduler(
        &user_ctx,
        user_root_1,
        user_mapper_1,
        &user_ctx_2,
        user_root_2,
        user_mapper_2,
    );
    serial_println!(
        "ShivaCore: USER-001 isolated page-table roots PID=1000 CR3={:#x} PID=1001 CR3={:#x}",
        user_root_1.start_address().as_u64(),
        user_root_2.start_address().as_u64()
    );
    serial_println!(
        "ShivaCore: USER-001 entering PID={} with timer preemption target PID={}",
        user_pid.0,
        user_pid_2.0
    );
    // Enter Ring 3 with PID 1000's private lower-half mappings active.
    unsafe { memory::switch_to_page_table(user_root_1) };
    unsafe { userspace::enter_ring3(&user_ctx) }

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
) -> Result<usize, &'static str> {
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
        validated = validated
            .checked_add(1)
            .ok_or("ACPI table count overflow")?;
    }
    Ok(validated)
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
