// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ShivaCore — Interrupt Descriptor Table + PIC-Remapping.
// K-Sprint 1: Breakpoint-, Double-Fault- und Page-Fault-Handler; die beiden
// klassischen 8259-PICs werden von ihren BIOS-Default-Vektoren (0x08-0x0F,
// 0x70-0x77 -- kollidieren mit CPU-Exceptions) auf 0x20-0x2F umgemappt.

use crate::gdt;
use crate::serial_println;
use core::arch::global_asm;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use lazy_static::lazy_static;
use libshivacore::CapabilityHandle;
use pic8259::ChainedPics;
use shivacore::ats1000::Pid;
use shivacore::syscall::SyscallRequest;
use shivacore::syscall_gate::SyscallGate;
use shivacore::user_sched::{SavedContext, UserScheduler};
use shivacore::userspace::UserContext;
use spin::Mutex;
use x86_64::VirtAddr;
use x86_64::registers::rflags::RFlags;
use x86_64::structures::idt::{
    InterruptDescriptorTable, InterruptStackFrame, InterruptStackFrameValue, PageFaultErrorCode,
};

/// Erster Vektor der ersten PIC-Bank; ab hier liegen die Geraetevektoren, nicht die Ausnahmen.
pub const PIC_1_OFFSET: u8 = 0x20;
/// Erster Vektor der zweiten PIC-Bank, acht Vektoren nach der ersten.
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

/// Beide verketteten PIC-Banken hinter einer Sperre.
pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

lazy_static! {
    /// Modifikatorzustand der PS/2-Tastatur. Nur der IRQ-Handler greift zu.
    static ref KEY_STATE: Mutex<shivacore::keymap::ModifierState> =
        Mutex::new(shivacore::keymap::ModifierState {
            shift: false,
            caps_lock: false,
            extended: false,
        });
}

lazy_static! {
    /// Die Routing-Tabelle lebt im Kernel, nicht im Bootpfad. Vorher war sie
    /// eine lokale Variable in `main`: nach dem Boot wusste niemand mehr, welche
    /// Line auf welchem Vektor liegt. Sie wird waehrend des Boots befuellt (dort
    /// auch zum ersten Mal initialisiert), danach greift nur der IRQ-Pfad zu.
    static ref DEVICE_ROUTES: Mutex<shivacore::irq_routing::IrqRouting> =
        Mutex::new(shivacore::irq_routing::IrqRouting::new());
}

/// Traegt eine Line mit vorgegebenem Vektor ein (Timer, dessen Vektor im IDT steht).
pub fn fix_route(
    irq: u8,
    gsi: u32,
    vector: u8,
) -> Result<shivacore::irq_routing::IrqRoute, shivacore::irq_routing::RouteError> {
    DEVICE_ROUTES.lock().assign_fixed(irq, gsi, vector)
}

/// Vergibt den naechsten freien Geraetevektor an eine Line.
pub fn route_line(
    irq: u8,
    gsi: u32,
) -> Result<shivacore::irq_routing::IrqRoute, shivacore::irq_routing::RouteError> {
    DEVICE_ROUTES.lock().assign_next_free(irq, gsi)
}

/// Weg zurueck fuer den Handler, der nur den Vektor kennt.
pub fn line_for_vector(vector: u8) -> Option<shivacore::irq_routing::IrqRoute> {
    DEVICE_ROUTES.lock().route_for_vector(vector)
}

/// Naechster freier Geraetevektor, ohne einen zu belegen.
pub fn next_free_vector() -> Option<u8> {
    DEVICE_ROUTES.lock().next_free_vector()
}

/// Anzahl der vergebenen Geraetelinien.
pub fn routed_lines() -> usize {
    DEVICE_ROUTES.lock().len()
}

/// Maskenebene fuer die Bruecke: benutzt dieselben APIC-Funktionen, die auch
/// der direkte Pfad nimmt. Nur aktiv, wenn ueber den LAPIC quittiert wird.
pub struct KernelMasking;

impl shivacore::irq_bridge::IrqMasking for KernelMasking {
    fn mask_vector(&mut self, vector: u32) -> Result<(), shivacore::apic::ApicError> {
        if !(32..=255).contains(&vector) {
            return Err(shivacore::apic::ApicError::ReservedVector);
        }
        if shivacore::apic::mask_interrupt_vector(vector as u8) {
            Ok(())
        } else {
            Err(shivacore::apic::ApicError::ReservedVector)
        }
    }

    fn unmask_vector(&mut self, vector: u32) -> Result<(), shivacore::apic::ApicError> {
        if !(32..=255).contains(&vector) {
            return Err(shivacore::apic::ApicError::ReservedVector);
        }
        if shivacore::apic::unmask_interrupt_vector(vector as u8) {
            Ok(())
        } else {
            Err(shivacore::apic::ApicError::ReservedVector)
        }
    }
}

lazy_static! {
    /// Die Bruecke aus SC-006, erstmals im echten Pfad benutzt.
    static ref IRQ_BRIDGE: Mutex<shivacore::irq_bridge::IrqBridge<KernelMasking>> =
        Mutex::new(shivacore::irq_bridge::IrqBridge::new(KernelMasking));
    /// Tabelle, in der die Abo-Capability wirklich liegt, samt ihrer Id.
    ///
    /// Vorher habe ich die Tabelle nach dem Anlegen verworfen und nur eine
    /// Kopie der Capability behalten. Eine Capability ausserhalb ihrer Tabelle
    /// laesst sich nicht widerrufen, ihre Abstammung kennt niemand, und
    /// `revoke` haette nie etwas gefunden. Jetzt ist die Tabelle der Ort, an dem
    /// die Capability lebt; der Handler holt sie bei jedem ACK dort ab.
    static ref KERNEL_CAPS: Mutex<shivacore::kernel_caps::KernelCaps> =
        Mutex::new(shivacore::kernel_caps::KernelCaps::new());
}

/// Abonniert eine Line mit einer Kernel-Capability (Eigentuemer PID 0).
/// Muss vor dem Freigeben des IOAPIC-Eintrags laufen, sonst kommt der erste
/// Interrupt ohne Eintrag in der Tabelle.
pub fn subscribe_kernel_line(line: u32) -> Result<u32, &'static str> {
    // Kurze kritische Sektion mit Interrupts aus.
    //
    // Ohne diese Sperre blieb der Boot hier reproduzierbar stehen: genau
    // nach "IRQ 1 -> GSI 1 -> Vektor 0x30" und vor "Line 1 abonniert", ohne
    // Panic, ohne weitere Zeile. Ein Timer-IRQ im Fenster zwischen den beiden
    // Sperren trifft auf einen halb fertigen Zustand — und ein Haenger, der
    // von der Laenge eines Logsatzes abhaengt, ist kein Zufall, sondern ein
    // Zeitfenster. Zwei Sperren, keine Ausgabe, kein Warten: das Fenster
    // schliessen ist billiger als es zu erklaeren.
    let were_enabled = x86_64::instructions::interrupts::are_enabled();
    x86_64::instructions::interrupts::disable();

    let result = (|| {
        // Anlegen in der Tabelle, damit die Capability widerrufbar bleibt.
        let cap = KERNEL_CAPS
            .lock()
            .grant(line)
            .map_err(|_| "Kernel-Capability fuer die Line abgelehnt")?;
        let mut bridge = IRQ_BRIDGE.lock();
        let line = bridge
            .subscribe(&cap)
            .map_err(|_| "Line bereits abonniert oder Rechte fehlen")?;
        drop(bridge);
        Ok(line)
    })();

    if were_enabled {
        x86_64::instructions::interrupts::enable();
    }
    result
}

/// Zaehlt Zustellungen, die ueber die Bruecke liefen und solche, die im
/// Rueckfall direkt gesperrt wurden. Beide Zahlen im Log machen sichtbar, ob
/// der Pfad wirklich ueber die Tabelle geht.
pub static BRIDGED_DELIVERIES: AtomicU64 = AtomicU64::new(0);
/// Zustellungen, die nicht ueber die Tabelle liefen, sondern im Rueckfall direkt gesperrt wurden.
pub static DIRECT_FALLBACKS: AtomicU64 = AtomicU64::new(0);
/// Eingabewarteschlange zwischen Handler und Konsument (dem Tick).
///
/// Der Handler legt nur ab, was er gelesen hat. Formatierung und Serienausgabe
/// macht der Tick: Arbeit, die nicht in den Tastatur-Interrupt gehoert.
pub static INPUT_QUEUE: Mutex<shivacore::input_queue::InputQueue> =
    Mutex::new(shivacore::input_queue::InputQueue::new());

/// Wie viele Ereignisse der Konsument insgesamt abgeholt hat.
pub static INPUT_CONSUMED: AtomicU64 = AtomicU64::new(0);

/// Wie oft der Syscall einem Ring-3-Prozess ein Tastenereignis ausgehaendigt hat.
pub static INPUT_DELIVERED: AtomicU64 = AtomicU64::new(0);
/// Verpasste ACK-Fristen (SC-006): sichtbar, auch wenn sie nie auftreten.
pub static ACK_TIMEOUTS: AtomicU64 = AtomicU64::new(0);

/// Sobald ein Prozess den Eingabekanal haelt, ist er der Konsument. Der Tick
/// darf dann nicht mehr abraeumen: zwei Leser auf einer Warteschlange nehmen
/// sich gegenseitig die Ereignisse weg (im ersten Lauf kam 'a' nie beim
/// Prozess an, weil der Tick schneller war).
pub static INPUT_CONSUMER_USERSPACE: AtomicBool = AtomicBool::new(false);

/// Zustellungen, die auf eine offene (nie quittierte) Zustellung trafen.
pub static STUCK_DELIVERIES: AtomicU64 = AtomicU64::new(0);

/// Zaehlt jeden zugestellten Timer-Tick. Dient der Messung, ob ein
/// Zustellweg Ticks verliert.
pub static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

/// Merkt je Ring-3-Smoke-Prozess, ob sein Register-Muster schon einmal
/// beobachtet wurde (Slot 0 = PID 1000, Slot 1 = PID 1001).
/// Wie viele Syscalls der Gate insgesamt bedient hat.
pub static SYSCALL_ENTRIES: AtomicU64 = AtomicU64::new(0);

lazy_static! {
    /// Zaehler ueber alle Prozessorfehler seit dem Start.
    static ref FAULT_COUNTERS: Mutex<shivacore::fault::FaultCounters> =
        Mutex::new(shivacore::fault::FaultCounters::new());
}

static PATTERN_ESTABLISHED: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];

/// Ob fuer einen Platz ueberhaupt ein Register-Muster erwartet wird.
///
/// Das Muster gehoert zur eingebauten Nutzlast, nicht zum Prozessplatz: PID 1000
/// fuehrt den Register-Smoke-Test nur, wenn keine Ramdisk mitgegeben wurde. Mit
/// einem echten Programm setzt niemand RAX/RBX/R12, und eine Pruefung, die nie
/// greifen kann, ist schlimmer als keine: sie schreibt bei jedem Tick eine
/// Aufwaermzeile und beweist nichts.
/// Vorgabe ist AN: eine Pruefung, die still ausfaellt, weil jemand den Aufruf
/// vergisst, waere schlimmer als eine, die zu viel prueft. Abgeschaltet wird
/// ausdruecklich und mit Logzeile, wenn eine Nutzlast keine Musterregister
/// setzt.
static PATTERN_EXPECTED: [AtomicBool; 2] = [AtomicBool::new(true), AtomicBool::new(true)];

/// Legt fest, ob fuer diesen Prozess ein Register-Muster erwartet wird.
///
/// Der Bootpfad entscheidet das, weil nur er weiss, welche Nutzlast in dem
/// Prozess laeuft. Aufruf vor dem ersten Ring-3-Eintritt.
pub fn expect_register_pattern(pid: u32, expected: bool) {
    let slot = match pid {
        1000 => 0usize,
        1001 => 1usize,
        _ => return,
    };
    PATTERN_EXPECTED[slot].store(expected, Ordering::Release);
    if !expected {
        serial_println!(
            "ShivaCore: SCHED-001 Musterpruefung fuer PID {} abgeschaltet (Nutzlast setzt keine Musterregister)",
            pid
        );
    }
}

static USER_SCHEDULER: Mutex<Option<UserScheduler>> = Mutex::new(None);

/// Der einzige Userspace->Kernel-Pfad: Dispatcher + Capability-Tabelle.
static SYSCALL_GATE: Mutex<Option<SyscallGate>> = Mutex::new(None);

/// Hinterlegt die Seitentabelle eines Prozesses im laufenden Scheduler.
/// Setzt die Seitentabelle eines bereits eingetragenen Prozesses nach.
///
/// Das Aufziehen benutzt diesen Weg nicht mehr (dort wird alles vor der
/// Veroeffentlichung gesetzt); er bleibt fuer spaetere Umschaltungen etwa nach
/// einem Programmwechsel.
#[allow(dead_code)]
pub fn set_process_cr3(pid: crate::ats1000::Pid, cr3: u64) {
    if let Some(scheduler) = USER_SCHEDULER.lock().as_mut() {
        scheduler.set_cr3(pid, cr3);
    }
}

/// Setzt den Userspace-Planer mit bis zu vier Kontexten und ihren Seitentabellen auf.
pub fn init_user_scheduler(
    first: &UserContext,
    second: &UserContext,
    third: &UserContext,
    fourth: Option<&UserContext>,
    cr3_first: (crate::ats1000::Pid, u64),
    cr3_second: (crate::ats1000::Pid, u64),
    cr3_third: (crate::ats1000::Pid, u64),
    cr3_fourth: (crate::ats1000::Pid, u64),
) {
    let mut scheduler = UserScheduler::new();
    scheduler.add_process(first.pid, first, 0);
    scheduler.add_process(second.pid, second, 0);
    scheduler.add_process(third.pid, third, 0);
    // Der vierte Platz ist der zweite Programmplatz aus der Ramdisk. Er ist
    // optional: ein Image ohne Verzeichnis hat nur drei Prozesse.
    if let Some(fourth) = fourth {
        scheduler.add_process(fourth.pid, fourth, 0);
    }
    // Jede eigene Tabelle eintragen, BEVOR der Scheduler sichtbar wird. Ein
    // Tick, der zwischen Eintragung und CR3-Wert plant, wuerde einen Prozess
    // ohne Tabelle auswaehlen; deshalb gibt es dieses Fenster nicht mehr.
    for (pid, cr3) in [cr3_first, cr3_second, cr3_third] {
        scheduler.set_cr3(pid, cr3);
    }
    if fourth.is_some() {
        scheduler.set_cr3(cr3_fourth.0, cr3_fourth.1);
    }
    let _ = scheduler.schedule(None);
    *USER_SCHEDULER.lock() = Some(scheduler);
    {
        let mut gate = SYSCALL_GATE.lock();
        if gate.is_none() {
            *gate = Some(SyscallGate::new());
        }
    }
    // Klammern um die Arme: `serial_println!` endet intern mit einem Semikolon
    // und ist damit kein Ausdruck, den ein Match-Arm direkt zurueckgeben kann.
    match fourth {
        Some(fourth) => {
            serial_println!(
                "ShivaCore: SCHED-001 scheduler armed: PID={} + PID={} + PID={} + PID={} (Ring 3)",
                first.pid.0,
                second.pid.0,
                third.pid.0,
                fourth.pid.0
            );
        }
        None => {
            serial_println!(
                "ShivaCore: SCHED-001 scheduler armed: PID={} + PID={} + PID={} (Syscall-Schleife)",
                first.pid.0,
                second.pid.0,
                third.pid.0
            );
        }
    }
}

/// Gibt einem Prozess das Leserecht auf den Kernel-Eingabekanal und liefert die
/// Cap-Nummer zurueck, die der Prozess im Cap-Slot-Register braucht.
///
/// Muss laufen, bevor ein Payload gebaut wird, in dem die Nummer als Immediate
/// steckt: die Nummer vergibt die Tabelle, nicht der Payload.
/// Installiert die eine Capability Authority im Live-Gate.
///
/// Vorher baute sich das Gate beim ersten Syscall seine eigene Tabelle; der
/// Prozessmanager und der VFS hatten je eine weitere. Damit entschied keine
/// Instanz allein. Der Kernel ruft diese Funktion direkt nach
/// `KernelState::boot()` auf und uebergibt seine Authority.
pub fn install_authority(
    authority: alloc::sync::Arc<shivacore::cap_authority::CapabilityAuthority>,
) -> usize {
    let identity = authority.identity();
    let mut guard = SYSCALL_GATE.lock();
    *guard = Some(SyscallGate::with_authority(authority));
    identity
}

/// Identitaet der im Live-Gate installierten Authority (Adresse der Instanz).
pub fn authority_identity() -> Option<usize> {
    SYSCALL_GATE
        .lock()
        .as_ref()
        .map(|gate| gate.authority().identity())
}

/// Anzahl der Capabilities eines Prozesses, gelesen ueber das Live-Gate.
pub fn cap_count_for(pid: Pid) -> usize {
    SYSCALL_GATE
        .lock()
        .as_ref()
        .map(|gate| gate.cap_count_for(pid))
        .unwrap_or(0)
}

/// Anzahl der beim Boot aktivierten CPUs. Die Freigabe darf nur stattfinden,
/// wenn kein anderer Prozessor den Adressraum noch benutzt; ohne IPI-Shootdown
/// ist das nur bei einer CPU sicher.
static ONLINE_CPUS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(1);

/// Setzt die Zahl der aktiven Prozessoren, mindestens einen, und gibt sie an die Freigabe weiter.
pub fn set_online_cpus(count: usize) {
    ONLINE_CPUS.store(count.max(1), Ordering::Release);
    shivacore::reclaim::set_online_cpus(count);
}

/// Anzahl der beim Start aktivierten Prozessoren.
pub fn online_cpus() -> usize {
    ONLINE_CPUS.load(Ordering::Acquire)
}

/// Setzt einen Dienst auf seinen Startzustand zurueck.
///
/// Der Prozess laeuft in diesem Moment: sein Trap hat uns hierher gebracht,
/// seine Seitentabelle ist aktiv. Deshalb sind Stapel und Datensegment ueber
/// die virtuelle Adresse erreichbar und muessen nicht eigens abgebildet werden.
/// Zurueckgesetzt werden Stapel, Startvertrag, Register und — seit dem
/// 14.11.2026 — das Datensegment aus dem hinterlegten Startabbild. Nicht
/// angefasst wird der Haufen; die Capabilities bleiben, sonst waere der Dienst
/// nach dem Neustart rechtlos.
fn reset_service_process(frame: &mut UserTrapFrame, service: &crate::services::Service) {
    let start = service.start_rsp;
    let end = start.saturating_add(16);
    if start >= service.stack_low && end <= service.stack_low.saturating_add(0x0000_8000) {
        unsafe {
            core::ptr::write_bytes(
                service.stack_low as *mut u8,
                0,
                (end - service.stack_low) as usize,
            );
            // Startvertrag exakt wie beim Boot: erst Konsole, dann Eingabe.
            core::ptr::write_volatile(start as *mut u64, service.caps[0]);
            core::ptr::write_volatile((start as *mut u64).add(1), service.caps[1]);
        }
    }
    // Datensegment zurueck auf den Startzustand. Der Dienst laeuft in diesem
    // Trap, seine Seitentabelle ist aktiv, die Seiten sind ueber die virtuelle
    // Adresse erreichbar; es wird nichts abgebildet und nichts angefordert.
    match crate::services::restore_image(service.pid) {
        Some(bytes) => serial_println!(
            "ShivaCore: Dienst '{}' Daten zurueckgesetzt ({} Bytes aus dem Startabbild)",
            service.name,
            bytes
        ),
        None => serial_println!(
            "ShivaCore: Dienst '{}' ohne Datenabbild — Datensegment bleibt stehen",
            service.name
        ),
    }

    frame.rax = 0;
    frame.rbx = 0;
    frame.rcx = 0;
    frame.rdx = 0;
    frame.rsi = 0;
    frame.rdi = 0;
    frame.rbp = start;
    frame.r8 = 0;
    frame.r9 = 0;
    frame.r10 = 0;
    frame.r11 = 0;
    frame.r12 = 0;
    frame.r13 = 0;
    frame.r14 = 0;
    frame.r15 = 0;
    frame.rip = service.entry_rip;
    frame.rsp = start;
    // Nur die Pflichtbits: Bit 1 ist immer gesetzt, IF (Bit 9) muss stehen,
    // sonst bekaeme der Dienst keine Ticks mehr und das Fenster bliebe stumm.
    frame.rflags = 0x202;
}

/// Beendet den aufrufenden Prozess aus einem Syscall heraus (P1-3B, echter
/// Prozess-Exit).
///
/// Reihenfolge ist bindend: Capabilities widerrufen, TLB leeren, Prozess auf
/// Zombie setzen, erst dann auf den naechsten umschalten. Das Reaping und die
/// Frame-Freigabe passieren im Timer-Tick — der Syscall-Pfad schaltet keinen
/// Speicher frei.
fn exit_current_process(frame: &mut UserTrapFrame, pid: Pid, exit_code: i32) {
    // Ein Dienst verschwindet nicht, er kehrt an seinen Startzustand zurueck.
    // Das passiert VOR dem Widerruf der Capabilities: danach haette der neue
    // Lauf keine Rechte mehr, und ein Neustart ohne Rechte waere ein Dienst,
    // der nichts tun kann.
    if let Some(service) = crate::services::restart(u64::from(pid.0)) {
        reset_service_process(frame, &service);
        serial_println!(
            "ShivaCore: Dienst '{}' PID={} neu gestartet (Versuch {}/{}) — Stapel geleert, Startvertrag neu geschrieben, Capabilities bleiben",
            service.name,
            pid.0,
            service.attempts,
            service.limit
        );
        return;
    }
    let revoked = cleanup_process(pid);
    // Nach dem Wechsel weg von dieser Seitentabelle sind ihre Eintraege im TLB
    // wertlos; der Wechsel selbst leert die nicht-globalen Eintraege. Das
    // explizite Flush macht die Annahme sichtbar statt implizit.
    x86_64::instructions::tlb::flush_all();
    shivacore::security::record(
        "kernel",
        "process.exit",
        &alloc::format!(
            "pid={:?} code={} caps_widerrufen={}",
            pid,
            exit_code,
            revoked
        ),
        shivacore::security::AuditResult::Allowed,
    );
    serial_println!(
        "ShivaCore: Exit-Syscall PID={:?} Code={} — {} Capabilities widerrufen, TLB geleert",
        pid,
        exit_code,
        revoked
    );

    let next = {
        let mut guard = USER_SCHEDULER.lock();
        guard
            .as_mut()
            .and_then(|scheduler| scheduler.exit_process(pid, exit_code, None))
    };
    match next {
        Some((next_pid, next)) => {
            let cr3 = {
                let mut guard = USER_SCHEDULER.lock();
                guard.as_ref().and_then(|s| s.cr3_of(next_pid))
            };
            if let Some(cr3) = cr3 {
                shivacore::addr_space::install_raw(cr3);
            }
            frame.apply_saved_context(&next);
            serial_println!(
                "ShivaCore: nach Exit von PID={:?} laeuft PID={} weiter",
                pid,
                next_pid.0
            );
        }
        None => {
            serial_println!(
                "ShivaCore: Exit von PID={:?} — kein lauffaehiger Prozess mehr, Kernel haelt an",
                pid
            );
            loop {
                x86_64::instructions::hlt();
            }
        }
    }
}

/// Reiht die Seitentabellen-Frames eines beendeten Prozesses zur Freigabe ein.
fn engine_reclaim_queue(pid: Pid) {
    let queued = shivacore::reclaim::RECLAIMER.queue_process(pid);
    if queued > 0 {
        serial_println!(
            "ShivaCore: Frame-Freigabe vorgemerkt: PID={:?} {} Frames",
            pid,
            queued
        );
    }
}

/// Selbsttest des Zombie-Reapings: beendet einen synthetischen Prozess und
/// sammelt ihn ein. Belegt, dass der Pfad im Kernel-Binary existiert und wirkt.
pub fn self_test_reap() -> (usize, usize, usize) {
    use shivacore::userspace::{PrivilegeLevel, UserAddressSpace, UserContext};

    let mut scheduler = UserScheduler::new();
    let pid = Pid(0x0C1E);
    let context = UserContext {
        pid,
        privilege: PrivilegeLevel::User,
        rip: 0x40_0000,
        rsp: 0x7FFF_F000,
        rbp: 0x7FFF_F000,
        rax: 0,
        cs: 0x1B,
        ss: 0x23,
        rflags: 0x202,
        addr_space: UserAddressSpace::default(),
        exit_code: None,
    };
    scheduler.add_process(pid, &context, 0);
    let before = scheduler.entries().len();
    scheduler.exit_process(pid, 0, None);
    let zombies = scheduler
        .entries()
        .iter()
        .filter(|entry| entry.state.is_zombie())
        .count();
    scheduler.reap_zombies();
    (before, zombies, scheduler.entries().len())
}

/// Exit-Cleanup: widerruft alle Capabilities eines beendeten Prozesses am
/// Syscall-Gate und leert die TLB.
///
/// Vorher rief der Fehlerpfad nur `exit_process` auf: der Prozess war Zombie,
/// seine Capabilities blieben aber gueltig. Der Test-Harness hatte
/// `close_all_for` fuer Kanaele, im Live-Pfad lief nichts davon.
///
/// Bewusst am Gate und nicht in `user_sched`: die Tabelle, die ueber Syscalls
/// entscheidet, ist die Tabelle, die aufgeraeumt werden muss. Rueckgabe ist die
/// Anzahl der widerrufenen Capabilities.
pub fn cleanup_process(pid: Pid) -> usize {
    let revoked = {
        let mut guard = SYSCALL_GATE.lock();
        match guard.as_mut() {
            Some(gate) => gate.revoke_all_for(pid),
            None => 0,
        }
    };
    // P1-3C: Kanaele und Handles gehoeren zum Prozess wie seine Capabilities.
    // Ein Kanal wird nur zerstoert, wenn dieser Prozess der letzte Teilnehmer
    // war; sonst faellt nur seine Referenz weg.
    let ipc = shivacore::ipc_lifecycle::release_process(pid);
    if ipc.channels_attempted > 0 || ipc.handles_revoked > 0 {
        shivacore::security::record(
            "kernel",
            "ipc.release",
            &alloc::format!(
                "pid={:?} kanaele_zerstoert={} kanaele_weitergereicht={} nachrichten_verworfen={} wartende_abgebrochen={}",
                pid,
                ipc.channels_destroyed,
                ipc.channels_released,
                ipc.messages_discarded,
                ipc.waiters_aborted
            ),
            shivacore::security::AuditResult::Allowed,
        );
        shivacore::security::record(
            "kernel",
            "handle.revoke",
            &alloc::format!(
                "pid={:?} handes_widerrufen={} veraltet_abgewiesen={}",
                pid,
                ipc.handles_revoked,
                ipc.stale_handles_denied
            ),
            shivacore::security::AuditResult::Allowed,
        );
        serial_println!(
            "ShivaCore: IPC-Cleanup PID={:?}: {} Kanaele geprueft, {} zerstoert, {} weitergereicht, {} Nachrichten verworfen, {} Wartende abgebrochen",
            pid,
            ipc.channels_attempted,
            ipc.channels_destroyed,
            ipc.channels_released,
            ipc.messages_discarded,
            ipc.waiters_aborted
        );
        serial_println!(
            "ShivaCore: Handle-Cleanup PID={:?}: {} Handles widerrufen, {} veraltete Zugriffe abgewiesen",
            pid,
            ipc.handles_revoked,
            ipc.stale_handles_denied
        );
        let (open_ch, shared_ch, messages) = shivacore::ipc_lifecycle::CHANNELS.lock().snapshot();
        serial_println!(
            "ShivaCore: IPC nach Exit: {} Kanaele offen, davon {} geteilt, {} Nachrichten erhalten, Doppelschliessen abgewiesen={}",
            open_ch,
            shared_ch,
            messages,
            ipc.double_close_blocked
        );
    }
    // Die Bildflaeche gehoert nicht dem Prozess, sondern dem System: endet der
    // Inhaber, schreibt der Kernel wieder. Ohne diesen Schritt blieb "gehoert
    // dem Userspace" stehen und der Bildschirm war nach dem letzten Absturz
    // fuer immer tot.
    if crate::framebuffer::take_back_from(u64::from(pid.0)) {
        crate::framebuffer::clear_screen();
        crate::framebuffer::write_text("GlobusOS: Konsole zurueck beim Kernel (Dienst beendet)\n");
        shivacore::security::record(
            "kernel",
            "console.reclaimed",
            &alloc::format!("pid={:?}", pid),
            shivacore::security::AuditResult::Allowed,
        );
        serial_println!(
            "ShivaCore: Konsole zurueck beim Kernel (PID={:?} hat sie nicht mehr)",
            pid
        );
    }
    // TLB leeren: die Rechte sind weg, die uebersetzten Eintraege duerfen nicht
    // als Rest im Adressuebersetzer stehen bleiben.
    x86_64::instructions::tlb::flush_all();
    shivacore::security::record(
        "kernel",
        "process.exit",
        &alloc::format!("pid={:?} revoked_caps={}", pid, revoked),
        shivacore::security::AuditResult::Allowed,
    );
    revoked
}

/// Selbsttest fuer das Exit-Cleanup, ohne laufende Prozesse zu stoeren:
/// legt fuer eine nie geplante PID zwei Capabilities an, raeumt auf und
/// zaehlt vorher/nachher.
pub fn self_test_cleanup() -> (usize, usize, usize) {
    let probe = Pid(9999);
    let mut guard = SYSCALL_GATE.lock();
    let gate = guard.get_or_insert_with(SyscallGate::new);
    let _ = gate.grant_channel_read(probe, 0xDEAD);
    let _ = gate.grant_channel_read(probe, 0xBEEF);
    let before = gate.cap_count_for(probe);
    let revoked = gate.revoke_all_for(probe);
    let after = gate.cap_count_for(probe);
    (before, revoked, after)
}

/// Gibt einem Prozess Schreibrecht auf die Systemkonsole und liefert die
/// Cap-Nummer, die das Programm im Argumentblock einsetzen muss.
///
/// Die Konsole ist eine Ressource wie jede andere: ohne diese Cap gibt es kein
/// Schreibrecht, und der Syscall antwortet mit `PermissionDenied`.
pub fn grant_console(pid: Pid) -> Option<u64> {
    let mut guard = SYSCALL_GATE.lock();
    let gate = guard.get_or_insert_with(SyscallGate::new);
    let cap = gate.authority().create(
        pid,
        shivacore::capability::ResourceType::Console,
        shivacore::syscall::CONSOLE_RESOURCE_ID,
        shivacore::capability::Rights::WRITE,
    );
    Some(cap.0)
}

/// Behandelt den Console-Syscall: Rechte, Kopie aus dem Prozessspeicher, Ausgabe.
///
/// Der Aufruf liegt hier und nicht im Dispatcher, weil er den Bildschirm und
/// den Adressraum des Aufrufers braucht. Die Pruefreihenfolge ist bindend:
/// erst die Capability, dann die Puffergrenze, dann das Prozessabbild, dann die
/// Kopie, zuletzt die Ausgabe.
fn handle_console(pid: Pid, request: SyscallRequest) -> shivacore::syscall::SyscallResponse {
    use libshivacore::{AbiError, CONSOLE_MAX_BYTES, CONSOLE_OP_CLEAR, CONSOLE_OP_WRITE};
    use shivacore::capability::{CapId, ResourceType, Rights};
    use shivacore::syscall::SyscallResponse;

    let Some(handle) = request.capability else {
        return SyscallResponse::err(AbiError::PermissionDenied);
    };
    let permitted = {
        let mut guard = SYSCALL_GATE.lock();
        let gate = guard.get_or_insert_with(SyscallGate::new);
        let capabilities = gate.capabilities();
        capabilities
            .get(CapId(handle.0))
            .map(|cap| {
                cap.owner == pid
                    && cap.resource_type == ResourceType::Console
                    && cap.rights.has(Rights::WRITE)
            })
            .unwrap_or(false)
    };
    if !permitted {
        shivacore::security::record(
            "syscall-gate",
            "console.denied",
            &alloc::format!("pid={:?} cap={}", pid, handle.0),
            shivacore::security::AuditResult::Denied,
        );
        return SyscallResponse::err(AbiError::PermissionDenied);
    }

    match request.arg0 {
        CONSOLE_OP_CLEAR => {
            if crate::framebuffer::hand_over_to_userspace(u64::from(pid.0)) {
                serial_println!(
                    "ShivaCore: Konsole an Ring 3 uebergeben (PID={}); Kernel schreibt nur noch seriell",
                    pid.0
                );
            }
            crate::framebuffer::clear_screen();
            SyscallResponse::ok(0)
        }
        CONSOLE_OP_WRITE => {
            let len = request.buffer_len;
            if len == 0 || len > CONSOLE_MAX_BYTES {
                return SyscallResponse::err(AbiError::InvalidPayload);
            }
            let space = USER_SCHEDULER
                .lock()
                .as_ref()
                .and_then(|scheduler| scheduler.address_space(pid));
            let Some(space) = space else {
                return SyscallResponse::err(AbiError::InvalidPayload);
            };
            if !space.range_readable(request.buffer_ptr, len) {
                return SyscallResponse::err(AbiError::InvalidPayload);
            }
            // Der Bereich ist gegen das Prozessabbild geprueft und die Laenge
            // begrenzt: die Kopie bleibt im Fenster und im Stapelpuffer.
            let mut bytes = [0u8; CONSOLE_MAX_BYTES];
            unsafe {
                core::ptr::copy_nonoverlapping(
                    request.buffer_ptr as *const u8,
                    bytes.as_mut_ptr(),
                    len,
                );
            }
            // Ungueltiges UTF-8 wird nicht erfunden, sondern leer gelassen:
            // ein falsches Zeichen auf dem Bildschirm waere eine Behauptung.
            let text = core::str::from_utf8(&bytes[..len]).unwrap_or("");
            crate::framebuffer::write_text(text);
            SyscallResponse::ok(0)
        }
        _ => SyscallResponse::err(AbiError::InvalidPayload),
    }
}

/// Prueft ein Passwort aus dem Anmeldefenster.
///
/// Berechtigt ist nur der Besitzer der Konsole: das Anmeldefenster haelt die
/// Konsolen-Capability, ein beliebiges Programm nicht. Der Nachweis laeuft
/// ueber dieselbe Capability wie das Schreiben auf den Bildschirm.
fn handle_identity_check(pid: Pid, request: SyscallRequest) -> shivacore::syscall::SyscallResponse {
    use libshivacore::{AbiError, IDENTITY_MAX_PASSWORD_BYTES};
    use shivacore::capability::{CapId, ResourceType, Rights};
    use shivacore::syscall::SyscallResponse;

    let Some(handle) = request.capability else {
        return SyscallResponse::err(AbiError::PermissionDenied);
    };
    let permitted = {
        let mut guard = SYSCALL_GATE.lock();
        let gate = guard.get_or_insert_with(SyscallGate::new);
        let capabilities = gate.capabilities();
        capabilities
            .get(CapId(handle.0))
            .map(|cap| {
                cap.owner == pid
                    && cap.resource_type == ResourceType::Console
                    && cap.rights.has(Rights::WRITE)
            })
            .unwrap_or(false)
    };
    if !permitted {
        shivacore::security::record(
            "identity",
            "identity.denied",
            &alloc::format!("pid={:?} cap={}", pid, handle.0),
            shivacore::security::AuditResult::Denied,
        );
        return SyscallResponse::err(AbiError::PermissionDenied);
    }

    let length = request.buffer_len;
    if length == 0 || length > IDENTITY_MAX_PASSWORD_BYTES {
        return SyscallResponse::err(AbiError::InvalidPayload);
    }
    let space = USER_SCHEDULER
        .lock()
        .as_ref()
        .and_then(|scheduler| scheduler.address_space(pid));
    let Some(space) = space else {
        return SyscallResponse::err(AbiError::InvalidPayload);
    };
    if !space.range_readable(request.buffer_ptr, length) {
        return SyscallResponse::err(AbiError::InvalidPayload);
    }

    // Kopie in den Kernel, danach wird nur noch Kernspeicher gelesen.
    let mut password = [0u8; IDENTITY_MAX_PASSWORD_BYTES];
    unsafe {
        core::ptr::copy_nonoverlapping(
            request.buffer_ptr as *const u8,
            password.as_mut_ptr(),
            length,
        );
    }
    let started = unsafe { core::arch::x86_64::_rdtsc() };
    let outcome = crate::identity_gate::check(&password[..length]);
    let cycles = unsafe { core::arch::x86_64::_rdtsc() }.saturating_sub(started);
    // Das Passwort bleibt nicht im Kernspeicher liegen.
    password.fill(0);

    serial_println!(
        "ShivaCore: Anmeldepruefung: PID={} Ergebnis={:?} ({} TSC-Zyklen, {} Byte)",
        pid.0,
        outcome,
        cycles,
        length
    );
    shivacore::security::record(
        "identity",
        "identity.checked",
        &alloc::format!("pid={:?} outcome={:?}", pid, outcome),
        match outcome {
            shivacore::identity_check::CheckOutcome::Ok => {
                shivacore::security::AuditResult::Allowed
            }
            shivacore::identity_check::CheckOutcome::WrongPassword => {
                shivacore::security::AuditResult::Denied
            }
            _ => shivacore::security::AuditResult::Error,
        },
    );
    SyscallResponse::ok(outcome.code())
}

/// Gibt einem Prozess Leserecht auf den Kernel-Eingabekanal und merkt, dass der Userspace liest.
pub fn grant_input_read(pid: Pid) -> Option<u64> {
    let mut gate = SYSCALL_GATE.lock();
    // Das Gate frueh anlegen: der Payload braucht die Cap-Nummer, bevor der
    // Scheduler laeuft. `init_user_scheduler` ersetzt es deshalb nicht mehr.
    let gate = gate.get_or_insert_with(SyscallGate::new);
    let cap = gate.grant_channel_read(pid, shivacore::syscall::KERNEL_INPUT_CHANNEL);
    INPUT_CONSUMER_USERSPACE.store(true, Ordering::Release);
    Some(cap.0)
}

/// Feste Vektoren, die der Kernel selbst belegt.
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    /// Zeitgeber auf dem ersten programmierbaren Interrupt-Controller.
    Timer = PIC_1_OFFSET,
    /// Tastatur; der Vektor folgt direkt auf den Zeitgeber.
    Keyboard,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }
    fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        // Seitenfehler laufen ueber den Trampolin, nicht ueber einen
        // x86-interrupt-Handler: nur dort laesst sich der Rahmen aendern, und
        // nur so kann ein Fehler in Ring 3 den Prozess beenden statt sich zu
        // wiederholen.
        unsafe {
            idt.page_fault.set_handler_addr(fault_trampoline_addr());
            // Schutzverletzung bringt einen Fehlercode mit, also derselbe
            // Aufbau wie beim Seitenfehler — nur ein anderer Bericht.
            idt.general_protection_fault
                .set_handler_addr(general_protection_trampoline_addr());
            // Ungueltiger Opcode bringt keinen mit.
            idt.invalid_opcode
                .set_handler_addr(fault_trampoline_nocode_addr());
        }
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        unsafe {
            idt[InterruptIndex::Timer.as_u8()]
                .set_handler_addr(timer_interrupt_entry_addr())
        };
        idt[InterruptIndex::Keyboard.as_u8()].set_handler_fn(keyboard_interrupt_handler);
        // Zweite Line: die Tastatur kommt ueber den IOAPIC auf dem ersten
        // automatisch vergebenen Geraetevektor.
        idt[KEYBOARD_VECTOR].set_handler_fn(keyboard_interrupt_handler_at_device_vector);
        // User-mode software interrupt gate. DPL3 permits CPL3 to enter the kernel.
        // Volles Register-Gate statt Handler-Stub (Audit-Befund K-01).
        unsafe {
            idt[0x80]
                .set_handler_addr(syscall_gate_entry_addr())
                .set_privilege_level(x86_64::PrivilegeLevel::Ring3)
        };
        idt
    };
}

/// Laedt die Interrupt-Tabelle.
pub fn init_idt() {
    IDT.load();
}

/// Initialisiert beide PIC-Banken und gibt nur die Zeitgeberleitung frei, alle anderen bleiben gesperrt.
pub fn init_pics() {
    unsafe {
        PICS.lock().initialize();

        // UEFI firmware may leave IRQ0 masked in the legacy PIC. Make the
        // timer IRQ unmasked explicitly; keep all other legacy PIC IRQs masked
        // until their handlers and device initialization are ready.
        PICS.lock().write_masks(0xFE, 0xFF);

        // Program channel 0 explicitly so IRQ0 drives scheduler preemption
        // consistently in both BIOS and UEFI boots (100 Hz).
        const PIT_INPUT_HZ: u32 = 1_193_182;
        const PIT_HZ: u32 = 100;
        let divisor = (PIT_INPUT_HZ / PIT_HZ) as u16;
        let mut command: x86_64::instructions::port::Port<u8> =
            x86_64::instructions::port::Port::new(0x43);
        let mut channel0: x86_64::instructions::port::Port<u8> =
            x86_64::instructions::port::Port::new(0x40);
        // Modus 2 (Rate Generator) statt Modus 3 (Rechteck): Modus 2 erzeugt
        // genau eine Flanke pro Periode. Die Rechteckwelle aus Modus 3 hat zwei
        // Flanken, was bei kantengetriggerter Zustellung zu doppelten
        // Interrupts fuehrt und die Zeitrechnung beschleunigt.
        command.write(0x34); // channel 0, lobyte/hibyte, mode 2, binary
        channel0.write((divisor & 0x00ff) as u8);
        channel0.write((divisor >> 8) as u8);
    }
    x86_64::instructions::interrupts::enable();
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    serial_println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;
    serial_println!("EXCEPTION: PAGE FAULT");
    serial_println!("Accessed Address: {:?}", Cr2::read());
    serial_println!("Error Code: {:?}", error_code);
    serial_println!("{:#?}", stack_frame);
}

/// Registerstand beim Uebergang in den Kernel; die Reihenfolge entspricht dem Stapelbild.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct UserTrapFrame {
    /// Akkumulator und Rueckgabewert.
    pub rax: u64,
    /// Allzweckregister.
    pub rbx: u64,
    /// Allzweckregister.
    pub rcx: u64,
    /// Allzweckregister.
    pub rdx: u64,
    /// Quellzeiger bei Zeichenkettenbefehlen.
    pub rsi: u64,
    /// Zielzeiger bei Zeichenkettenbefehlen.
    pub rdi: u64,
    /// Rahmenzeiger.
    pub rbp: u64,
    /// Erweitertes Allzweckregister.
    pub r8: u64,
    /// Erweitertes Allzweckregister.
    pub r9: u64,
    /// Erweitertes Allzweckregister.
    pub r10: u64,
    /// Erweitertes Allzweckregister.
    pub r11: u64,
    /// Erweitertes Allzweckregister.
    pub r12: u64,
    /// Erweitertes Allzweckregister.
    pub r13: u64,
    /// Erweitertes Allzweckregister.
    pub r14: u64,
    /// Erweitertes Allzweckregister.
    pub r15: u64,
    /// Befehlszeiger beim Unterbrechungspunkt.
    pub rip: u64,
    /// Code-Segment des unterbrochenen Codes.
    pub cs: u64,
    /// Flag-Register beim Unterbruch.
    pub rflags: u64,
    /// Stapelzeiger vor dem Uebergang.
    pub rsp: u64,
    /// Stapel-Segment vor dem Uebergang.
    pub ss: u64,
}

/// CPU-Rahmen fuer Ausnahmen mit einheitlichem Fehlercode-Slot.
///
/// Anders als Timer-/Syscall-Rahmen enthaelt dieser Frame nach den 15
/// gesicherten GP-Registern immer einen Fehlercode. Bei #UD fuegt das
/// Trampolin einen synthetischen Nullcode ein. Damit liegen RIP/CS/RFLAGS/RSP/SS
/// fuer Page Fault, General Protection und Invalid Opcode an identischen Offsets.
#[repr(C)]
pub struct ExceptionTrapFrame {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub error_code: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl ExceptionTrapFrame {
    /// Ersetzt den fehlerhaften Kontext durch den naechsten Ring-3-Kontext.
    pub fn apply_saved_context(&mut self, ctx: &SavedContext) {
        self.rax = ctx.regs.rax;
        self.rbx = ctx.regs.rbx;
        self.rcx = ctx.regs.rcx;
        self.rdx = ctx.regs.rdx;
        self.rsi = ctx.regs.rsi;
        self.rdi = ctx.regs.rdi;
        self.rbp = ctx.regs.rbp;
        self.r8 = ctx.regs.r8;
        self.r9 = ctx.regs.r9;
        self.r10 = ctx.regs.r10;
        self.r11 = ctx.regs.r11;
        self.r12 = ctx.regs.r12;
        self.r13 = ctx.regs.r13;
        self.r14 = ctx.regs.r14;
        self.r15 = ctx.regs.r15;
        self.rip = ctx.iret.rip;
        self.cs = u64::from(ctx.iret.cs);
        self.rflags = ctx.iret.rflags;
        self.rsp = ctx.iret.rsp;
        self.ss = u64::from(ctx.iret.ss);
    }
}

impl UserTrapFrame {
    /// Schreibt einen gespeicherten Kontext in diesen Rahmen.
    ///
    /// Das Gegenstueck zu `saved_context`: nach der Rueckkehr ins Assembly
    /// stellt `iretq` den Prozess aus genau diesen Feldern her. Ohne diese
    /// Richtung koennte ein Handler den Prozess nicht wechseln, und eine
    /// Ausnahme liesse sich nicht beenden.
    pub fn apply_saved_context(&mut self, ctx: &SavedContext) {
        self.rax = ctx.regs.rax;
        self.rbx = ctx.regs.rbx;
        self.rcx = ctx.regs.rcx;
        self.rdx = ctx.regs.rdx;
        self.rsi = ctx.regs.rsi;
        self.rdi = ctx.regs.rdi;
        self.rbp = ctx.regs.rbp;
        self.rsp = ctx.regs.rsp;
        self.r8 = ctx.regs.r8;
        self.r9 = ctx.regs.r9;
        self.r10 = ctx.regs.r10;
        self.r11 = ctx.regs.r11;
        self.r12 = ctx.regs.r12;
        self.r13 = ctx.regs.r13;
        self.r14 = ctx.regs.r14;
        self.r15 = ctx.regs.r15;
        self.rip = ctx.iret.rip;
        self.cs = u64::from(ctx.iret.cs);
        self.rflags = ctx.iret.rflags;
    }

    fn saved_context(&self) -> SavedContext {
        SavedContext {
            regs: shivacore::user_sched::SavedRegisters {
                rax: self.rax,
                rbx: self.rbx,
                rcx: self.rcx,
                rdx: self.rdx,
                rsi: self.rsi,
                rdi: self.rdi,
                rbp: self.rbp,
                rsp: self.rsp,
                r8: self.r8,
                r9: self.r9,
                r10: self.r10,
                r11: self.r11,
                r12: self.r12,
                r13: self.r13,
                r14: self.r14,
                r15: self.r15,
            },
            iret: shivacore::user_sched::IretFrame {
                rip: self.rip,
                cs: self.cs as u16,
                rflags: self.rflags,
                rsp: self.rsp,
                ss: self.ss as u16,
            },
        }
    }

    fn restore(&mut self, ctx: SavedContext) {
        self.rax = ctx.regs.rax;
        self.rbx = ctx.regs.rbx;
        self.rcx = ctx.regs.rcx;
        self.rdx = ctx.regs.rdx;
        self.rsi = ctx.regs.rsi;
        self.rdi = ctx.regs.rdi;
        self.rbp = ctx.regs.rbp;
        self.r8 = ctx.regs.r8;
        self.r9 = ctx.regs.r9;
        self.r10 = ctx.regs.r10;
        self.r11 = ctx.regs.r11;
        self.r12 = ctx.regs.r12;
        self.r13 = ctx.regs.r13;
        self.r14 = ctx.regs.r14;
        self.r15 = ctx.regs.r15;
        self.rip = ctx.iret.rip;
        self.cs = ctx.iret.cs as u64;
        self.rflags = ctx.iret.rflags;
        self.rsp = ctx.iret.rsp;
        self.ss = ctx.iret.ss as u64;
    }
}

unsafe extern "C" {
    fn shivacore_timer_trampoline();
    fn shivacore_syscall_gate();
}

fn syscall_gate_entry_addr() -> VirtAddr {
    VirtAddr::new(shivacore_syscall_gate as *const () as usize as u64)
}

// Syscall-Gate: identische Stack-Disziplin wie das Timer-Trampolin (Interrupt-
// Gate ohne Error-Code, 15 GP-Register, 16-Byte-Alignment via `mov rdi, rsp`).
// Ergebnis wird in die gesicherten Slots von RAX (Wert) und RBX (Status)
// zurueckgeschrieben und beim `pop` wiederhergestellt.
global_asm!(
    r#"
    .global shivacore_syscall_gate
shivacore_syscall_gate:
    push r15
    push r14
    push r13
    push r12
    push r11
    push r10
    push r9
    push r8
    push rbp
    push rdi
    push rsi
    push rdx
    push rcx
    push rbx
    push rax
    mov rdi, rsp
    call {handler}
    pop rax
    pop rbx
    pop rcx
    pop rdx
    pop rsi
    pop rdi
    pop rbp
    pop r8
    pop r9
    pop r10
    pop r11
    pop r12
    pop r13
    pop r14
    pop r15
    iretq
"#,
    handler = sym syscall_gate_rust
);

global_asm!(
    r#"
    .global shivacore_timer_trampoline
shivacore_timer_trampoline:
    push r15
    push r14
    push r13
    push r12
    push r11
    push r10
    push r9
    push r8
    push rbp
    push rdi
    push rsi
    push rdx
    push rcx
    push rbx
    push rax
    mov rdi, rsp
    call {handler}
    pop rax
    pop rbx
    pop rcx
    pop rdx
    pop rsi
    pop rdi
    pop rbp
    pop r8
    pop r9
    pop r10
    pop r11
    pop r12
    pop r13
    pop r14
    pop r15
    iretq
"#,
    handler = sym timer_trap_rust
);

global_asm!(
    r#"
    // Alle Ausnahme-Rahmen haben dieselbe Form:
    // 15 GP-Register, Fehlercode, RIP, CS, RFLAGS, RSP, SS.
    // Der Fehlercode bleibt auf dem Stack; kein GP-Register wird vor seiner
    // Sicherung als Scratch-Register missbraucht.
    .global shivacore_fault_trampoline
shivacore_fault_trampoline:
    push r15
    push r14
    push r13
    push r12
    push r11
    push r10
    push r9
    push r8
    push rbp
    push rdi
    push rsi
    push rdx
    push rcx
    push rbx
    push rax
    mov esi, 0
    jmp shivacore_fault_common

    .global shivacore_gp_trampoline
shivacore_gp_trampoline:
    push r15
    push r14
    push r13
    push r12
    push r11
    push r10
    push r9
    push r8
    push rbp
    push rdi
    push rsi
    push rdx
    push rcx
    push rbx
    push rax
    mov esi, 1

shivacore_fault_common:
    // Nach 15 Pushes liegt der Fehlercode bei frame+120. Der Frame-Zeiger
    // bleibt in RDI; R12 ist als callee-saved Register unser Rueckweg zum Frame.
    lea rdi, [rsp]
    // Bei CPL0 legt die CPU kein altes RSP/SS ab. Diese beiden Felder werden
    // aus dem bekannten Hardware-Frame rekonstruiert, damit die Rust-Struktur
    // vollstaendig initialisiert ist. Bei CPL3 stammen sie aus dem CPU-Frame.
    test byte ptr [rdi + 136], 3
    jnz 1f
    lea rax, [rdi + 152]
    mov [rdi + 152], rax
    xor eax, eax
    mov ax, ss
    mov [rdi + 160], rax
1:
    mov r12, rdi
    and rsp, -16
    call {handler}
    mov rsp, r12
    pop rax
    pop rbx
    pop rcx
    pop rdx
    pop rsi
    pop rdi
    pop rbp
    pop r8
    pop r9
    pop r10
    pop r11
    pop r12
    pop r13
    pop r14
    pop r15
    // Den echten bzw. synthetischen Fehlercode vor IRETQ entfernen.
    add rsp, 8
    iretq

    .global shivacore_fault_trampoline_nocode
shivacore_fault_trampoline_nocode:
    // #UD hat keinen Hardware-Fehlercode. Einen Nullcode einfuegen, damit
    // ExceptionTrapFrame fuer alle drei Ausnahmearten dasselbe Layout hat.
    push 0
    push r15
    push r14
    push r13
    push r12
    push r11
    push r10
    push r9
    push r8
    push rbp
    push rdi
    push rsi
    push rdx
    push rcx
    push rbx
    push rax
    lea rdi, [rsp]
    mov esi, 2
    test byte ptr [rdi + 136], 3
    jnz 2f
    lea rax, [rdi + 152]
    mov [rdi + 152], rax
    xor eax, eax
    mov ax, ss
    mov [rdi + 160], rax
2:
    mov r12, rdi
    and rsp, -16
    call {handler}
    mov rsp, r12
    pop rax
    pop rbx
    pop rcx
    pop rdx
    pop rsi
    pop rdi
    pop rbp
    pop r8
    pop r9
    pop r10
    pop r11
    pop r12
    pop r13
    pop r14
    pop r15
    add rsp, 8
    iretq
"#,
    handler = sym exception_trap_rust
);

/// Fasst die Ausnahme an, statt sie zu wiederholen.
///
/// Der Rahmen wird hier **veraendert**: nach der Rueckkehr springt `iretq`
/// nicht in den fehlerhaften Prozess, sondern in den naechsten lauffaehigen.
/// Genau das war vorher die Luecke — der alte Handler gab nur aus und kehrte
/// zurueck, die Ausnahme loeste sofort wieder aus und die Maschine haengte.
///
/// `kind` kommt als Zahl aus dem Assembly (0/1/2). Der Weg ueber eine Zahl ist
/// kein Schoenheitsfehler: die drei Einsprünge teilen sich einen Rumpf, damit
/// eine Korrektur nicht nur an einer Haelfte ankommt.
#[unsafe(no_mangle)]
extern "C" fn exception_trap_rust(frame: &mut ExceptionTrapFrame, kind: u32) {
    handle_exception(
        frame,
        shivacore::fault::ExceptionKind::from_id(kind),
    );
}

fn handle_exception(
    frame: &mut ExceptionTrapFrame,
    kind: shivacore::fault::ExceptionKind,
) {
    use x86_64::registers::control::Cr2;

    let error_code = frame.error_code;

    let origin = shivacore::fault::Origin::from_cs(frame.cs);
    let pid = USER_SCHEDULER
        .lock()
        .as_ref()
        .and_then(|scheduler| scheduler.current_pid());
    // Die Rolle kommt aus der Diensteverwaltung. Sie wird VOR der Entscheidung
    // gelesen, damit die Entscheidung eine reine Funktion bleibt.
    let role = shivacore::fault::ProcessKind::from_is_service(
        pid.is_some_and(|pid| crate::services::is_service(u64::from(pid.0))),
    );
    let mut action = shivacore::fault::decide(kind, origin, role);
    let report = shivacore::fault::FaultReport::from_error_code(error_code);
    // Cr2::read liefert in dieser Fassung ein Result; die Zahl ist das, was im
    // Log steht, nicht die Debug-Form.
    let address = match Cr2::read() {
        Ok(addr) => addr.as_u64(),
        Err(_) => 0,
    };
    // Ein Dienst kehrt an seinen Startzustand zurueck, BEVOR Capabilities
    // widerrufen werden — wie beim Exit-Syscall. Danach haette der neue Lauf
    // keine Rechte mehr, und ein Neustart ohne Rechte waere ein Dienst, der
    // nichts tun kann. Ist das Neustart-Limit erreicht, bleibt es beim Ende.
    if action == shivacore::fault::FaultAction::RestartService {
        let restarted = pid.and_then(|pid| crate::services::restart(u64::from(pid.0)));
        match restarted {
            Some(service) => {
                reset_service_process(frame, &service);
                // Der Fehlercode bedeutet je Vektor etwas anderes; beim
                // ungueltigen Opcode gibt es keinen, und CR2 steht dann auf dem
                // letzten Wert. Eine Adresse waere dort eine falsche Aussage.
                if kind.has_error_code() {
                    serial_println!(
                        "ShivaCore: Dienst '{}' PID={} nach {} bei Adresse {:#x} ({}) neu gestartet (Versuch {}/{}) — Fehler wird nicht wiederholt",
                        service.name,
                        pid.map(|pid| pid.0).unwrap_or_default(),
                        kind.name(),
                        address,
                        report.describe(),
                        service.attempts,
                        service.limit
                    );
                } else {
                    serial_println!(
                        "ShivaCore: Dienst '{}' PID={} nach {} (ohne Fehlercode) neu gestartet (Versuch {}/{}) — Fehler wird nicht wiederholt",
                        service.name,
                        pid.map(|pid| pid.0).unwrap_or_default(),
                        kind.name(),
                        service.attempts,
                        service.limit
                    );
                }
                let recorded = FAULT_COUNTERS
                    .lock()
                    .record(origin, shivacore::fault::FaultAction::RestartService);
                shivacore::security::record(
                    "kernel",
                    "fault.service_restart",
                    &alloc::format!(
                        "kind={} addr={:#x} attempts={}/{} restarts={}",
                        kind.name(),
                        address,
                        service.attempts,
                        service.limit,
                        recorded.service_restarts
                    ),
                    shivacore::security::AuditResult::Allowed,
                );
                return;
            }
            None => {
                serial_println!(
                    "ShivaCore: Dienst PID={:?} Neustart-Limit erreicht — der Fehler beendet ihn",
                    pid
                );
                action = shivacore::fault::FaultAction::TerminateProcess;
            }
        }
    }
    let recorded = FAULT_COUNTERS.lock().record(origin, action);

    // Audit fuer Seitenfehler (und die beiden anderen Ausnahmen): Vektor,
    // Adresse, Herkunft und ob der Prozess beendet wurde. Vorher stand das nur
    // auf der seriellen Konsole und war damit nicht Teil der Kette.
    shivacore::security::record(
        "kernel",
        "fault.exception",
        &alloc::format!(
            "kind={} addr={:#x} user={} terminated={} restarts={} kernel_faults={}",
            kind.name(),
            address,
            report.from_user,
            recorded.terminated,
            recorded.service_restarts,
            recorded.kernel_faults
        ),
        shivacore::security::AuditResult::Error,
    );

    match action {
        shivacore::fault::FaultAction::TerminateProcess => {
            let pid = USER_SCHEDULER
                .lock()
                .as_ref()
                .and_then(|scheduler| scheduler.current_pid());
            // Was im Log steht, muss zur Ausnahme passen. Der Fehlercode
            // bedeutet je Vektor etwas anderes, und beim ungueltigen Opcode
            // gibt es keinen — CR2 steht dann auf dem letzten Wert und hat mit
            // diesem Fehler nichts zu tun.
            match kind {
                // RIP und RSP gehoeren in den Bericht: ohne sie sagt die
                // Adresse nicht, welche Anweisung sie benutzt hat, und die
                // Fehlersuche beginnt bei Null.
                shivacore::fault::ExceptionKind::PageFault => serial_println!(
                    "ShivaCore: {} PID={:?} Adresse={:#x} ({}) Usermodus={} RIP={:#x} RSP={:#x} [beendet: {}, Kernel: {}]",
                    kind.name(),
                    pid,
                    address,
                    report.describe(),
                    report.from_user,
                    frame.rip,
                    frame.rsp,
                    recorded.terminated,
                    recorded.kernel_faults
                ),
                shivacore::fault::ExceptionKind::GeneralProtection => serial_println!(
                    // Bei der Schutzverletzung ist der Fehlercode eine
                    // Segmentauswahl oder 0 — Bit 2 sagt hier NICHT, ob der
                    // Zugriff aus Ring 3 kam. Der Ring steht im Code-Segment
                    // des Rahmens, deshalb `origin` und nicht `report`.
                    "ShivaCore: {} PID={:?} Fehlercode={:#x} Usermodus={} [beendet: {}, Kernel: {}]",
                    kind.name(),
                    pid,
                    error_code,
                    origin == shivacore::fault::Origin::User,
                    recorded.terminated,
                    recorded.kernel_faults
                ),
                shivacore::fault::ExceptionKind::InvalidOpcode => serial_println!(
                    "ShivaCore: {} PID={:?} ohne Fehlercode, Usermodus={} [beendet: {}, Kernel: {}]",
                    kind.name(),
                    pid,
                    origin == shivacore::fault::Origin::User,
                    recorded.terminated,
                    recorded.kernel_faults
                ),
            };
            // Exit-Cleanup vor dem Kontextwechsel: ein beendeter Prozess darf
            // keine gueltigen Capabilities hinterlassen.
            if let Some(pid) = pid {
                let revoked = cleanup_process(pid);
                serial_println!(
                    "ShivaCore: Exit-Cleanup PID={:?}: {} Capabilities widerrufen, TLB geleert",
                    pid,
                    revoked
                );
            }
            let next = {
                let mut guard = USER_SCHEDULER.lock();
                let scheduler = guard.as_mut();
                match (scheduler, pid) {
                    (Some(scheduler), Some(pid)) => {
                        // Negativer Exit-Code wie ein Signal, aus der
                        // Fehlerart abgeleitet und im Zombie-Fach nachlesbar.
                        scheduler.exit_process(pid, shivacore::fault::exit_code(kind), None)
                    }
                    _ => None,
                }
            };
            match next {
                Some((next_pid, next)) => {
                    // Auch hier gehoert die Seitentabelle zum Prozess: ohne
                    // diesen Wechsel laeuft der naechste Prozess mit der
                    // Tabelle des beendeten weiter und stirbt an seiner
                    // eigenen Codeadresse — genau das war im Log zu sehen
                    // (PID 1000, 0x400077, nachdem 1003 beendet wurde).
                    let cr3 = {
                        let mut guard = USER_SCHEDULER.lock();
                        guard.as_ref().and_then(|s| s.cr3_of(next_pid))
                    };
                    if let Some(cr3) = cr3 {
                        shivacore::addr_space::install_raw(cr3);
                    }
                    frame.apply_saved_context(&next);
                    serial_println!(
                        "ShivaCore: weiter mit PID={} statt den Fehler zu wiederholen",
                        next_pid.0
                    );
                }
                None => {
                    serial_println!("ShivaCore: kein lauffaehiger Prozess mehr, Kernel haelt an");
                    loop {
                        x86_64::instructions::hlt();
                    }
                }
            }
        }
        // Wird oben behandelt. Erreichbar bleibt der Zweig nur, wenn der
        // Prozess zwischen Entscheidung und Neustart aus der Verwaltung
        // verschwunden ist; dann gilt er als abgestuerzt und wird beendet,
        // statt an derselben Anweisung erneut zu scheitern.
        shivacore::fault::FaultAction::RestartService => {
            if let Some(pid) = pid {
                exit_current_process(frame, pid, shivacore::fault::exit_code(kind));
            }
        }
        shivacore::fault::FaultAction::PanicKernel => {
            panic!(
                "Kernelausnahme {}: Adresse={:#x} ({}), Ring 0",
                kind.name(),
                address,
                report.describe()
            );
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn timer_trap_rust(frame: &mut UserTrapFrame) {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    // SC-006 REQ-02/03 im echten Pfad: die Line wird gesperrt, bevor der
    // Handler arbeitet, und erst im ACK wieder freigegeben. Ein Trigger
    // waehrend der Bearbeitung bleibt im IOAPIC liegen (kein Re-Trigger).
    // SC-006 REQ-02/03: Line sperren, bevor der Handler arbeitet; die
    // Freigabe passiert im ACK (acknowledge_timer_irq). Ein Trigger waehrend
    // der Bearbeitung bleibt im IOAPIC liegen statt zu verschachteln.
    if shivacore::apic::acknowledges_via_apic() {
        shivacore::apic::mask_interrupt_vector(TIMER_VECTOR);
    }
    // Ein Hardware-Tick -> genau ein MonotonicTime-Schritt.
    // try_lock: faellt das Gate gerade in einem Syscall, wird der Tick
    // uebersprungen (monoton bleibt monoton) statt im IRQ zu blockieren.
    // Zeit fortschreiben, ohne auf das Gate zu warten. Frueher wurde der Tick
    // bei belegtem Gate uebersprungen; damit verlor MonotonicTime genau die
    // Zeit, in der Syscalls liefen.
    shivacore::syscall::advance_monotonic_tick();
    // Konsument der Eingabewarteschlange. Bewusst hier und nicht im
    // Tastatur-Handler: der Handler soll nur ablegen. Acht Ereignisse pro Tick
    // reichen fuer jede Tippgeschwindigkeit und begrenzen die Arbeit im IRQ.
    pump_input_into_channel();
    drain_input_queue();
    // ACK-Fristen: haengt ein Subskribent, gibt der Kernel die Line frei und
    // zaehlt die verpasste Quittung. Ohne diesen Schritt waere "maskiert bis
    // ACK" fuer einen haengenden Prozess eine dauerhafte Sperre.
    let now = TIMER_TICKS.load(Ordering::Relaxed);
    let expired: alloc::vec::Vec<shivacore::irq_bridge::AckTimeout> =
        IRQ_BRIDGE.lock().sweep_acks(now);
    for timeout in expired {
        ACK_TIMEOUTS.fetch_add(1, Ordering::Relaxed);
        serial_println!(
            "ShivaCore: SC-006 ACK-Frist verpasst: Line={} PID={} Serie={} ausgesetzt={}",
            timeout.line,
            timeout.pid.0,
            timeout.strikes,
            timeout.suspended
        );
    }
    let mut guard = USER_SCHEDULER.lock();
    let Some(scheduler) = guard.as_mut() else {
        acknowledge_timer_irq();
        return;
    };

    // P1-3: Zombies einsammeln. Vorher rief im Live-Pfad niemand
    // `reap_zombies`; beendete Prozesse blieben als Eintraege stehen. Die
    // Seitentabellen-Frames gehen ueber die Reclaim-Warteschlange zurueck (der
    // Frame-Allocator ist im Interrupt-Kontext ueber `try_lock` erreichbar; ist
    // er belegt, bleibt der Auftrag fuer den naechsten Tick liegen).
    let reaped = scheduler.reap_zombies();
    if !reaped.is_empty() {
        for (pid, exit_code) in &reaped {
            shivacore::security::record(
                "kernel",
                "process.reap",
                &alloc::format!("pid={:?} exit={}", pid, exit_code),
                shivacore::security::AuditResult::Allowed,
            );
        }
        for (pid, exit_code) in &reaped {
            serial_println!(
                "ShivaCore: Zombie-Reaping: PID={:?} exit={} (Ursache aus der Fehlerart)",
                pid,
                exit_code
            );
        }
        serial_println!(
            "ShivaCore: Zombie-Reaping: {} eingesammelt, {} Eintraege verbleiben",
            reaped.len(),
            scheduler.entries().len()
        );
    }

    // P1-3B: Seitentabellen-Frames der eingesammelten Prozesse zur Freigabe
    // anmelden. Die Freigabe selbst passiert verzoegert und ohne Warten: ist
    // der Allocator belegt, bleibt der Auftrag fuer den naechsten Tick liegen.
    if !reaped.is_empty() {
        for (pid, _) in &reaped {
            engine_reclaim_queue(*pid);
        }
    }
    if shivacore::reclaim::RECLAIMER.pending_len() > 0 {
        match crate::memory::FRAME_ALLOCATOR.try_lock() {
            Some(mut slot) => {
                if let Some(allocator) = slot.as_mut() {
                    let freed = shivacore::reclaim::RECLAIMER
                        .drain_with(|frame| allocator.deallocate_frame(frame));
                    if freed > 0 {
                        // Wiederbenutzung im Live-Pfad belegen, nicht nur im
                        // Selbsttest: einen Frame holen und pruefen, ob er aus
                        // der Freiliste kam (die Freiliste muss dabei kuerzer
                        // werden; der Bump-Zeiger laesst sie unberuehrt).
                        use x86_64::structures::paging::FrameAllocator;
                        let list_before_probe = allocator.free_list_len();
                        let probe = allocator.allocate_frame();
                        let reused =
                            probe.is_some() && allocator.free_list_len() < list_before_probe;
                        if let Some(frame) = probe {
                            // Zurueck auf die Freiliste: der Probe-Frame war
                            // geliehen, nicht verbraucht.
                            allocator.deallocate_frame(frame);
                        }
                        serial_println!(
                            "ShivaCore: Frame-Freigabe: {} Frames zurueckgegeben (Freiliste={}, Wiederbenutzung={}, Doppelfreigaben abgewiesen={})",
                            freed,
                            allocator.free_list_len(),
                            reused,
                            allocator.double_free_blocked()
                        );
                        for (pid, registered, released, rejected) in
                            shivacore::reclaim::RECLAIMER.take_last_report()
                        {
                            serial_println!(
                                "ShivaCore: Ressourcenbilanz PID={}: Frames {}/{} freigegeben, {} abgewiesen, Freiliste={}, freigegeben gesamt={}",
                                pid,
                                released,
                                registered,
                                rejected,
                                allocator.free_list_len(),
                                allocator.freed_total()
                            );
                        }
                        let entries = shivacore::security::KERNEL_AUDIT.entries();
                        let mut actions: alloc::vec::Vec<&str> = alloc::vec::Vec::new();
                        for entry in &entries {
                            if !actions.contains(&entry.action.as_str()) {
                                actions.push(entry.action.as_str());
                            }
                        }
                        serial_println!(
                            "ShivaCore: Audit nach Exit+Reclamation: {} Ereignisse, Kette {}, [{}]",
                            entries.len(),
                            if shivacore::security::audit_chain_intact() {
                                "intakt"
                            } else {
                                "GEBROCHEN"
                            },
                            actions.join(", ")
                        );
                    }
                }
            }
            // Allocator belegt: nicht blockieren, nur vormerken.
            None => shivacore::reclaim::RECLAIMER.note_busy(),
        }
    }

    let current_pid = scheduler.current_pid();
    let current = frame.saved_context();

    // SCHED-001 runtime invariant: each Ring-3 smoke process owns a distinct
    // register pattern. A mismatch fails the QEMU gate instead of masking
    // register-save/restore corruption.
    if let Some(pid) = current_pid {
        let (slot, expected) = match pid.0 {
            1000 => (Some(0usize), (0x1000, 0x1001, 0x1012)),
            1001 => (Some(1usize), (0x2000, 0x2001, 0x2012)),
            _ => (None, (0, 0, 0)),
        };
        let slot = slot.filter(|slot| PATTERN_EXPECTED[*slot].load(Ordering::Acquire));
        if let Some(slot) = slot {
            let observed = (current.regs.rax, current.regs.rbx, current.regs.r12);
            // Der erste Tick kann eintreffen, bevor der Ring-3-Payload seine
            // Register gesetzt hat. Bis das Muster einmal beobachtet wurde, ist
            // ein Tick Aufwaermphase; ab dann gilt die Invariante unveraendert.
            if !PATTERN_ESTABLISHED[slot].load(Ordering::Acquire) {
                if observed == expected {
                    PATTERN_ESTABLISHED[slot].store(true, Ordering::Release);
                    serial_println!(
                        "ShivaCore: SCHED-001 register pattern established PID={} RAX={:#x} RBX={:#x} R12={:#x}",
                        pid.0,
                        observed.0,
                        observed.1,
                        observed.2
                    );
                } else {
                    serial_println!(
                        "ShivaCore: SCHED-001 warm-up tick PID={} RAX={:#x} RBX={:#x} R12={:#x}",
                        pid.0,
                        observed.0,
                        observed.1,
                        observed.2
                    );
                }
            } else if observed != expected {
                panic!(
                    "SCHED-001 register corruption: PID={} RAX={:#x} RBX={:#x} R12={:#x}",
                    pid.0, observed.0, observed.1, observed.2
                );
            } else {
                serial_println!(
                    "ShivaCore: SCHED-001 register checkpoint PID={} RAX={:#x} RBX={:#x} R12={:#x}",
                    pid.0,
                    observed.0,
                    observed.1,
                    observed.2
                );
            }
        }
    }

    // P1-3D-H1: Ein Wechsel ist nur aus Ring 3 heraus zulaessig. Unterbricht
    // der Tick den Kernel selbst (Boot zwischen "scheduler armed" und dem
    // Ring-3-Sprung, Syscall-Bearbeitung, gehaltene Sperren), wird nur die
    // Zeit fortgeschrieben: der naechste Nutzertick entscheidet. Frueher
    // konnte der Tick mitten im Boot umschalten und blieb dann mit einer
    // halb aufgebauten Welt stehen (Bootstillstand bei 99 Zeilen).
    let from_user_mode = (frame.cs & 3) == 3;
    let switched = if from_user_mode {
        scheduler.timer_tick_saved(current)
    } else {
        if KERNEL_TICKS_DEFERRED.fetch_add(1, Ordering::Relaxed) == 0 {
            serial_println!(
                "ShivaCore: SCHED-001 Wechsel aufgeschoben: Tick im Kernel (CS={:#x}), naechster Nutzertick entscheidet",
                frame.cs
            );
        }
        scheduler.kernel_tick_saved(current);
        None
    };
    if let Some((next_pid, next)) = switched {
        // Erst die Seitentabelle, dann der Rahmen: der Prozess, dessen
        // Register gleich geladen werden, muss seine eigene Tabelle vorfinden,
        // sonst laeuft er weiter im gemeinsamen Abbild und die Trennung waere
        // nur eine Absicht.
        let cr3 = scheduler.cr3_of(next_pid);
        serial_println!(
            "ShivaCore: scheduler preemption -> context switch to PID={} RIP={:#x} RAX={:#x} CR3={:#x}",
            next_pid.0,
            next.iret.rip,
            next.regs.rax,
            cr3.unwrap_or(0)
        );
        if let Some(cr3) = cr3 {
            shivacore::addr_space::install_raw(cr3);
        }
        frame.restore(next);
    }

    acknowledge_timer_irq();
}

/// Vektor des Timer-IRQ, direkt aus dem IDT-Index abgeleitet. Damit kann der
/// IOAPIC-Eintrag nicht auf einen anderen Vektor zeigen als der Handler.
pub const TIMER_VECTOR: u8 = InterruptIndex::Timer as u8;

/// Ticks, die den Kernel selbst unterbrochen haben und deshalb keinen
/// Prozesswechsel ausgeloest haben.
pub static KERNEL_TICKS_DEFERRED: AtomicU64 = AtomicU64::new(0);

/// Vektor der PS/2-Tastatur. Kommt aus der Routing-Tabelle: der erste
/// automatisch vergebene Geraetevektor. Der IOAPIC-Eintrag wird beim Boot auf
/// genau diesen Vektor gelegt, damit Eintrag und IDT nicht auseinanderlaufen.
pub const KEYBOARD_VECTOR: u8 = shivacore::irq_routing::FIRST_DEVICE_VECTOR;

/// Zaehlt zugestellte Tastatur-Interrupts.
pub static KEYSTROKES: AtomicU64 = AtomicU64::new(0);

/// Bestaetigt den Timer-IRQ auf dem Weg, ueber den er kam: Local APIC im
/// IOAPIC-Betrieb, sonst die PIC. Beide Wege duerfen nicht gemischt werden.
fn acknowledge_timer_irq() {
    if shivacore::apic::acknowledges_via_apic() {
        shivacore::apic::acknowledge_interrupt();
        // Freigabe ist Teil des ACK, nicht ein spaeterer Schritt.
        shivacore::apic::unmask_interrupt_vector(TIMER_VECTOR);
    } else {
        unsafe {
            PICS.lock()
                .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
        }
    }
}

/// Pumpt das Geraete-FIFO in den Kanal der Registry.
///
/// Der Tastatur-IRQ legt weiterhin in `INPUT_QUEUE` ab: das ist die
/// Hardware-Strecke, kein Kanal. Ab hier gehoeren Nachrichten, Teilnehmer und
/// Lebensdauer der Registry. Ohne Nutzerkonsument protokolliert der Kernel
/// selbst (`drain_input_queue`), deshalb wird nur gepumpt, wenn ein
/// Ring-3-Konsument angemeldet ist.
fn pump_input_into_channel() {
    if !INPUT_CONSUMER_USERSPACE.load(Ordering::Relaxed) {
        return;
    }
    // Das FIFO nur versuchen: haelt der Trap-Pfad die Sperre, bleibt das
    // Ereignis liegen und wird im naechsten Tick gepumpt. Das Zaehlen
    // uebernimmt `pump_from_ingress` (belegte Runde vs. zugestellt).
    let mut queue = match INPUT_QUEUE.try_lock() {
        Some(queue) => queue,
        None => return,
    };
    shivacore::ipc_lifecycle::pump_from_ingress(
        &mut queue,
        shivacore::syscall::KERNEL_INPUT_CHANNEL,
        8,
    );
}

/// Holt abgelegte Tastenereignisse ab und protokolliert sie.
///
/// Bis zu acht pro Tick, damit ein Tippschwall den Tick nicht ueberzieht.
fn drain_input_queue() {
    if INPUT_CONSUMER_USERSPACE.load(Ordering::Relaxed) {
        return;
    }
    let mut drained = 0;
    while drained < 8 {
        let event = INPUT_QUEUE.lock().pop();
        let Some(event) = event else { break };
        drained += 1;
        INPUT_CONSUMED.fetch_add(1, Ordering::Relaxed);
        let line = line_for_vector(KEYBOARD_VECTOR).map(|route| route.irq);
        let dropped = INPUT_QUEUE.lock().dropped();
        match event.character {
            Some(character) if character.is_ascii_graphic() => {
                serial_println!(
                    "ShivaCore: Eingabe (Konsument): '{}' (Scancode {:#04X}) [Line {:?}, Bruecke {}, Rueckfall {}, Haenger {}, verworfen {}]",
                    character as char,
                    event.scancode,
                    line,
                    BRIDGED_DELIVERIES.load(Ordering::Relaxed),
                    DIRECT_FALLBACKS.load(Ordering::Relaxed),
                    STUCK_DELIVERIES.load(Ordering::Relaxed),
                    dropped
                );
            }
            Some(character) => {
                serial_println!(
                    "ShivaCore: Eingabe (Konsument): <Zeichen {:#X}> (Scancode {:#04X}) [Line {:?}, Bruecke {}, Rueckfall {}, Haenger {}, verworfen {}]",
                    character as u32,
                    event.scancode,
                    line,
                    BRIDGED_DELIVERIES.load(Ordering::Relaxed),
                    DIRECT_FALLBACKS.load(Ordering::Relaxed),
                    STUCK_DELIVERIES.load(Ordering::Relaxed),
                    dropped
                );
            }
            None => {
                serial_println!(
                    "ShivaCore: Eingabe (Konsument): <ohne Zeichen> (Scancode {:#04X}) [Line {:?}, Bruecke {}, Rueckfall {}, Haenger {}, verworfen {}]",
                    event.scancode,
                    line,
                    BRIDGED_DELIVERIES.load(Ordering::Relaxed),
                    DIRECT_FALLBACKS.load(Ordering::Relaxed),
                    STUCK_DELIVERIES.load(Ordering::Relaxed),
                    dropped
                );
            }
        }
    }
}

fn timer_interrupt_entry_addr() -> VirtAddr {
    VirtAddr::new(shivacore_timer_trampoline as usize as u64)
}

/// Adresse des Ausnahme-Trampolins fuer die IDT (mit Fehlercode).
fn fault_trampoline_addr() -> VirtAddr {
    VirtAddr::new(shivacore_fault_trampoline as usize as u64)
}

/// Adresse des Einsprungs fuer die Schutzverletzung.
fn general_protection_trampoline_addr() -> VirtAddr {
    VirtAddr::new(shivacore_gp_trampoline as usize as u64)
}

/// Adresse des Ausnahme-Trampolins ohne Fehlercode.
fn fault_trampoline_nocode_addr() -> VirtAddr {
    VirtAddr::new(shivacore_fault_trampoline_nocode as usize as u64)
}

unsafe extern "C" {
    fn shivacore_fault_trampoline();
    fn shivacore_gp_trampoline();
    fn shivacore_fault_trampoline_nocode();
}

/// Register-Konvention nach SC-004 §3 (Cap-Slot, Opcode, 4 Argument-Woerter,
/// 2 Resultat-Woerter). Die konkrete Registerzuordnung ist SC-DEC-L §13 und
/// damit noch offen; hier gilt:
///   RDI = Cap-Slot (0 = kein Cap) · RSI = Opcode
///   RDX, RCX, R8, R9 = die vier Argument-Woerter
///   RAX = Resultat 1 (Wert) · RBX = Resultat 2 (Status: 0 = Erfolg, sonst AbiError)
/// Merkt sich, von welchen Prozessen schon ein Syscall protokolliert wurde.
///
/// Acht Plaetze genuegen: mehr Prozesse laufen nicht gleichzeitig, und ein
/// voller Vorrat schaltet nur die Zusatzzeile ab, nicht die Zaehlung.
struct FirstSyscalls {
    seen: [u64; 8],
}

impl FirstSyscalls {
    const fn new() -> Self {
        Self { seen: [0; 8] }
    }

    /// Wahr beim ersten Aufruf dieses Prozesses.
    fn note(&mut self, pid: u64) -> bool {
        for slot in self.seen.iter_mut() {
            if *slot == pid {
                return false;
            }
            if *slot == 0 {
                *slot = pid;
                return true;
            }
        }
        false
    }
}

static FIRST_SYSCALLS: spin::Mutex<FirstSyscalls> = spin::Mutex::new(FirstSyscalls::new());

#[cfg(test)]
mod first_syscall_tests {
    use super::FirstSyscalls;

    #[test]
    fn each_process_reports_its_first_call_exactly_once() {
        let mut first = FirstSyscalls::new();
        assert!(first.note(1000));
        assert!(!first.note(1000));
        assert!(first.note(1003));
        assert!(!first.note(1003));
    }

    #[test]
    fn a_full_table_stops_the_extra_line_instead_of_forgetting_slots() {
        let mut first = FirstSyscalls::new();
        for pid in 1000..1008 {
            assert!(first.note(pid));
        }
        // Neunter Prozess: keine Zusatzzeile mehr, aber keine Verwechslung
        // mit einem schon bekannten Prozess.
        assert!(!first.note(1008));
        assert!(!first.note(1000));
    }
}

#[unsafe(no_mangle)]
extern "C" fn syscall_gate_rust(frame: &mut UserTrapFrame) {
    // ABI 2.0.0: RDI = Zeiger auf den Argumentblock, RSI = Blocklaenge,
    // RDX = ABI-Version. Reihenfolge ist bindend: erst die Version, dann die
    // Laenge, dann die Rechte am Adressraum, dann die Kopie. Ein Zeiger aus dem
    // Userspace ist ein Vorschlag, keine Zusage.
    let version = frame.rdx as u32;
    let block_ptr = frame.rdi;
    let block_len = frame.rsi as usize;

    let reply = |frame: &mut UserTrapFrame, value: u64, error: Option<libshivacore::AbiError>| {
        frame.rax = value;
        frame.rbx = error.map(|e| e as u64).unwrap_or(0);
    };

    if !libshivacore::validate_abi_version(version) {
        reply(frame, 0, Some(libshivacore::AbiError::AbiVersionMismatch));
        return;
    }
    if !(libshivacore::SYSCALL_BLOCK_MIN_SIZE..=libshivacore::SYSCALL_BLOCK_MAX_SIZE)
        .contains(&block_len)
    {
        reply(frame, 0, Some(libshivacore::AbiError::InvalidPayload));
        return;
    }

    let pid = USER_SCHEDULER
        .lock()
        .as_ref()
        .and_then(|s| s.current_pid())
        .unwrap_or(Pid(0));

    // Ohne das Abbild des Aufrufers gibt es keine Grundlage fuer eine Pruefung.
    let space = USER_SCHEDULER
        .lock()
        .as_ref()
        .and_then(|s| s.address_space(pid));
    match space {
        Some(space) if space.range_writable(block_ptr, block_len) => {}
        _ => {
            reply(frame, 0, Some(libshivacore::AbiError::InvalidPayload));
            return;
        }
    }

    // Kopie in den Kernel, danach wird nur noch der Kernspeicher gelesen.
    let mut block_bytes = [0u8; libshivacore::SYSCALL_BLOCK_MAX_SIZE];
    // Der Bereich ist als beschreibbarer User-Bereich geprueft und die Laenge
    // ist auf SYSCALL_BLOCK_MAX_SIZE begrenzt — die Kopie bleibt im Fenster.
    unsafe {
        core::ptr::copy_nonoverlapping(block_ptr as *const u8, block_bytes.as_mut_ptr(), block_len);
    }

    // Diagnose: den rohen Opcode mitloggen, wenn er weder Yield noch
    // IpcReceive ist. So faellt ein Aufruf auf (z. B. Exit), der die Parser-
    // Stufe nicht erreicht, statt still als InvalidSyscall zu enden.
    {
        let raw_opcode = u16::from_le_bytes([block_bytes[8], block_bytes[9]]);
        if raw_opcode != libshivacore::Syscall::Yield.id()
            && raw_opcode != libshivacore::Syscall::IpcReceive.id()
            && raw_opcode != libshivacore::Syscall::Console.id()
        {
            serial_println!(
                "ShivaCore: Syscall-Rohopcode aus Ring 3: PID={} Opcode={:#06X} Laenge={}",
                pid.0,
                raw_opcode,
                block_len
            );
        }
    }

    let block = match libshivacore::SyscallBlock::parse(&block_bytes[..block_len]) {
        Ok(block) => block,
        Err(error) => {
            reply(frame, 0, Some(error));
            return;
        }
    };
    let request = SyscallRequest::from_block(&block);

    // Konsole: der Aufruf braucht den Bildschirm und eine Kopie aus dem
    // Prozessspeicher. Beides kennt der Dispatcher nicht, deshalb wird er hier
    // behandelt — an derselben Stelle wie `Exit`, aus demselben Grund.
    if request.syscall_id == libshivacore::Syscall::Console.id() {
        let response = handle_console(pid, request);
        reply(frame, response.value, response.error);
        return;
    }

    // Anmeldepruefung: wie die Konsole braucht sie Bildschirmbesitz und eine
    // Kopie aus dem Prozessspeicher, deshalb an derselben Stelle.
    if request.syscall_id == libshivacore::Syscall::IdentityCheck.id() {
        let response = handle_identity_check(pid, request);
        reply(frame, response.value, response.error);
        return;
    }

    // Freiwilliges Prozessende. Vor der Gate-Disposition abgefangen, weil der
    // Gate diesen Opcode nicht kennt: er beendet den Aufrufer, statt ihm zu
    // antworten. Danach laeuft der naechste Prozess im Rahmen dieses Traps.
    if request.syscall_id == libshivacore::Syscall::Exit.id() {
        reply(frame, 0, None);
        exit_current_process(frame, pid, request.arg0 as i32);
        return;
    }

    let entries = SYSCALL_ENTRIES.fetch_add(1, Ordering::Relaxed) + 1;
    let response = match SYSCALL_GATE.lock().as_mut() {
        Some(gate) => gate.handle(pid, request),
        None => {
            reply(frame, 0, Some(libshivacore::AbiError::EndpointUnavailable));
            return;
        }
    };

    // Der erste Syscall eines Prozesses ist die aussagekraeftigste Zeile: dort
    // steht der Opcode, den das Programm tatsaechlich gewaehlt hat. Mit dem
    // globalen Zaehler (Nr=1 oder Vielfache von 1_000_000) faellt er unter den
    // Tisch, sobald ein anderer Prozess vorher rechnet — bei einer Messung, die
    // genau einen Aufruf braucht, ist das der Unterschied zwischen Befund und
    // Blindstelle.
    let first_of_process = FIRST_SYSCALLS.lock().note(u64::from(pid.0));
    if entries == 1 || first_of_process || entries % 1_000_000 == 0 {
        serial_println!(
            "ShivaCore: Syscall aus Ring 3: Nr={} PID={} Opcode={:#06X} Ergebnis={:?}",
            entries,
            pid.0,
            request.syscall_id,
            response.error
        );
    }

    if request.syscall_id == libshivacore::Syscall::IpcReceive.id() && response.value != 0 {
        let delivered = INPUT_DELIVERED.fetch_add(1, Ordering::Relaxed) + 1;
        let word = response.value;
        let scancode = word & 0xFF;
        let character = (word >> 16) & 0xFF;
        let queue = INPUT_QUEUE.lock();
        let (bridged, fallbacks) = (
            BRIDGED_DELIVERIES.load(Ordering::Relaxed),
            DIRECT_FALLBACKS.load(Ordering::Relaxed),
        );
        let dropped = queue.dropped();
        drop(queue);
        if character != 0 {
            serial_println!(
                "ShivaCore: Tastenereignis an Ring 3: Nr={} PID={} Zeichen='{}' (Scancode {:#04X}) [Bruecke {}, Rueckfall {}, verworfen {}]",
                delivered,
                pid.0,
                character as u8 as char,
                scancode,
                bridged,
                fallbacks,
                dropped
            );
            // P1-3D: Der Nachweis fuer den Audit-Pfad entsteht hier, nicht im
            // Selbsttest. Genau einmal, damit der Log nicht bei jedem
            // Tastendruck eine Zusammenfassung traegt.
            if delivered == 1 {
                let entries = shivacore::security::KERNEL_AUDIT.entries();
                let mut actions: alloc::vec::Vec<&str> = alloc::vec::Vec::new();
                for entry in &entries {
                    if !actions.contains(&entry.action.as_str()) {
                        actions.push(entry.action.as_str());
                    }
                }
                let (rounds, busy, delivered, backed_up) = shivacore::ipc_lifecycle::pump_stats();
                serial_println!(
                    "ShivaCore: IPC-Pumpe: {} Versuche, {} wegen belegter Sperre uebersprungen, {} zugestellt, {} Stau",
                    rounds,
                    busy,
                    delivered,
                    backed_up
                );
                serial_println!(
                    "ShivaCore: Audit nach Live-IPC: {} Ereignisse, Kette {}, [{}]",
                    entries.len(),
                    if shivacore::security::audit_chain_intact() {
                        "intakt"
                    } else {
                        "GEBROCHEN"
                    },
                    actions.join(", ")
                );
            }
        } else {
            serial_println!(
                "ShivaCore: Tastenereignis an Ring 3: Nr={} PID={} Scancode {:#04X} [Bruecke {}, Rueckfall {}, verworfen {}]",
                delivered,
                pid.0,
                scancode,
                bridged,
                fallbacks,
                dropped
            );
        }
    }

    reply(frame, response.value, response.error);
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;
    let mut port: Port<u8> = Port::new(0x60);
    let _scancode: u8 = unsafe { port.read() };
    // Vollstaendiges Scancode->Keycode-Mapping folgt in einem spaeteren Sprint.
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

/// Tastatur ueber die zweite Line: derselbe Ablauf wie beim Timer, damit
/// SC-006 nicht nur fuer einen Vektor gilt. Scancode wird gemeldet statt
/// verworfen, sonst waere die Zustellung nicht nachweisbar.
extern "x86-interrupt" fn keyboard_interrupt_handler_at_device_vector(
    _stack_frame: InterruptStackFrame,
) {
    use x86_64::instructions::port::Port;
    KEYSTROKES.fetch_add(1, Ordering::Relaxed);
    let vector = u32::from(KEYBOARD_VECTOR);
    let apic = shivacore::apic::acknowledges_via_apic();
    // Eintritt: sperren ueber die rechtegepruefte Bruecke. Der Rueckfall auf
    // direktes Sperren ist gezaehlt, damit ein stiller Umweg auffaellt.
    let mut bridged = false;
    let mut release = false;
    if apic {
        let line = line_for_vector(KEYBOARD_VECTOR).map(|route| u32::from(route.irq));
        let through_bridge = match line {
            Some(line) => {
                let mut bridge = IRQ_BRIDGE.lock();
                let now = TIMER_TICKS.load(Ordering::Relaxed);
                match bridge.on_interrupt_at(line, vector, now) {
                    Ok(Some(_)) => {
                        bridged = true;
                        release = true;
                        true
                    }
                    // Offene Zustellung: die Line bleibt gesperrt, bis sie
                    // quittiert wird. Nicht freigeben und nicht neu zustellen.
                    // Ohne Meldung saehe das aus wie eine tote Tastatur.
                    Ok(None) => {
                        STUCK_DELIVERIES.fetch_add(1, Ordering::Relaxed);
                        true
                    }
                    Err(_) => false,
                }
            }
            None => false,
        };
        if !through_bridge {
            shivacore::apic::mask_interrupt_vector(KEYBOARD_VECTOR);
            release = true;
            DIRECT_FALLBACKS.fetch_add(1, Ordering::Relaxed);
        }
    }
    let mut port: Port<u8> = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };
    // Rohcode uebersetzen: erst damit ist der Interrupt eine Taste und keine
    // Zahl. Der Zustand (Shift, Caps Lock, Praefix) liegt daneben.
    let decoded = {
        let mut state = KEY_STATE.lock();
        shivacore::keymap::decode(scancode, &mut state)
    };
    // Die Line kommt aus der Routing-Tabelle: der Vektor allein sagt, wohin
    // zugestellt wurde, nicht wer der Absender ist.
    if decoded.pressed {
        // Nur ablegen. Ob der Puffer voll ist, entscheidet der Puffer selbst
        // und zaehlt es; hier wird nicht gewartet und nicht formatiert.
        let event = shivacore::input_queue::InputEvent::new(
            scancode,
            decoded.character.map(|byte| byte as char),
        );
        INPUT_QUEUE.lock().push(event);
    }
    if apic {
        // EOI zuerst, dann freigeben: der Vektor ist erst nach dem Quittieren
        // wieder zustellbar.
        shivacore::apic::acknowledge_interrupt();
        if bridged {
            BRIDGED_DELIVERIES.fetch_add(1, Ordering::Relaxed);
            let cap = KERNEL_CAPS.lock().subscriber_cap();
            let mut acked = false;
            if let Some(cap) = cap {
                let mut bridge = IRQ_BRIDGE.lock();
                acked = bridge.ack(&cap, vector).is_ok();
            }
            if !acked {
                shivacore::apic::unmask_interrupt_vector(KEYBOARD_VECTOR);
                DIRECT_FALLBACKS.fetch_add(1, Ordering::Relaxed);
            }
        } else if release {
            shivacore::apic::unmask_interrupt_vector(KEYBOARD_VECTOR);
        }
    } else {
        unsafe {
            PICS.lock().notify_end_of_interrupt(KEYBOARD_VECTOR);
        }
    }
}

#[cfg(test)]
mod exception_frame_tests {
    use super::ExceptionTrapFrame;
    use shivacore::user_sched::{IretFrame, SavedContext, SavedRegisters};

    #[test]
    fn exception_frame_matches_hardware_error_code_layout() {
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, error_code), 120);
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, rip), 128);
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, cs), 136);
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, rflags), 144);
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, rsp), 152);
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, ss), 160);
        assert_eq!(core::mem::size_of::<ExceptionTrapFrame>(), 168);
    }

    #[test]
    fn exception_frame_restores_every_general_register_and_iret_field() {
        let saved = SavedContext {
            regs: SavedRegisters {
                rax: 1, rbx: 2, rcx: 3, rdx: 4, rsi: 5, rdi: 6, rbp: 7,
                rsp: 0x8000, r8: 8, r9: 9, r10: 10, r11: 11, r12: 12,
                r13: 13, r14: 14, r15: 15,
            },
            iret: IretFrame {
                rip: 0x400000, cs: 0x1B, rflags: 0x202, rsp: 0x8000, ss: 0x23,
            },
        };
        let mut frame = ExceptionTrapFrame {
            rax: 0, rbx: 0, rcx: 0, rdx: 0, rsi: 0, rdi: 0, rbp: 0,
            r8: 0, r9: 0, r10: 0, r11: 0, r12: 0, r13: 0, r14: 0, r15: 0,
            error_code: 0x5, rip: 0, cs: 0, rflags: 0, rsp: 0, ss: 0,
        };
        frame.apply_saved_context(&saved);
        assert_eq!((frame.rax, frame.rbx, frame.rcx, frame.rdx), (1, 2, 3, 4));
        assert_eq!((frame.rsi, frame.rdi, frame.rbp), (5, 6, 7));
        assert_eq!((frame.r8, frame.r9, frame.r10, frame.r11), (8, 9, 10, 11));
        assert_eq!((frame.r12, frame.r13, frame.r14, frame.r15), (12, 13, 14, 15));
        assert_eq!((frame.rip, frame.cs, frame.rflags, frame.rsp, frame.ss),
            (0x400000, 0x1B, 0x202, 0x8000, 0x23));
        assert_eq!(frame.error_code, 0x5, "error code remains diagnostic data");
    }
}
