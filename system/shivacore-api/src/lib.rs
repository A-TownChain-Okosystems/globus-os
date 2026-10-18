//! Stable, versioned userspace ABI exposed by ShivaCore.
//!
//! This crate contains only wire-safe identifiers and request/response types.
//! Kernel implementations remain private to ShivaCore.
#![no_std]

pub use globus_ipc::{Endpoint, Message, MessageHeader};

pub const ABI_MAJOR: u16 = 1;
pub const ABI_MINOR: u16 = 0;
pub const ABI_VERSION: u32 = ((ABI_MAJOR as u32) << 16) | ABI_MINOR as u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Syscall {
    Yield = 0x0001,
    IpcSend = 0x0010,
    IpcReceive = 0x0011,
    CapabilityQuery = 0x0020,
    HandleClose = 0x0030,
    MonotonicTime = 0x0040,
    CapabilityDelegate = 0x0021,
    CapabilityRevoke = 0x0022,
}

impl Syscall {
    pub const fn id(self) -> u16 {
        self as u16
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct CapabilityHandle(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ObjectHandle(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityRight {
    Read,
    Write,
    Execute,
    Grant,
    Revoke,
    Inspect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum AbiError {
    InvalidSyscall = 1,
    InvalidHandle = 2,
    PermissionDenied = 3,
    InvalidPayload = 4,
    EndpointUnavailable = 5,
    ResourceExhausted = 6,
    AbiVersionMismatch = 7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbiHandshake {
    pub major: u16,
    pub minor: u16,
}

impl AbiHandshake {
    pub const CURRENT: Self = Self {
        major: ABI_MAJOR,
        minor: ABI_MINOR,
    };

    pub const fn compatible(self) -> bool {
        self.major == ABI_MAJOR
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcSendRequest {
    pub endpoint: Endpoint,
    pub capability: CapabilityHandle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcReceiveRequest {
    pub endpoint: Endpoint,
    pub capability: CapabilityHandle,
    pub max_payload: u32,
}

/// Stable process identity used by userspace service contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ProcessId(pub u64);

/// Resource namespace understood by the stable capability service contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CapabilityResource {
    KnowledgeGraphEntity = 1,
}

/// Authorization operations supplied by the trusted kernel IPC/syscall client.
/// This is a contract, not a userspace-owned capability database.
pub trait CapabilityClient {
    fn create_capability(
        &mut self,
        owner: ProcessId,
        resource: CapabilityResource,
        resource_id: u64,
        rights: CapabilityRights,
    ) -> Result<CapabilityHandle, AbiError>;

    fn check_capability(
        &self,
        owner: ProcessId,
        resource: CapabilityResource,
        resource_id: u64,
        rights: CapabilityRights,
    ) -> bool;

    fn delegate_resource(
        &mut self,
        owner: ProcessId,
        resource: CapabilityResource,
        resource_id: u64,
        target: ProcessId,
        rights: CapabilityRights,
    ) -> Result<CapabilityHandle, AbiError>;

    fn revoke_resource(
        &mut self,
        owner: ProcessId,
        resource: CapabilityResource,
        resource_id: u64,
    ) -> Result<(), AbiError>;
}

/// Stable 32-byte digest exchanged across the kernel/service boundary.
pub type Hash32 = [u8; 32];

/// Deterministic compatibility hash used by existing Service-Space records.
///
/// This is deliberately kept as a contract primitive so callers do not import
/// the kernel security implementation. It is not a cryptographic hash.
pub fn simple_hash(data: &[u8]) -> Hash32 {
    let mut hash: u32 = 0x811c9dc5;
    for &byte in data {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    let mut result = [0u8; 32];
    result[..4].copy_from_slice(&hash.to_be_bytes());
    for i in (4..32).step_by(4) {
        hash ^= result[i - 4] as u32;
        hash = hash.wrapping_mul(0x01000193);
        result[i..i + 4].copy_from_slice(&hash.to_be_bytes());
    }
    result
}

/// Public capability rights representation. Kernel storage remains private.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct CapabilityRights(pub u8);

impl CapabilityRights {
    pub const NONE: Self = Self(0);
    pub const READ: Self = Self(1);
    pub const WRITE: Self = Self(2);
    pub const EXECUTE: Self = Self(4);
    pub const DELEGATE: Self = Self(8);
    pub const INSPECT: Self = Self(16);
    pub const DEFINED_MASK: u8 = 0x1F;

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    /// Compatibility helper for callers migrating from the kernel Rights type.
    pub const fn has(self, required: Self) -> bool {
        self.contains(required)
    }

    pub const fn bits(self) -> u8 {
        self.0
    }
}

impl core::ops::BitOr for CapabilityRights {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitAnd for CapabilityRights {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

/// Stable boot phases visible to services without exposing KernelState.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum KernelBootPhase {
    Heap = 0,
    Capabilities = 1,
    Processes = 2,
    Scheduler = 3,
    Ipc = 4,
    FileSystem = 5,
    Security = 6,
    Ready = 7,
}

pub const MAX_SYSCALL_PAYLOAD: usize = 1024 * 1024;

pub const fn validate_abi_version(version: u32) -> bool {
    (version >> 16) as u16 == ABI_MAJOR
}


/// x86-64 software-interrupt register contract for ABI v1.
///
/// Entry registers: RAX=syscall id, RDI=ABI_VERSION, RSI=capability handle
/// (0 means absent), RDX=arg0, R10=arg1, R8=payload pointer, R9=payload
/// length/capacity. Return registers: RAX=value, RDX=0 on success or an
/// AbiError discriminant on failure. The kernel must preserve all other GPRs.
pub mod trap_abi {
    pub const VECTOR: u8 = 0x80;
    pub const REG_SYSCALL_ID: &str = "rax";
    pub const REG_ABI_VERSION: &str = "rdi";
    pub const REG_CAPABILITY: &str = "rsi";
    pub const REG_ARG0: &str = "rdx";
    pub const REG_ARG1: &str = "r10";
    pub const REG_PAYLOAD_PTR: &str = "r8";
    pub const REG_PAYLOAD_LEN: &str = "r9";
    pub const REG_RESULT: &str = "rax";
    pub const REG_ERROR: &str = "rdx";
}

/// Invoke the ABI-v1 x86-64 interrupt gate.
///
/// # Safety
/// This is valid only in the intended Ring-3 execution environment after the
/// kernel has installed vector 0x80 and mapped the supplied payload span.
/// The kernel independently validates the complete user span and capabilities.
#[cfg(target_arch = "x86_64")]
pub unsafe fn invoke_syscall(
    syscall: Syscall,
    capability: Option<CapabilityHandle>,
    arg0: u64,
    arg1: u64,
    payload_ptr: *mut u8,
    payload_len: usize,
) -> Result<u64, AbiError> {
    use core::arch::asm;

    let mut result = syscall.id() as u64;
    let mut error = arg0;
    // SAFETY: caller guarantees this is executing as a configured Ring-3
    // process. Fixed registers are the public trap ABI; the trampoline saves
    // and restores the remaining general-purpose registers.
    unsafe {
        asm!(
            "int 0x80",
            in("rdi") ABI_VERSION as u64,
            in("rsi") capability.map_or(0, |handle| handle.0),
            inlateout("rdx") error,
            in("r10") arg1,
            in("r8") payload_ptr as u64,
            in("r9") payload_len as u64,
            inlateout("rax") result,
        );
    }
    if error == 0 {
        Ok(result)
    } else {
        Err(AbiError::from_code(error as u32).unwrap_or(AbiError::InvalidSyscall))
    }
}

/// Production userspace IPC client using the same versioned trap ABI.
#[derive(Debug, Clone, Copy, Default)]
pub struct SyscallClient;

#[cfg(target_arch = "x86_64")]
impl SyscallClient {
    /// Send bytes to a channel using a kernel-validated user pointer.
    ///
    /// # Safety
    /// Must be called from Ring 3 with the vector-0x80 ABI installed.
    pub unsafe fn ipc_send(
        &self,
        channel_id: u64,
        capability: CapabilityHandle,
        payload: &[u8],
    ) -> Result<usize, AbiError> {
        // SAFETY: forwarded preconditions are documented on this method.
        unsafe {
            invoke_syscall(
                Syscall::IpcSend,
                Some(capability),
                channel_id,
                0,
                payload.as_ptr() as *mut u8,
                payload.len(),
            )
        }
        .map(|length| length as usize)
    }

    /// Receive one message into a caller-provided writable user buffer.
    ///
    /// # Safety
    /// Must be called from Ring 3 with the vector-0x80 ABI installed. The
    /// kernel validates the complete writable span before dequeuing a message.
    pub unsafe fn ipc_receive(
        &self,
        channel_id: u64,
        capability: CapabilityHandle,
        buffer: &mut [u8],
    ) -> Result<usize, AbiError> {
        // SAFETY: forwarded preconditions are documented on this method.
        unsafe {
            invoke_syscall(
                Syscall::IpcReceive,
                Some(capability),
                channel_id,
                buffer.len() as u64,
                buffer.as_mut_ptr(),
                buffer.len(),
            )
        }
        .map(|length| length as usize)
    }
}

impl AbiError {
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            1 => Some(Self::InvalidSyscall),
            2 => Some(Self::InvalidHandle),
            3 => Some(Self::PermissionDenied),
            4 => Some(Self::InvalidPayload),
            5 => Some(Self::EndpointUnavailable),
            6 => Some(Self::ResourceExhausted),
            7 => Some(Self::AbiVersionMismatch),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trap_abi_register_contract_is_stable() {
        assert_eq!(trap_abi::VECTOR, 0x80);
        assert_eq!(trap_abi::REG_SYSCALL_ID, "rax");
        assert_eq!(trap_abi::REG_ABI_VERSION, "rdi");
        assert_eq!(trap_abi::REG_CAPABILITY, "rsi");
        assert_eq!(trap_abi::REG_ARG0, "rdx");
        assert_eq!(trap_abi::REG_ARG1, "r10");
        assert_eq!(trap_abi::REG_PAYLOAD_PTR, "r8");
        assert_eq!(trap_abi::REG_PAYLOAD_LEN, "r9");
        assert_eq!(trap_abi::REG_RESULT, "rax");
        assert_eq!(trap_abi::REG_ERROR, "rdx");
    }

    #[test]
    fn abi_error_codes_round_trip_and_unknown_codes_fail_closed() {
        assert_eq!(
            AbiError::from_code(AbiError::InvalidSyscall as u32),
            Some(AbiError::InvalidSyscall)
        );
        assert_eq!(
            AbiError::from_code(AbiError::PermissionDenied as u32),
            Some(AbiError::PermissionDenied)
        );
        assert_eq!(
            AbiError::from_code(AbiError::AbiVersionMismatch as u32),
            Some(AbiError::AbiVersionMismatch)
        );
        assert_eq!(AbiError::from_code(0), None);
        assert_eq!(AbiError::from_code(99), None);
    }

    #[test]
    fn abi_version_is_stable() {
        assert_eq!(ABI_VERSION, 0x0001_0000);
        assert!(validate_abi_version(0x0001_ffff));
        assert!(!validate_abi_version(0x0002_0000));
    }

    #[test]
    fn syscall_ids_are_stable() {
        assert_eq!(Syscall::IpcSend.id(), 0x0010);
        assert_eq!(Syscall::MonotonicTime.id(), 0x0040);
        assert_eq!(Syscall::CapabilityDelegate.id(), 0x0021);
        assert_eq!(Syscall::CapabilityRevoke.id(), 0x0022);
    }

    #[test]
    fn simple_hash_is_stable() {
        assert_eq!(simple_hash(b"abc"), simple_hash(b"abc"));
        assert_ne!(simple_hash(b"abc"), simple_hash(b"abd"));
    }

    #[test]
    fn capability_rights_are_monotonic() {
        let rights = CapabilityRights::READ;
        assert!(rights.contains(CapabilityRights::READ));
        assert!(!rights.contains(CapabilityRights::WRITE));
    }

    #[test]
    fn capability_rights_compose_without_changing_wire_bits() {
        let rights = CapabilityRights::READ | CapabilityRights::WRITE | CapabilityRights::INSPECT;
        assert_eq!(rights.bits(), 0b1_0011);
        assert!(rights.has(CapabilityRights::READ | CapabilityRights::WRITE));
        assert!(!rights.has(CapabilityRights::EXECUTE));
    }

    #[test]
    fn handshake_accepts_minor_changes() {
        assert!(
            AbiHandshake {
                major: 1,
                minor: 99
            }
            .compatible()
        );
        assert!(!AbiHandshake { major: 2, minor: 0 }.compatible());
    }
}
