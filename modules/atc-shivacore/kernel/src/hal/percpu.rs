// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! Building blocks for per-logical-CPU state.
//!
//! This type is storage and state-transition logic only. It is not yet wired to
//! GS-relative addressing, AP startup, interrupt entry, or scheduler dispatch.
//! Callers must keep one instance per logical CPU and publish it only after
//! that CPU's local architecture state has been initialized.
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CpuLifecycle {
    Offline = 0,
    Initializing = 1,
    Online = 2,
    Failed = 3,
}

impl CpuLifecycle {
    fn from_raw(value: u8) -> Self {
        match value {
            1 => Self::Initializing,
            2 => Self::Online,
            3 => Self::Failed,
            _ => Self::Offline,
        }
    }
}

/// State intended to be owned by exactly one logical CPU.
///
/// `current_pid == 0` means that no process is assigned. PID zero is therefore
/// reserved as the idle/no-process sentinel for this structure.
#[repr(C, align(64))]
pub struct PerCpuState {
    apic_id: u32,
    logical_id: u32,
    lifecycle: AtomicU8,
    _reserved: [u8; 7],
    current_pid: AtomicU64,
    interrupt_count: AtomicU64,
}

impl PerCpuState {
    pub const fn new(apic_id: u32, logical_id: u32) -> Self {
        Self {
            apic_id,
            logical_id,
            lifecycle: AtomicU8::new(CpuLifecycle::Offline as u8),
            _reserved: [0; 7],
            current_pid: AtomicU64::new(0),
            interrupt_count: AtomicU64::new(0),
        }
    }

    pub const fn apic_id(&self) -> u32 {
        self.apic_id
    }

    pub const fn logical_id(&self) -> u32 {
        self.logical_id
    }

    pub fn lifecycle(&self) -> CpuLifecycle {
        CpuLifecycle::from_raw(self.lifecycle.load(Ordering::Acquire))
    }

    /// Claim the one-time initialization transition.
    pub fn begin_initialization(&self) -> bool {
        self.lifecycle
            .compare_exchange(
                CpuLifecycle::Offline as u8,
                CpuLifecycle::Initializing as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    /// Publish this CPU as usable after its local state is fully initialized.
    pub fn publish_online(&self) -> bool {
        self.lifecycle
            .compare_exchange(
                CpuLifecycle::Initializing as u8,
                CpuLifecycle::Online as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    /// Mark initialization as failed. Online CPUs cannot be failed via this API.
    pub fn mark_initialization_failed(&self) -> bool {
        self.lifecycle
            .compare_exchange(
                CpuLifecycle::Initializing as u8,
                CpuLifecycle::Failed as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub fn current_pid(&self) -> Option<u64> {
        if self.lifecycle() != CpuLifecycle::Online {
            return None;
        }
        match self.current_pid.load(Ordering::Acquire) {
            0 => None,
            pid => Some(pid),
        }
    }

    /// Update the CPU-local process assignment only while the CPU is online.
    pub fn set_current_pid(&self, pid: Option<u64>) -> bool {
        if self.lifecycle() != CpuLifecycle::Online {
            return false;
        }
        self.current_pid.store(pid.unwrap_or(0), Ordering::Release);
        true
    }

    pub fn record_interrupt(&self) -> bool {
        if self.lifecycle() != CpuLifecycle::Online {
            return false;
        }
        self.interrupt_count.fetch_add(1, Ordering::Relaxed);
        true
    }

    pub fn interrupt_count(&self) -> u64 {
        self.interrupt_count.load(Ordering::Relaxed)
    }
}

/// IA32_GS_BASE in 64-bit mode. The kernel must establish an explicit
/// `swapgs` entry/exit policy before using this facility across CPL3 traps.
const IA32_GS_BASE: u32 = 0xC000_0101;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerCpuInitError {
    InterruptsEnabled,
    NullPointer,
    NonCanonicalAddress,
    ReadbackMismatch,
}

/// Install the current CPU's state as its GS base.
///
/// # Safety
/// - `state` must remain pinned, mapped, and alive for the lifetime of this
///   CPU's use of GS-relative access.
/// - The caller must arrange a correct `swapgs` policy for every privilege
///   transition before user mode can run code that relies on kernel GS state.
/// - Call before enabling local interrupts; this function rejects IF=1.
/// - Only one initializer may configure GS_BASE on this logical CPU at a time.
pub unsafe fn install_current(state: &'static PerCpuState) -> Result<(), PerCpuInitError> {
    if interrupts_enabled() {
        return Err(PerCpuInitError::InterruptsEnabled);
    }

    let address = state as *const PerCpuState as u64;
    if address == 0 {
        return Err(PerCpuInitError::NullPointer);
    }
    if !is_canonical_48_bit_address(address) {
        return Err(PerCpuInitError::NonCanonicalAddress);
    }

    let previous = unsafe { read_msr(IA32_GS_BASE) };
    unsafe { write_msr(IA32_GS_BASE, address) };
    if unsafe { read_msr(IA32_GS_BASE) } != address {
        unsafe { write_msr(IA32_GS_BASE, previous) };
        return Err(PerCpuInitError::ReadbackMismatch);
    }
    Ok(())
}

/// Read the APIC ID from the currently installed GS-based per-CPU block.
/// Returns `None` when GS_BASE has not been initialized to a non-null block.
#[inline]
pub fn current_apic_id_gs() -> Option<u32> {
    let value: u32;
    // SAFETY: caller-facing contract relies on GS_BASE being installed by
    // install_current; a null base is checked through the MSR first.
    if unsafe { read_msr(IA32_GS_BASE) } == 0 {
        return None;
    }
    unsafe {
        core::arch::asm!(
            "mov {value:e}, gs:[0]",
            value = out(reg) value,
            options(nostack, readonly, preserves_flags)
        );
    }
    Some(value)
}

/// Read the logical CPU ID via a GS-relative memory operand.
#[inline]
pub fn current_logical_id_gs() -> Option<u32> {
    if unsafe { read_msr(IA32_GS_BASE) } == 0 {
        return None;
    }
    let value: u32;
    unsafe {
        core::arch::asm!(
            "mov {value:e}, gs:[4]",
            value = out(reg) value,
            options(nostack, readonly, preserves_flags)
        );
    }
    Some(value)
}

#[inline]
fn interrupts_enabled() -> bool {
    let flags: u64;
    // Read RFLAGS.IF without depending on the optional x86_64 crate, so the
    // state-machine library remains testable without the boot feature.
    unsafe {
        core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(nomem));
    }
    (flags & (1 << 9)) != 0
}

fn is_canonical_48_bit_address(address: u64) -> bool {
    let upper = address >> 48;
    let sign = (address >> 47) & 1;
    if sign == 0 {
        upper == 0
    } else {
        upper == 0xFFFF
    }
}

unsafe fn read_msr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
        options(nomem, nostack, preserves_flags)
    );
    ((high as u64) << 32) | low as u64
}

unsafe fn write_msr(msr: u32, value: u64) {
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") value as u32,
        in("edx") (value >> 32) as u32,
        options(nomem, nostack, preserves_flags)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_address_check_accepts_low_and_high_48_bit_addresses() {
        assert!(is_canonical_48_bit_address(0x0000_7fff_ffff_ffff));
        assert!(is_canonical_48_bit_address(0xffff_8000_0000_0000));
        assert!(!is_canonical_48_bit_address(0x0000_8000_0000_0000));
        assert!(!is_canonical_48_bit_address(0xffff_7fff_ffff_ffff));
    }

    #[test]
    fn cpu_must_complete_initialization_before_becoming_online() {
        let cpu = PerCpuState::new(0x1234, 2);
        assert_eq!(cpu.lifecycle(), CpuLifecycle::Offline);
        assert!(!cpu.publish_online());
        assert!(cpu.begin_initialization());
        assert_eq!(cpu.lifecycle(), CpuLifecycle::Initializing);
        assert!(cpu.publish_online());
        assert_eq!(cpu.lifecycle(), CpuLifecycle::Online);
        assert!(!cpu.begin_initialization());
    }

    #[test]
    fn process_assignment_is_rejected_until_cpu_is_online() {
        let cpu = PerCpuState::new(7, 1);
        assert!(!cpu.set_current_pid(Some(42)));
        assert_eq!(cpu.current_pid(), None);
        assert!(cpu.begin_initialization());
        assert!(cpu.publish_online());
        assert!(cpu.set_current_pid(Some(42)));
        assert_eq!(cpu.current_pid(), Some(42));
        assert!(cpu.set_current_pid(None));
        assert_eq!(cpu.current_pid(), None);
    }

    #[test]
    fn failed_initialization_cannot_be_published_online() {
        let cpu = PerCpuState::new(3, 1);
        assert!(cpu.begin_initialization());
        assert!(cpu.mark_initialization_failed());
        assert_eq!(cpu.lifecycle(), CpuLifecycle::Failed);
        assert!(!cpu.publish_online());
        assert!(!cpu.set_current_pid(Some(9)));
    }

    #[test]
    fn interrupt_counter_changes_only_for_online_cpu() {
        let cpu = PerCpuState::new(0, 0);
        assert!(!cpu.record_interrupt());
        assert_eq!(cpu.interrupt_count(), 0);
        assert!(cpu.begin_initialization());
        assert!(cpu.publish_online());
        assert!(cpu.record_interrupt());
        assert!(cpu.record_interrupt());
        assert_eq!(cpu.interrupt_count(), 2);
    }

    #[test]
    fn cpu_local_process_and_interrupt_state_is_isolated() {
        let cpu0 = PerCpuState::new(0x10, 0);
        let cpu1 = PerCpuState::new(0x20, 1);

        for cpu in [&cpu0, &cpu1] {
            assert!(cpu.begin_initialization());
            assert!(cpu.publish_online());
        }

        assert!(cpu0.set_current_pid(Some(1000)));
        assert!(cpu1.set_current_pid(Some(1001)));
        assert!(cpu0.record_interrupt());
        assert!(cpu0.record_interrupt());
        assert!(cpu1.record_interrupt());

        assert_eq!(cpu0.apic_id(), 0x10);
        assert_eq!(cpu1.apic_id(), 0x20);
        assert_eq!(cpu0.logical_id(), 0);
        assert_eq!(cpu1.logical_id(), 1);
        assert_eq!(cpu0.current_pid(), Some(1000));
        assert_eq!(cpu1.current_pid(), Some(1001));
        assert_eq!(cpu0.interrupt_count(), 2);
        assert_eq!(cpu1.interrupt_count(), 1);

        assert!(cpu0.set_current_pid(None));
        assert_eq!(cpu0.current_pid(), None);
        assert_eq!(cpu1.current_pid(), Some(1001));
        assert_eq!(cpu1.interrupt_count(), 1);
    }

    #[test]
    fn cpu_local_state_rejects_duplicate_and_invalid_lifecycle_transitions() {
        let cpu = PerCpuState::new(7, 3);
        assert!(!cpu.publish_online());
        assert!(!cpu.mark_initialization_failed());
        assert!(cpu.begin_initialization());
        assert!(!cpu.begin_initialization());
        assert!(cpu.mark_initialization_failed());
        assert!(!cpu.publish_online());
        assert!(!cpu.set_current_pid(Some(77)));
        assert!(!cpu.record_interrupt());
        assert_eq!(cpu.current_pid(), None);
        assert_eq!(cpu.interrupt_count(), 0);
    }
}
