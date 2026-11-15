# ShivaCore Dependency-Matrix — Anbindungen, Lücken, Doppelablagen

**Stand:** 15.11.2026 · Basis: `globus-os` `main` = `50ffc556` (frischer Klon, 558 Dateien im Register)
**Methode:** statisch, read-only. Quellen: Workspace-Graph aus `Cargo.toml` (30 Member), Aufrufpfad-Grep ab `main.rs`, `kernel_init.rs`, `interrupts.rs`, `syscall.rs`, ABI-Enum in `system/shivacore-api/src/lib.rs`. Kein neuer Laufzeitbeleg, keine Commits.
**Codebasis:** Kernel-Crate 56 Module / 34.490 Zeilen; Service Space 25 Module / 26.977 Zeilen; `system/*` 24 Crates.
<!-- audit-basis: 50ffc5565377e2ad20abfb59e8515cd882ef0dfd -->

**Aussagekraft (verbindlich):** Der untersuchte Quellstand ist der Commit `50ffc556`; der
Commit, der diese Datei aufnimmt, ist **nicht** der untersuchte Stand. Zwei Vorbehalte
gelten fuer alle Aussagen unten: (1) `MISSING` heisst `NOT_FOUND_IN_AUDITED_SCOPE` — kein
Treffer im geprueften Umfang (`modules/atc-shivacore`, `system/*`, Cargo-Graph, Aufrufpfade),
nicht der Nachweis, dass keine Implementierung existiert. (2) Bei Entropie ist die Wirkung
zu trennen: fehlende Entropie blockiert Schluesselerzeugung und sichere Zufallsoperationen;
die Signaturpruefung eines Secure-Boot-Images braucht nicht zwingend einen Zufallszahlengenerator.

## Statusvokabular

| Status | Bedeutung |
| --- | --- |
| `WIRED` | von `main.rs`, `kernel_init` oder dem Interrupt-Pfad erreichbar **und** mit Nutzerkante (Syscall, IRQ, Gerät) |
| `BOOTED_NO_CONSUMER` | wird beim Boot initialisiert, aber kein Syscall und kein Dienst erreicht es |
| `OFF_IMAGE` | Code vorhanden, läuft aber nicht im Kernel-Image (Service Space / separate Edition) |
| `HOST_ONLY` | nur im Testbuild eingebunden (`#[cfg(test)]`) |
| `ISLAND` | keine eingehende Kante: weder Boot noch Interrupt noch Syscall |
| `DUPLICATE` | zweite Ablage derselben Verantwortung |
| `MISSING` | `NOT_FOUND_IN_AUDITED_SCOPE`: kein Treffer im geprueften Umfang, kein Nachweis der Nichtexistenz |

---

## 1. Die 15 Anbindungsbereiche

| Nr | Anbindung | Ort im Code | Status | Befund / Kante |
| --- | --- | --- | --- | --- |
| 1 | CPU / Architektur-HAL | `kernel/hal/{cpu,mod}.rs`, `gdt.rs` | `WIRED` | `main.rs` ruft HAL-Kompatibilitätsprüfung; Vendor/APIC/NX werden gelesen. **SMP fehlt vollständig** (0 Treffer auf `smp`, `startup_ipi`, `per_cpu`). |
| 2 | MMU / Memory | `vmm.rs` (1534), `addr_space.rs` (374), `allocator.rs` (107), `reclaim.rs` (382) | `WIRED` | Seitentabellen, CR3 je Prozess, Frame-Rückgewinnung im Boot-Pfad. Zweite Ablage: `memory_manager.rs` (940) nur in der Init-Kette. |
| 3 | Interrupt-Controller | `apic.rs`, `ioapic.rs`, `irq.rs`, `irq_bridge.rs`, `irq_routing.rs` | `WIRED` | LAPIC-MMIO, IOAPIC-Programmierung, Maskierung bis ACK, Routingtabelle. Grenze: genau eine Line (Timer) geroutet; **kein ACK-Syscall**, Geräte-IRQs enden im Log. |
| 4 | Timer / Clock | Tick-Pfad in `interrupts.rs`, PIT/TSC-Kalibrierung in `main.rs`; `timer.rs` (571) daneben | `WIRED` + `DUPLICATE` | Echte Zeit existiert. `timer.rs` (`TimerManager`, `SimulatedTimerSource`) hat **0 eingehende Kanten** — Host-Simulation ohne Nutzer. |
| 5 | Scheduler | `user_sched.rs` (1622) im Interrupt-Pfad; `scheduler.rs` (505, DA-HEFT) in der Init-Kette | `WIRED` + `DUPLICATE` | Ausgeführt wird Round-Robin über den Timer; DA-HEFT ist initialisiert, aber nicht der laufende Scheduler. Thread-Ebene ist im Pfad nicht belegt (nur Prozesse). |
| 6 | IPC | `ipc.rs` (737), `ipc_lifecycle.rs` (1016), `system/ipc` (387) | `BOOTED_NO_CONSUMER` | IPC-Subsystem wird gebaut, aber `IpcSend`/`IpcReceive` können keinen Transport liefern (ABI hat keinen Pufferzeiger für Nachrichten; `EndpointUnavailable`). Drei Ablagen, keine Transportkante. |
| 7 | Capability System | `capability.rs` (650, gewired), `cap_authority.rs` (385), `kernel_caps.rs` (140), `system/security` (235) | `WIRED` + `DUPLICATE` | `CapabilityQuery` funktioniert; `kernel_init` behauptet „genau eine Authority“, der Code hat **drei Kernel-Loci** plus ein System-Crate. |
| 8 | Process / Thread | `userspace.rs` (1408), `spawn.rs` (328), `elf_loader.rs` (1494); `process.rs` (418) in der Kette | `WIRED` + `DUPLICATE` | Ring-3-Prozesse, Adressräume, Zombie/Exit. Zweite Ablage `process.rs` (Prozessgruppen). Spawn-Syscall fehlt: `0x0060` ist laut ABI **für `SpawnProcess` reserviert**. |
| 9 | Boot / Firmware | `main.rs`, ACPI/MADT/HPET aus `libshivacore`, `boot_devices.rs`, `ramdisk.rs` | `WIRED` | BIOS und UEFI belegt, ACPI-Checksummen, MADT, Gerätevorrat. **Kein Secure/Measured Boot**, kein `globus-init`. |
| 10 | Syscall Interface | ABI 2.0 in `system/shivacore-api` (1254) | `WIRED` | 9 Syscalls: `Yield`, `Exit`, `IpcSend`, `IpcReceive`, `CapabilityQuery`, `HandleClose`, `MonotonicTime`, `Console`, `IdentityCheck`. **Keine** Speicher-, VFS-, Netz-, IRQ-ACK- oder Spawn-Nummer. |
| 11 | IOMMU / DMA Authority | `dma_auth.rs` (383) | `WIRED` (Policy) | Cap-gebundene Domainvergabe mit Negativtests im Boot. Grenze: schreibt **keine** IOMMU-Seitentabellen, kein DMA-Fault-Pfad. |
| 12 | VFS / Storage | `vfs.rs` (1254) + `atcfs.rs` (731) im Kernel; `system/vfs` (18 Module, 2715) daneben | `BOOTED_NO_CONSUMER` + `DUPLICATE` | Dateibaum wird beim Boot aufgebaut, aber es gibt **keinen VFS-Syscall** und keinen Blocktreiber im Kernel-Crate. `system/vfs` (GPT, Journal, Extents, Recovery) wird von genau einem Crate gezogen. |
| 13 | Network Stack | `service_space`: `tcpip.rs`, `sockets.rs`, `net.rs`, `atcnet.rs`, `p2p*.rs`, `gossip_bridge.rs`, `container_net.rs` (~12k Zeilen); `system/net` (222) | `OFF_IMAGE` | Kernel korrekt frei von Netzcode (AD-012-konform). Der Stack ist aber nicht im Image: der Boot-Builder nimmt eine Ramdisk nur als optionales Argument, der CI-Aufruf übergibt keine. |
| 14 | Service Manager | `services.rs` (347, gewired), `system/services` (34), `service_space/service_init.rs` (221) | `DUPLICATE` | Drei Service-Modelle, kein laufender Manager: `services.rs` kennt Beenden als Neustart, `system/services` ist ein 34-Zeilen-Gerüst, `service_init` läuft nicht im Image. |
| 15 | Observability | `serial.rs` (58), `screen.rs` (106), `diagnostics.rs` (98), `system/diagnostics` (178), AuditLog in `security.rs` | `WIRED` (nur seriell) | Boot-Wahrheit ist die serielle Zeile. `screen.rs`, `diagnostics.rs` haben 0 Kanten; `system/diagnostics` wird von Kernel-`diagnostics.rs` benutzt, das selbst nicht gewired ist. Keine Metriken, kein Export, kein Abfragepfad. |

## 2. Weitere Systemanbindungen

| Anbindung | Ort | Status | Befund |
| --- | --- | --- | --- |
| SMP / Multi-Core | — | `MISSING` | 0 Treffer. Ein Kern, keine IPIs, keine Per-CPU-Daten. |
| Entropy / RNG | `entropy_gate.rs` (329), `system/entropy-core` (837) | `ISLAND` | Zwei Ablagen, **keine im Boot-Pfad**. `rand` wurde in Phase A aus dem Service Space entfernt; im Kernel-Pfad gibt es aktuell keinen Entropielieferanten. |
| Secure Boot | `service_space/identity_key_{hal,service}.rs` | `MISSING` (Kernel) / `OFF_IMAGE` | Kein TPM, keine Messung, keine Signaturprüfung des Image. |
| Kernel Integrity | `identity_check.rs`, `identity_file.rs`, `identity_gate.rs` | `WIRED` (Anmeldung) | IDENT-001/003: Passwortprüfung gegen Anmeldedatensatz. Das ist Authentifizierung, **keine** Integritätsmessung des Kerns. |
| ELF / Executable Loader | `elf_loader.rs`, `spawn.rs`, `ramdisk.rs` | `WIRED` | Ramdisk-Verzeichnis (SRAM) und ELF-Start funktionieren. Bekannter Rest aus dem Spawn-Befund: `map_user_binary` deckt nicht alle `PT_LOAD`-Segmente ab (`.bss`/Stringliterale). |
| Power Management | — | `MISSING` | Keine S-States, kein ACPI-Schlaf, kein Throttling. |
| ACPI | `libshivacore::{acpi_devices, madt}` | `WIRED` (Teil) | Tabellen, MADT, HPET. Kein AML-Interpreter, keine Power-/Hotplug-Ereignisse. |
| Watchdog | — | `MISSING` | Kein Watchdog-Gerät, kein Heartbeat, kein Recovery. |
| Crash / Panic Handler | `fault.rs` (467, 21 Kanten), Panik-Handler in `main.rs` | `WIRED` (Teil) | User-Faults terminieren den Prozess kontrolliert; Kernel-Panik ist `panic = "abort"` ohne Dump oder Persistenz. |
| System Time | `MonotonicTime` 0x0040 | `WIRED` (Teil) | Monoton vorhanden. **Keine RTC/Wanduhr** — es gibt keinen Zeitpunkt, nur eine Dauer seit Boot. |
| Resource Accounting | `reclaim.rs` | `MISSING` (als Accounting) | Frame-Rückgewinnung existiert, aber keine Limits/Quotas je Prozess, keine CPU-Zeit-Konten. |
| Audit Interface | `security.rs`, `cap_authority.rs` | `BOOTED_NO_CONSUMER` | Auditlog im Kernel-Speicher, nicht exportierbar, kein Syscall, keine Signatur. |
| **LKM (nicht in der Liste)** | `lkm.rs` (3625) | `HOST_ONLY` | Größtes Einzelmodul der Kernel-Crate, per `#[cfg(test)]` **nie im Kernel**. Beschreibt `insmod`/`rmmod`/Symbol-Exporte — das wäre eine Microkernel-Grenzverletzung, existiert aber nur als Testbau. |

## 3. Doppelte Ablagen (dieselbe Verantwortung, zwei Orte)

| Verantwortung | Ablage A | Ablage B | Ablage C | Konsequenz |
| --- | --- | --- | --- | --- |
| Scheduler | `user_sched.rs` (läuft) | `scheduler.rs` DA-HEFT (Kette) | — | Zwei Politiken, eine wird ausgeführt. |
| Prozessverwaltung | `userspace.rs` + `spawn.rs` | `process.rs` (Kette) | `system/process` (123) | Drei Prozessbegriffe. |
| Speicher | `vmm.rs` + `addr_space.rs` | `memory_manager.rs` (Kette) | `system/memory` (438) | Drei Speicherbegriffe. |
| Capabilities | `capability.rs` | `cap_authority.rs` + `kernel_caps.rs` | `system/security` | „Eine Authority“ ist derzeit nicht belegbar. |
| IPC | `ipc.rs` | `ipc_lifecycle.rs` | `system/ipc` (387) | Keine Transportkante, drei Modelle. |
| VFS/Dateisystem | `vfs.rs` + `atcfs.rs` (Kernel) | `system/vfs` (2715, Journal/GPT) | `service_space/hw_drivers` (Block) | Kein Syscall erreicht eine davon. |
| Netz | `service_space/*net*` (~12k) | `system/net` (222 Policy) | `modules/atc-globus-net` (0 Nutzer) | Nur die erste hat Code, keine läuft. |
| Geräte | `pci_scan.rs` + `boot_devices.rs` | `service_space/device_registry.rs` (1003) | `system/devices` (2928) + `dma_auth.rs` | Vier Orte für Geräteautorität. |
| Services | `services.rs` | `system/services` (34) | `service_space/service_init.rs` | Kein laufender Manager. |
| Diagnose | `diagnostics.rs` (Kernel) | `system/diagnostics` (178) | Auditlog in `security.rs` | Drei Meldewege, keiner abfragbar. |
| Entropie | `entropy_gate.rs` | `system/entropy-core` | `service_space/identity_key_hal` | Keine im Pfad. |
| Boot-Tabelle | Kernel-Crate im Repo | Spiegel `a-townchain/kernel/globus-os` (Drift seit 08.10.) | — | Zwei Kernel-Wahrheiten (bereits im Audit vom 09.11. belegt). |

## 4. Konkrete Integrationsabhängigkeiten

Die Reihenfolge ist nicht Geschmack, sondern Kausalität: jede Zeile blockiert die folgenden.

1. **ABI-Erweiterung** (Speicher-Mapping, VFS-Operationen, IRQ-ACK, `SpawnProcess` auf `0x0060`) blockiert VFS, Netz, Gerätedienste, Service Manager und jede Nutzer-Dienstkette. Heute gibt es 9 Syscalls und keinen einzigen Datei- oder Speicher-Aufruf.
2. **Service Space ins Image** (eigener `_start`, Ramdisk-Eintrag, Zyklus `service_space → kernel` auflösen) blockiert Device Manager, Netz, Blockchain-Dienste, Identität/Wallet. Ohne das bleibt jeder Dienst `OFF_IMAGE`.
3. **Entropie im Pfad** (`entropy_gate` oder `entropy-core` an den Boot anschließen) blockiert Secure Boot, Schlüsselerzeugung und TLS im Nutzerraum. Nach dem `rand`-Entfernen ist das eine offene P0-Lücke, keine Aufräumfrage.
4. **Eine Capability-Autorität** (drei Kernel-Loci zusammenführen, `system/security` als Policy statt zweiter Prüfung) ist Voraussetzung für Audit, Remote-Caps und jede belastbare Aussage über Rechte.
5. **Geräteautorität an einer Stelle** (PCI-Vorrat → Registratur → Cap-Vergabe → DMA-Policy als eine Kette) blockiert Treiberbindung und Hotplug.
6. **SMP** blockiert Skalierung und jede Form von Recovery/Watchdog mit zweitem Kern; heute ist alles single-core.

## 5. Abgleich mit der Architekturgrenze

Die sieben verbindlichen Kernverantwortlichkeiten sind im Code vorhanden: HAL, Adressraumisolation, Scheduling, IPC-Mechanismus, Capability-Enforcement, Interrupt/Exception, Boot-Mechanik. Abweichungen:

- **Kernel-Integrität** ist die einzige der sieben, die faktisch fehlt (nur Anmelde-Prüfung, keine Messung).
- **Vier Grenzverletzungen liegen schon im Code**, alle als Init-Kette statt als Dienst: `vfs.rs`/`atcfs.rs`, `security.rs` (MultiSig/Audit/Reputation/Rate-Limit), `ipc.rs`/`ipc_lifecycle.rs`, `lkm.rs`. Sie sind nicht die Ursache der aktuellen Blockade, aber sie sind der Ort, an dem die Microkernel-Grenze später aufweicht.
- **`network` und `iommu` sind korrekt getrennt** (kein Netzcode im Kernel, DMA nur als Policy). Das ist der Teil, der die Architektur bereits stützt.

**Empfehlung, ohne neue Funktionalität:** die vier Init-Ketten-Module als Vertragstest-Fixtures kennzeichnen (`cfg(test)` oder `#[doc(hidden)]` + Modul-Doku „nicht im Hardware-Pfad“), damit Code und Architekturdokument nicht auseinanderlaufen. Reihenfolge der nächsten technischen Schritte: ABI-Erweiterung → Service Space ins Image → Entropie in den Pfad → Capability-Konsolidierung.
