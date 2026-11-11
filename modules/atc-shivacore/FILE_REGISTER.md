# 📋 File Register — atc-shivacore

> **Zweck:** Vollständige Liste aller getrackten Dateien (ohne Build-Artefakte).
> **Stand:** 11.11.2026 gegen `git ls-files modules/atc-shivacore` geprueft: 121 Dateien, vorher waren hier 106 genannt (122 vor der Entfernung von `service_space/src/ai.rs`).
> Die Dateiliste ist an dieser Stelle nicht vollstaendig nachgezogen.
> **Erzeugt:** 2026-11-07 aus `git ls-files modules/atc-shivacore` · **Dateien:** 121
>
> Diese Tabelle wird aus dem Baum erzeugt, nicht gepflegt. Der vorige Stand war vom
> 2026-10-08 und nannte 15 Dateien, die es nicht mehr gibt (u. a. `kernel/src/devfs.rs`,
> `kernel/src/fs_journal.rs`, `kernel/src/hw_drivers.rs`), und 11 falsche Zeilenzahlen
> (`kernel/src/interrupts.rs` mit 314 statt 919). `legacy/kernel-src-uncompiled/`
> ist mit aufgeführt, weil die Dateien getrackt sind — sie bauen nicht mit, siehe
> `docs/AUDIT_2026-11-06_KERNEL_FIXES.md`.

| Datei | Zeilen | Typ |
|------|--------|-----|
| `./.gitignore` | 23 | – |
| `./ARCHITECTURE.md` | 112 | md |
| `./CHANGELOG.md` | 21 | md |
| `./COMPONENT_PLAN.md` | 732 | md |
| `./FILE_REGISTER.md` | 132 | md |
| `./LICENSE` | 17 | – |
| `./README.md` | 1.101 | md |
| `./ROADMAP.md` | 21 | md |
| `./STATUS.md` | 114 | md |
| `./boot/Cargo.toml` | 15 | toml |
| `./boot/src/main.rs` | 72 | rs |
| `./docs/AUDIT_2026-11-06_KERNEL_FIXES.md` | 2.318 | md |
| `./docs/M1.2_VMM_IMPLEMENTATION.md` | 138 | md |
| `./docs/M1.2_VMM_SECURITY_REVIEW.md` | 75 | md |
| `./docs/SC-006-ACK-ABONNENTEN.md` | 89 | md |
| `./docs/SC-006_EVENT_KANAL_ENTWURF.md` | 74 | md |
| `./docs/SC-ABI-002-ARGUMENTBLOCK.md` | 106 | md |
| `./init_program/.cargo/config.toml` | 8 | toml |
| `./init_program/Cargo.toml` | 16 | toml |
| `./init_program/linker.ld` | 35 | ld |
| `./init_program/src/allocator.rs` | 52 | rs |
| `./init_program/src/bin/crash.rs` | 37 | rs |
| `./init_program/src/bin/crash_gp.rs` | 34 | rs |
| `./init_program/src/bin/crash_ud.rs` | 33 | rs |
| `./init_program/src/bin/peek.rs` | 73 | rs |
| `./init_program/src/bin/probe.rs` | 53 | rs |
| `./init_program/src/main.rs` | 104 | rs |
| `./kernel/.cargo/config.toml` | 2 | toml |
| `./kernel/.gitignore` | 1 | – |
| `./kernel/Cargo.toml` | 40 | toml |
| `./kernel/src/addr_space.rs` | 275 | rs |
| `./kernel/src/allocator.rs` | 49 | rs |
| `./kernel/src/apic.rs` | 410 | rs |
| `./kernel/src/atcfs.rs` | 709 | rs |
| `./kernel/src/ats1000.rs` | 87 | rs |
| `./kernel/src/boot_devices.rs` | 241 | rs |
| `./kernel/src/capability.rs` | 537 | rs |
| `./kernel/src/cross_subsystem.rs` | 544 | rs |
| `./kernel/src/diagnostics.rs` | 84 | rs |
| `./kernel/src/dma_auth.rs` | 369 | rs |
| `./kernel/src/elf_loader.rs` | 1.243 | rs |
| `./kernel/src/fault.rs` | 339 | rs |
| `./kernel/src/framebuffer.rs` | 123 | rs |
| `./kernel/src/gdt.rs` | 95 | rs |
| `./kernel/src/hal/cpu.rs` | 209 | rs |
| `./kernel/src/hal/mod.rs` | 4 | rs |
| `./kernel/src/input_queue.rs` | 254 | rs |
| `./kernel/src/interrupts.rs` | 1.376 | rs |
| `./kernel/src/ioapic.rs` | 382 | rs |
| `./kernel/src/ipc.rs` | 717 | rs |
| `./kernel/src/irq.rs` | 322 | rs |
| `./kernel/src/irq_bridge.rs` | 537 | rs |
| `./kernel/src/irq_routing.rs` | 251 | rs |
| `./kernel/src/kernel_caps.rs` | 138 | rs |
| `./kernel/src/kernel_init.rs` | 346 | rs |
| `./kernel/src/key_events.rs` | 157 | rs |
| `./kernel/src/keymap.rs` | 401 | rs |
| `./kernel/src/lib.rs` | 59 | rs |
| `./kernel/src/lkm.rs` | 3.398 | rs |
| `./kernel/src/main.rs` | 1.352 | rs |
| `./kernel/src/memory.rs` | 64 | rs |
| `./kernel/src/memory_manager.rs` | 912 | rs |
| `./kernel/src/pci_scan.rs` | 558 | rs |
| `./kernel/src/process.rs` | 390 | rs |
| `./kernel/src/ramdisk.rs` | 332 | rs |
| `./kernel/src/scheduler.rs` | 465 | rs |
| `./kernel/src/security.rs` | 1.161 | rs |
| `./kernel/src/serial.rs` | 53 | rs |
| `./kernel/src/syscall.rs` | 702 | rs |
| `./kernel/src/syscall_gate.rs` | 170 | rs |
| `./kernel/src/system.rs` | 1.367 | rs |
| `./kernel/src/timer.rs` | 549 | rs |
| `./kernel/src/user_sched.rs` | 1.490 | rs |
| `./kernel/src/userspace.rs` | 1.180 | rs |
| `./kernel/src/vfs.rs` | 1.198 | rs |
| `./kernel/src/vmm.rs` | 1.472 | rs |
| `./legacy/kernel-src-uncompiled/README.md` | 20 | md |
| `./legacy/kernel-src-uncompiled/block.rs` | 548 | rs |
| `./legacy/kernel-src-uncompiled/container.rs` | 2.758 | rs |
| `./legacy/kernel-src-uncompiled/cow.rs` | 1.485 | rs |
| `./legacy/kernel-src-uncompiled/devfs.rs` | 922 | rs |
| `./legacy/kernel-src-uncompiled/fs_journal.rs` | 1.162 | rs |
| `./legacy/kernel-src-uncompiled/hw_drivers.rs` | 3 | rs |
| `./legacy/kernel-src-uncompiled/module_security.rs` | 1.683 | rs |
| `./legacy/kernel-src-uncompiled/page_fault.rs` | 1.372 | rs |
| `./legacy/kernel-src-uncompiled/power.rs` | 1.154 | rs |
| `./legacy/kernel-src-uncompiled/signals.rs` | 2.250 | rs |
| `./legacy/kernel-src-uncompiled/smp.rs` | 2.507 | rs |
| `./legacy/kernel-src-uncompiled/threads.rs` | 1.468 | rs |
| `./legacy/kernel-src-uncompiled/tracing.rs` | 2.255 | rs |
| `./legacy/kernel-src-uncompiled/user_io.rs` | 1.324 | rs |
| `./service_space/Cargo.toml` | 32 | toml |
| `./service_space/src/atcnet.rs` | 1.255 | rs |
| `./service_space/src/blockchain.rs` | 505 | rs |
| `./service_space/src/consensus.rs` | 1.135 | rs |
| `./service_space/src/container_net.rs` | 2.840 | rs |
| `./service_space/src/contract.rs` | 339 | rs |
| `./service_space/src/device_registry.rs` | 977 | rs |
| `./service_space/src/did.rs` | 394 | rs |
| `./service_space/src/genesis.rs` | 1.183 | rs |
| `./service_space/src/genesis_bridge.rs` | 1.148 | rs |
| `./service_space/src/gossip_bridge.rs` | 1.498 | rs |
| `./service_space/src/hw_drivers.rs` | 1.767 | rs |
| `./service_space/src/identity_key_hal.rs` | 109 | rs |
| `./service_space/src/identity_key_service.rs` | 101 | rs |
| `./service_space/src/knowledge_graph.rs` | 1.075 | rs |
| `./service_space/src/lib.rs` | 44 | rs |
| `./service_space/src/mempool.rs` | 860 | rs |
| `./service_space/src/net.rs` | 859 | rs |
| `./service_space/src/p2p.rs` | 1.032 | rs |
| `./service_space/src/p2p_secure.rs` | 1.530 | rs |
| `./service_space/src/remote_caps.rs` | 878 | rs |
| `./service_space/src/security_audit.rs` | 1.670 | rs |
| `./service_space/src/service_init.rs` | 212 | rs |
| `./service_space/src/sockets.rs` | 2.065 | rs |
| `./service_space/src/tcpip.rs` | 1.137 | rs |
| `./service_space/src/vm.rs` | 792 | rs |
