// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ShivaCore — Interrupt Descriptor Table + PIC-Remapping.
// K-Sprint 1: Breakpoint-, Double-Fault- und Page-Fault-Handler; die beiden
// klassischen 8259-PICs werden von ihren BIOS-Default-Vektoren (0x08-0x0F,
// 0x70-0x77 -- kollidieren mit CPU-Exceptions) auf 0x20-0x2F umgemappt.

use crate::gdt;
use crate::serial_println;
use crate::user_sched::{SavedContext, UserScheduler};
use crate::userspace::{UserAddressSpace, UserBinary, UserContext};
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::structures::idt::{
    InterruptDescriptorTable, InterruptStackFrame, InterruptStackFrameValue, PageFaultErrorCode,
};
use x86_64::registers::rflags::RFlags;
use x86_64::VirtAddr;

pub const PIC_1_OFFSET: u8 = 0x20;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

static USER_SCHEDULER: Mutex<Option<UserScheduler>> = Mutex::new(None);

pub fn init_user_scheduler(first: &UserContext, second: &UserContext) {
    let mut scheduler = UserScheduler::new();
    scheduler.add_process(first.pid, first, 0);
    scheduler.add_process(second.pid, second, 0);
    let _ = scheduler.schedule(None);
    *USER_SCHEDULER.lock() = Some(scheduler);
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
        idt[InterruptIndex::Timer.as_u8()].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_u8()].set_handler_fn(keyboard_interrupt_handler);
        // User-mode software interrupt gate. DPL3 permits CPL3 to enter the kernel.
        idt[0x80].set_handler_fn(syscall_interrupt_handler)
            .set_privilege_level(x86_64::PrivilegeLevel::Ring3);
        idt
    };
}

pub fn init_idt() {
    IDT.load();
}

pub fn init_pics() {
    unsafe {
        PICS.lock().initialize();
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

extern "x86-interrupt" fn timer_interrupt_handler(mut stack_frame: InterruptStackFrame) {
    let mut switched: Option<(u32, SavedContext)> = None;

    {
        let mut guard = USER_SCHEDULER.lock();
        if let Some(scheduler) = guard.as_mut() {
            if let Some(current_pid) = scheduler.current_pid() {
                if let Some(entry) = scheduler.get_entry(current_pid) {
                    let saved = entry.saved_ctx;
                    let binary = UserBinary::from_bytes(
                        "timer-context",
                        alloc::vec![0x90],
                        saved.iret.rip,
                    );
                    let mut current_ctx =
                        UserContext::new(current_pid, &binary, UserAddressSpace::default());
                    current_ctx.rip = stack_frame.instruction_pointer.as_u64();
                    current_ctx.rsp = stack_frame.stack_pointer.as_u64();
                    current_ctx.rflags = stack_frame.cpu_flags.bits();
                    current_ctx.cs = stack_frame.code_segment.0;
                    current_ctx.ss = stack_frame.stack_segment.0;

                    if let Some(next) = scheduler.timer_tick(&current_ctx) {
                        switched = Some((next.0.0, next.1));
                    }
                }
            }
        }
    }

    if let Some((next_pid, next)) = switched {
        serial_println!(
            "ShivaCore: scheduler preemption -> context switch to PID={} RIP={:#x}",
            next_pid,
            next.iret.rip
        );
        let frame = InterruptStackFrameValue::new(
            VirtAddr::new(next.iret.rip),
            x86_64::structures::gdt::SegmentSelector::from_raw(next.iret.cs),
            RFlags::from_bits_truncate(next.iret.rflags),
            VirtAddr::new(next.iret.rsp),
            x86_64::structures::gdt::SegmentSelector::from_raw(next.iret.ss),
        );
        unsafe { stack_frame.as_mut().write(frame); }
    }

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

extern "x86-interrupt" fn syscall_interrupt_handler(stack_frame: InterruptStackFrame) {
    serial_println!(
        "ShivaCore: CPL3 syscall gate entered at RIP={:#x}",
        stack_frame.instruction_pointer().as_u64()
    );
    // Return to the interrupted userspace instruction for now. The ABI dispatcher
    // is wired next; this gate is intentionally observable before SCHED-001/SYS-001.
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
