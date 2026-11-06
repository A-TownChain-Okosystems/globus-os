// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ShivaCore — Interrupt Descriptor Table + PIC-Remapping.
// K-Sprint 1: Breakpoint-, Double-Fault- und Page-Fault-Handler; die beiden
// klassischen 8259-PICs werden von ihren BIOS-Default-Vektoren (0x08-0x0F,
// 0x70-0x77 -- kollidieren mit CPU-Exceptions) auf 0x20-0x2F umgemappt.

use crate::gdt;
use crate::serial_println;
use core::arch::global_asm;
use core::sync::atomic::{AtomicBool, Ordering};
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

/// Merkt je Ring-3-Smoke-Prozess, ob sein Register-Muster schon einmal
/// beobachtet wurde (Slot 0 = PID 1000, Slot 1 = PID 1001).
static PATTERN_ESTABLISHED: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];

static USER_SCHEDULER: Mutex<Option<UserScheduler>> = Mutex::new(None);

/// Der einzige Userspace->Kernel-Pfad: Dispatcher + Capability-Tabelle.
static SYSCALL_GATE: Mutex<Option<SyscallGate>> = Mutex::new(None);

pub fn init_user_scheduler(first: &UserContext, second: &UserContext) {
    let mut scheduler = UserScheduler::new();
    scheduler.add_process(first.pid, first, 0);
    scheduler.add_process(second.pid, second, 0);
    let _ = scheduler.schedule(None);
    *USER_SCHEDULER.lock() = Some(scheduler);
    *SYSCALL_GATE.lock() = Some(SyscallGate::new());
    serial_println!(
        "ShivaCore: SCHED-001 scheduler armed: PID={} + PID={}",
        first.pid.0,
        second.pid.0
    );
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
        idt.page_fault.set_handler_fn(page_fault_handler);
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
        command.write(0x36); // channel 0, lobyte/hibyte, mode 3, binary
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

#[no_mangle]
extern "C" fn timer_trap_rust(frame: &mut UserTrapFrame) {
    // Ein Hardware-Tick -> genau ein MonotonicTime-Schritt.
    // try_lock: faellt das Gate gerade in einem Syscall, wird der Tick
    // uebersprungen (monoton bleibt monoton) statt im IRQ zu blockieren.
    if let Some(mut guard) = SYSCALL_GATE.try_lock() {
        if let Some(gate) = guard.as_mut() {
            gate.on_tick();
        }
    }
    let mut guard = USER_SCHEDULER.lock();
    let Some(scheduler) = guard.as_mut() else {
        unsafe {
            PICS.lock()
                .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
        }
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

    if let Some((next_pid, next)) = scheduler.timer_tick_saved(current) {
        serial_println!(
            "ShivaCore: scheduler preemption -> context switch to PID={} RIP={:#x} RAX={:#x}",
            next_pid.0,
            next.iret.rip,
            next.regs.rax
        );
        frame.restore(next);
    }

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

fn timer_interrupt_entry_addr() -> VirtAddr {
    VirtAddr::new(shivacore_timer_trampoline as usize as u64)
}

/// Register-Konvention nach SC-004 §3 (Cap-Slot, Opcode, 4 Argument-Woerter,
/// 2 Resultat-Woerter). Die konkrete Registerzuordnung ist SC-DEC-L §13 und
/// damit noch offen; hier gilt:
///   RDI = Cap-Slot (0 = kein Cap) · RSI = Opcode
///   RDX, RCX, R8, R9 = die vier Argument-Woerter
///   RAX = Resultat 1 (Wert) · RBX = Resultat 2 (Status: 0 = Erfolg, sonst AbiError)
#[no_mangle]
extern "C" fn syscall_gate_rust(frame: &mut UserTrapFrame) {
    let cap_slot = frame.rdi;
    let request = SyscallRequest {
        abi_version: frame.r9 as u32,
        syscall_id: frame.rsi as u16,
        capability: if cap_slot == 0 {
            None
        } else {
            Some(CapabilityHandle(cap_slot))
        },
        arg0: frame.rdx,
        arg1: frame.rcx,
        payload_len: frame.r8 as usize,
    };

    let pid = USER_SCHEDULER
        .lock()
        .as_ref()
        .and_then(|s| s.current_pid())
        .unwrap_or(Pid(0));

    let response = match SYSCALL_GATE.lock().as_mut() {
        Some(gate) => gate.handle(pid, request),
        None => {
            frame.rax = 0;
            frame.rbx = libshivacore::AbiError::EndpointUnavailable as u64;
            return;
        }
    };

    frame.rax = response.value;
    frame.rbx = response.error.map(|e| e as u64).unwrap_or(0);
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
