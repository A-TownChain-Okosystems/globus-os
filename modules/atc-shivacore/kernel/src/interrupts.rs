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
use x86_64::registers::rflags::RFlags;
use x86_64::structures::idt::{
    InterruptDescriptorTable, InterruptStackFrame, InterruptStackFrameValue, PageFaultErrorCode,
};
use x86_64::VirtAddr;

pub const PIC_1_OFFSET: u8 = 0x20;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

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
pub fn set_process_cr3(pid: crate::ats1000::Pid, cr3: u64) {
    if let Some(scheduler) = USER_SCHEDULER.lock().as_mut() {
        scheduler.set_cr3(pid, cr3);
    }
}

pub fn init_user_scheduler(
    first: &UserContext,
    second: &UserContext,
    third: &UserContext,
    fourth: Option<&UserContext>,
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
pub fn grant_input_read(pid: Pid) -> Option<u64> {
    let mut gate = SYSCALL_GATE.lock();
    // Das Gate frueh anlegen: der Payload braucht die Cap-Nummer, bevor der
    // Scheduler laeuft. `init_user_scheduler` ersetzt es deshalb nicht mehr.
    let gate = gate.get_or_insert_with(SyscallGate::new);
    let cap = gate.grant_channel_read(pid, shivacore::syscall::KERNEL_INPUT_CHANNEL);
    INPUT_CONSUMER_USERSPACE.store(true, Ordering::Release);
    Some(cap.0)
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
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

pub fn init_idt() {
    IDT.load();
}

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

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct UserTrapFrame {
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
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
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

extern "C" {
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
    // Drei Einsprünge, ein Rumpf. Der Unterschied zwischen den Ausnahmen ist
    // nur, ob die CPU einen Fehlercode auf den Stapel legt und welche Art im
    // Bericht steht — nicht, wie der Rahmen aussieht. Zwei fast gleiche
    // Rümpfe wären die nächste Stelle, an der eine Korrektur nur an einer
    // Hälfte ankommt.
    //
    // Art-IDs: 0 = Seitenfehler, 1 = Schutzverletzung, 2 = Ungueltiger Opcode.
    .global shivacore_fault_trampoline
shivacore_fault_trampoline:
    pop r11
    mov edx, 0
    jmp shivacore_fault_common
    .global shivacore_gp_trampoline
shivacore_gp_trampoline:
    pop r11
    mov edx, 1
    jmp shivacore_fault_common

shivacore_fault_common:
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
    // Fehlercode: 15 Pushes (120 Byte), r11 war der fuenfte Push von oben,
    // also 15-5 = 10 Felder ueber dem Stapelzeiger -> rsp + 80.
    mov rsi, [rsp + 80]
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

    .global shivacore_fault_trampoline_nocode
shivacore_fault_trampoline_nocode:
    // Ungueltiger Opcode legt keinen Fehlercode ab: der Rahmen beginnt direkt
    // mit RIP, deshalb hier kein pop.
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
    xor esi, esi
    mov edx, 2
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
#[no_mangle]
extern "C" fn exception_trap_rust(frame: &mut UserTrapFrame, error_code: u64, kind: u32) {
    handle_exception(
        frame,
        error_code,
        shivacore::fault::ExceptionKind::from_id(kind),
    );
}

fn handle_exception(
    frame: &mut UserTrapFrame,
    error_code: u64,
    kind: shivacore::fault::ExceptionKind,
) {
    use x86_64::registers::control::Cr2;

    let origin = shivacore::fault::Origin::from_cs(frame.cs);
    let action = shivacore::fault::decide(kind, origin);
    let report = shivacore::fault::FaultReport::from_error_code(error_code);
    // Cr2::read liefert in dieser Fassung ein Result; die Zahl ist das, was im
    // Log steht, nicht die Debug-Form.
    let address = match Cr2::read() {
        Ok(addr) => addr.as_u64(),
        Err(_) => 0,
    };
    let recorded = FAULT_COUNTERS.lock().record(origin, action);

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
                shivacore::fault::ExceptionKind::PageFault => serial_println!(
                    "ShivaCore: {} PID={:?} Adresse={:#x} ({}) Usermodus={} [beendet: {}, Kernel: {}]",
                    kind.name(),
                    pid,
                    address,
                    report.describe(),
                    report.from_user,
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
            let next = {
                let mut guard = USER_SCHEDULER.lock();
                let scheduler = guard.as_mut();
                match (scheduler, pid) {
                    (Some(scheduler), Some(pid)) => {
                        // Negativer Exit-Code wie ein Signal: -11 steht fuer
                        // einen Seitenfehler und ist im Zombie-Fach nachlesbar.
                        scheduler.exit_process(pid, -11, None)
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

#[no_mangle]
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

    let switched = scheduler.timer_tick_saved(current);
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

extern "C" {
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

#[no_mangle]
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

    let block = match libshivacore::SyscallBlock::parse(&block_bytes[..block_len]) {
        Ok(block) => block,
        Err(error) => {
            reply(frame, 0, Some(error));
            return;
        }
    };
    let request = SyscallRequest::from_block(&block);

    let entries = SYSCALL_ENTRIES.fetch_add(1, Ordering::Relaxed) + 1;
    let response = match SYSCALL_GATE.lock().as_mut() {
        Some(gate) => gate.handle(pid, request, &mut *INPUT_QUEUE.lock()),
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
