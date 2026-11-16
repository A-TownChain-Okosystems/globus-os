//! Stable userspace Runtime Core API.
//!
//! This module is the only runtime-facing facade for the ShivaCore ABI.
//! It deliberately does not expose kernel internals.

use libshivacore::{ABI_VERSION, AbiError, CapabilityHandle, MAX_SYSCALL_PAYLOAD, Syscall};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Identifier of a process created through the Runtime Core.
pub struct ProcessHandle(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// ABI error the runtime reports to userspace.
pub struct RuntimeError(pub AbiError);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Stable runtime facade for the ShivaCore ABI; kernel internals stay hidden.
pub struct RuntimeCore {
    abi_version: u32,
    next_process: u64,
}

impl Default for RuntimeCore {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeCore {
    /// Creates a core bound to the compiled ABI version; process ids start at 1.
    pub const fn new() -> Self {
        Self {
            abi_version: ABI_VERSION,
            next_process: 1,
        }
    }

    /// ABI version this build speaks.
    pub const fn abi_version(&self) -> u32 {
        self.abi_version
    }

    /// Hands out the next process id; fails once the counter is exhausted.
    pub fn spawn(&mut self) -> Result<ProcessHandle, RuntimeError> {
        let id = self.next_process;
        self.next_process = self
            .next_process
            .checked_add(1)
            .ok_or(RuntimeError(AbiError::ResourceExhausted))?;
        Ok(ProcessHandle(id))
    }

    /// Rejects syscall payloads above `MAX_SYSCALL_PAYLOAD`.
    pub fn validate_payload_len(&self, len: usize) -> Result<(), RuntimeError> {
        if len > MAX_SYSCALL_PAYLOAD {
            Err(RuntimeError(AbiError::InvalidPayload))
        } else {
            Ok(())
        }
    }

    /// True when this runtime build supports the syscall.
    pub const fn syscall_supported(&self, syscall: Syscall) -> bool {
        match syscall {
            // Exit gehoert seit ABI 0x0002 zum Vertrag: der Kernel widerruft die
            // Capabilities, setzt den Prozess auf Zombie und gibt den
            // Adressraum verzoegert frei. Ohne diesen Arm waere die Pruefung
            // nicht vollstaendig und der Build auf Stable bricht mit E0004.
            Syscall::Exit
            | Syscall::Yield
            | Syscall::IpcSend
            | Syscall::IpcReceive
            | Syscall::CapabilityQuery
            | Syscall::HandleClose
            | Syscall::MonotonicTime => true,
        }
    }

    /// Returns the capability handle or fails closed with `InvalidHandle`.
    pub fn require_capability(
        &self,
        capability: Option<CapabilityHandle>,
    ) -> Result<CapabilityHandle, RuntimeError> {
        capability.ok_or(RuntimeError(AbiError::InvalidHandle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_is_part_of_the_supported_abi() {
        assert!(RuntimeCore::new().syscall_supported(Syscall::Exit));
    }

    #[test]
    fn runtime_core_uses_kernel_abi_version() {
        assert_eq!(RuntimeCore::new().abi_version(), ABI_VERSION);
    }

    #[test]
    fn process_handles_are_runtime_owned() {
        let mut runtime = RuntimeCore::new();
        assert_eq!(runtime.spawn().unwrap(), ProcessHandle(1));
        assert_eq!(runtime.spawn().unwrap(), ProcessHandle(2));
    }

    #[test]
    fn payload_limit_is_fail_closed() {
        let runtime = RuntimeCore::new();
        assert!(runtime.validate_payload_len(MAX_SYSCALL_PAYLOAD).is_ok());
        assert_eq!(
            runtime.validate_payload_len(MAX_SYSCALL_PAYLOAD + 1),
            Err(RuntimeError(AbiError::InvalidPayload))
        );
    }

    #[test]
    fn missing_capability_is_rejected() {
        let runtime = RuntimeCore::new();
        assert_eq!(
            runtime.require_capability(None),
            Err(RuntimeError(AbiError::InvalidHandle))
        );
    }
}
