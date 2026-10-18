// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ShivaCore — Interrupt Descriptor Table + PIC-Remapping.
// K-Sprint 1: Breakpoint-, Double-Fault- und Page-Fault-Handler; die beiden
// klassischen 8259-PICs werden von ihren BIOS-Default-Vektoren (0x08-0x0F,
// 0x70-0x77 -- kollidieren mit CPU-Exceptions) auf 0x20-0x2F umgemappt.

use crate::gdt;
use crate::serial_println;
use alloc::vec::Vec;
use core::arch::global_asm;
use x86_64::structures::paging::{OffsetPageTable, PhysFrame, Size4KiB};
use lazy_static::lazy_static;
use pic8259::ChainedPics;
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

static USER_SCHEDULER: Mutex<Option<UserScheduler>> = Mutex::new(None);
struct RegisteredUserAddressSpace {
    pid: shivacore::ats1000::Pid,
    context: UserContext,
    root_frame: PhysFrame<Size4KiB>,
    mapper: OffsetPageTable<'static>,
}

static USER_ADDRESS_SPACES: Mutex<Vec<RegisteredUserAddressSpace>> = Mutex::new(Vec::new());

pub fn init_user_scheduler(
    first: &UserContext,
    first_root: PhysFrame<Size4KiB>,
    first_mapper: OffsetPageTable<'static>,
    second: &UserContext,
    second_root: PhysFrame<Size4KiB>,
    second_mapper: OffsetPageTable<'static>,
) {
    *USER_ADDRESS_SPACES.lock() = alloc::vec![
        RegisteredUserAddressSpace {
            pid: first.pid,
            context: first.clone(),
            root_frame: first_root,
            mapper: first_mapper,
        },
        RegisteredUserAddressSpace {
            pid: second.pid,
            context: second.clone(),
            root_frame: second_root,
            mapper: second_mapper,
        },
    ];
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
        unsafe {
            idt[InterruptIndex::Timer.as_u8()]
                .set_handler_addr(timer_interrupt_entry_addr())
        };
        idt[InterruptIndex::Keyboard.as_u8()].set_handler_fn(keyboard_interrupt_handler);
        // User-mode software interrupt gate. The assembly trampoline saves every
        // general-purpose register before entering Rust and returns with IRETQ.
        unsafe {
            idt[0x80]
                .set_handler_addr(syscall_interrupt_entry_addr())
                .set_privilege_level(x86_64::PrivilegeLevel::Ring3);
        }
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
}

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
        let (rax, rbx, r12) = match pid.0 {
            1000 => (0x1000, 0x1001, 0x1012),
            1001 => (0x2000, 0x2001, 0x2012),
            _ => (current.regs.rax, current.regs.rbx, current.regs.r12),
        };
        if current.regs.rax != rax || current.regs.rbx != rbx || current.regs.r12 != r12 {
            panic!(
                "SCHED-001 register corruption: PID={} RAX={:#x} RBX={:#x} R12={:#x}",
                pid.0, current.regs.rax, current.regs.rbx, current.regs.r12
            );
        }
        serial_println!(
            "ShivaCore: SCHED-001 register checkpoint PID={} RAX={:#x} RBX={:#x} R12={:#x}",
            pid.0,
            current.regs.rax,
            current.regs.rbx,
            current.regs.r12
        );
    }

    if let Some((next_pid, next)) = scheduler.timer_tick_saved(current) {
        let next_root = USER_ADDRESS_SPACES
            .lock()
            .iter()
            .find(|space| space.pid == next_pid)
            .map(|space| space.root_frame)
            .unwrap_or_else(|| panic!("SCHED-001 missing page-table root for PID={}", next_pid.0));
        // The kernel stack, trampoline, IDT, and scheduler code are present in
        // every root through supervisor-only upper-half mappings.
        unsafe { crate::memory::switch_to_page_table(next_root) };
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

/// Register layout produced by shivacore_syscall_trampoline. The first
/// fifteen fields are pushed by the trampoline; the final five are the CPU's
/// privilege-transition IRET frame.
#[repr(C)]
struct SyscallTrapFrame {
    r15: u64, r14: u64, r13: u64, r12: u64, r11: u64,
    r10: u64, r9: u64, r8: u64, rbp: u64, rdi: u64,
    rsi: u64, rdx: u64, rcx: u64, rbx: u64, rax: u64,
    rip: u64, cs: u64, rflags: u64, rsp: u64, ss: u64,
}

static SYSCALL_KERNEL_STATE: Mutex<Option<shivacore::kernel_init::KernelState>> = Mutex::new(None);
static SYSCALL_DISPATCHER: Mutex<shivacore::syscall::SyscallDispatcher> =
    Mutex::new(shivacore::syscall::SyscallDispatcher::new());

/// Install the canonical, already initialized kernel state for trap dispatch.
/// The state is moved here once; syscall handling never constructs a parallel
/// capability table or IPC subsystem.
pub fn install_syscall_kernel(state: shivacore::kernel_init::KernelState) {
    let mut slot = SYSCALL_KERNEL_STATE.lock();
    assert!(slot.is_none(), "syscall kernel state installed twice");
    *slot = Some(state);
}

/// The mapper is installed after all boot-time mappings are complete. Ring-3
/// syscall handling is the only consumer while user execution is active.
fn user_address_space(pid: shivacore::ats1000::Pid) -> Option<UserContext> {
    USER_ADDRESS_SPACES
        .lock()
        .iter()
        .find(|space| space.pid == pid)
        .map(|space| space.context.clone())
}

fn validate_user_buffer(
    space: &RegisteredUserAddressSpace,
    address: u64,
    len: usize,
    write: bool,
) -> bool {
    if len == 0 {
        return true;
    }
    space.context.addr_space.validate_mapped_range(&space.mapper, address, len, write)
}

fn copy_from_user(
    space: &RegisteredUserAddressSpace,
    address: u64,
    len: usize,
) -> Option<Vec<u8>> {
    if !validate_user_buffer(space, address, len, false) {
        return None;
    }
    let mut bytes = alloc::vec![0u8; len];
    if len != 0 {
        // SAFETY: the address-space registry lock is held by the trap handler
        // across validation and copy, so no mapper mutation can invalidate PTEs.
        unsafe {
            core::ptr::copy_nonoverlapping(address as *const u8, bytes.as_mut_ptr(), len);
        }
    }
    Some(bytes)
}

fn copy_to_user(
    space: &RegisteredUserAddressSpace,
    address: u64,
    bytes: &[u8],
) -> bool {
    if !validate_user_buffer(space, address, bytes.len(), true) {
        return false;
    }
    if !bytes.is_empty() {
        // SAFETY: the registry lock remains held across PTE validation and copy.
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), address as *mut u8, bytes.len());
        }
    }
    true
}

extern "C" {
    fn shivacore_syscall_trampoline();
}

global_asm!(
    r#"
    .global shivacore_syscall_trampoline
shivacore_syscall_trampoline:
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
    mov r12, rsp
    mov rdi, rsp
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
    iretq
"#,
    handler = sym syscall_trap_rust
);

fn syscall_interrupt_entry_addr() -> VirtAddr {
    VirtAddr::new(shivacore_syscall_trampoline as usize as u64)
}

/// Trap ABI (x86-64): RAX=syscall id, RDI=ABI version, RSI=capability
/// handle (zero means absent), RDX=arg0, R10=arg1, R8=payload pointer,
/// R9=payload length. Return: RAX=value, RDX=0 on success or AbiError code.
///
/// IPC pointer spans are validated against the registered process layout and
/// that process's mapper before copy-in/copy-out. Processes have distinct CR3
/// roots, but this path is not yet SMP-safe: current PID/scheduler state is
/// global, there is no remote TLB shootdown protocol, and frame allocation and
/// address-space teardown do not yet have a complete concurrent lifecycle.
extern "C" fn syscall_trap_rust(frame: &mut SyscallTrapFrame) {
    let Some(pid) = USER_SCHEDULER.lock().as_ref().and_then(|s| s.current_pid()) else {
        frame.rax = 0;
        frame.rdx = libshivacore::AbiError::EndpointUnavailable as u64;
        return;
    };

    // Hold the address-space registry lock from PTE validation through
    // dispatch and copy-out. A future unmap/remap implementation must use this
    // same lock, preventing validation/copy TOCTOU races across CPUs.
    let address_spaces = USER_ADDRESS_SPACES.lock();
    let Some(user_space) = address_spaces.iter().find(|space| space.pid == pid) else {
        frame.rax = 0;
        frame.rdx = libshivacore::AbiError::EndpointUnavailable as u64;
        return;
    };

    let syscall_id = frame.rax as u16;
    let is_send = syscall_id == libshivacore::Syscall::IpcSend.id();
    let is_receive = syscall_id == libshivacore::Syscall::IpcReceive.id();
    let payload_len = if is_receive { 0 } else { frame.r9 as usize };
    let receive_capacity = if is_receive { frame.r9 as usize } else { 0 };

    // Enforce the ABI ceiling before allocating or walking a user-controlled span.
    if (is_send && payload_len > libshivacore::MAX_SYSCALL_PAYLOAD)
        || (is_receive && receive_capacity > libshivacore::MAX_SYSCALL_PAYLOAD)
    {
        frame.rax = 0;
        frame.rdx = libshivacore::AbiError::InvalidPayload as u64;
        return;
    }

    // Copy-in/copy-out is only performed after validating the full span against
    // the process layout and every effective PTE. IPC receive validates the
    // entire destination before dequeueing, so a bad buffer cannot consume data.
    let payload = if is_send {
        match copy_from_user(user_space, frame.r8, payload_len) {
            Some(bytes) => bytes,
            None => {
                serial_println!(
                    "ShivaCore: RING3_IPC pid={} syscall={} result=0 error=InvalidPayload reason=unreadable-or-unmapped-user-span",
                    pid.0,
                    syscall_id
                );
                frame.rax = 0;
                frame.rdx = libshivacore::AbiError::InvalidPayload as u64;
                return;
            }
        }
    } else {
        Vec::new()
    };
    if is_receive && !validate_user_buffer(user_space, frame.r8, receive_capacity, true) {
        serial_println!(
            "ShivaCore: RING3_IPC pid={} syscall={} result=0 error=InvalidPayload reason=unwritable-or-unmapped-user-span",
            pid.0,
            syscall_id
        );
        frame.rax = 0;
        frame.rdx = libshivacore::AbiError::InvalidPayload as u64;
        return;
    }

    let request = shivacore::syscall::SyscallRequest {
        abi_version: frame.rdi as u32,
        syscall_id,
        capability: if frame.rsi == 0 {
            None
        } else {
            Some(libshivacore::CapabilityHandle(frame.rsi))
        },
        arg0: frame.rdx,
        arg1: if is_receive { receive_capacity as u64 } else { frame.r10 },
        payload_len,
    };

    let mut state_guard = SYSCALL_KERNEL_STATE.lock();
    let Some(state) = state_guard.as_mut() else {
        frame.rax = 0;
        frame.rdx = libshivacore::AbiError::EndpointUnavailable as u64;
        return;
    };

    let mut receive_buffer = Vec::new();
    let response = SYSCALL_DISPATCHER.lock().dispatch_with_ipc(
        pid,
        request,
        &mut state.processes.caps,
        &mut state.ipc,
        &payload,
        &mut receive_buffer,
    );

    if response.error.is_none() && is_receive {
        if receive_buffer.len() > receive_capacity
            || !copy_to_user(user_space, frame.r8, &receive_buffer)
        {
            // The registry lock keeps the mapping stable from validation through
            // copy-out. Fail closed if the dispatcher returns an oversized payload
            // or the destination cannot be validated again.
            frame.rax = 0;
            frame.rdx = libshivacore::AbiError::InvalidPayload as u64;
            return;
        }
    }
    if is_send || is_receive {
        serial_println!(
            "ShivaCore: RING3_IPC pid={} syscall={} result={} error={:?}",
            pid.0,
            syscall_id,
            response.value,
            response.error
        );
    }
    frame.rax = response.value;
    frame.rdx = response.error.map(|error| error as u64).unwrap_or(0);
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
