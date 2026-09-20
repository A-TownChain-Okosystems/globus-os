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
mod ats1000;
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

    let phys_mem_offset = boot_info
        .physical_memory_offset
        .into_option()
        .expect("Bootloader hat physical_memory_offset nicht gesetzt (Config fehlt?)");
    let phys_mem_offset = x86_64::VirtAddr::new(phys_mem_offset);

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
