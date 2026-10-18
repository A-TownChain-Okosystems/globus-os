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
    /// Process operations are reserved in ABI v1 but may be unsupported by a kernel.
    ProcessStart = 0x0050,
    ProcessTerminate = 0x0051,
    ProcessStatus = 0x0052,
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
    UnsupportedOperation = 8,
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

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    pub const fn bits(self) -> u8 {
        self.0
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(Syscall::ProcessStart.id(), 0x0050);
        assert_eq!(Syscall::ProcessTerminate.id(), 0x0051);
        assert_eq!(Syscall::ProcessStatus.id(), 0x0052);
    }

    #[test]
    fn process_start_request_rejects_invalid_bounds_and_paths() {
        let mut request = ProcessStartRequest {
            path: [0; MAX_PROCESS_PATH],
            path_len: 5,
            priority: 10,
            flags: 0,
            args_len: 0,
        };
        request.path[..5].copy_from_slice(b"/init");
        assert_eq!(request.validate(), Ok(()));
        request.path_len = (MAX_PROCESS_PATH + 1) as u16;
        assert_eq!(request.validate(), Err(AbiError::InvalidPayload));
        request.path_len = 5;
        request.path[1] = 0;
        assert_eq!(request.validate(), Err(AbiError::InvalidPayload));
    }

    #[test]
    fn process_start_wire_decoder_requires_exact_size_and_valid_path() {
        let mut bytes = [0u8; ProcessStartRequest::WIRE_SIZE];
        bytes[..5].copy_from_slice(b"/init");
        let len_offset = MAX_PROCESS_PATH;
        bytes[len_offset..len_offset + 2].copy_from_slice(&5u16.to_le_bytes());
        bytes[len_offset + 2] = 10;
        bytes[len_offset + 4..len_offset + 8].copy_from_slice(&4u32.to_le_bytes());

        let decoded = ProcessStartRequest::decode_wire(&bytes).unwrap();
        assert_eq!(decoded.path_len, 5);
        assert_eq!(decoded.priority, 10);
        assert_eq!(decoded.args_len, 4);
        assert_eq!(
            ProcessStartRequest::decode_wire(&bytes[..bytes.len() - 1]),
            Err(AbiError::InvalidPayload)
        );

        bytes[0] = b'x';
        assert_eq!(
            ProcessStartRequest::decode_wire(&bytes),
            Err(AbiError::InvalidPayload)
        );
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


/// Maximum UTF-8 executable path length accepted by the process-start ABI.
pub const MAX_PROCESS_PATH: usize = 256;
/// Maximum argument block accepted by the process-start ABI.
pub const MAX_PROCESS_ARGS: usize = 4096;

/// Fixed-size request shape for a process start operation.
///
/// This is a wire contract only: it does not imply that the running kernel
/// implements process creation. Callers must check the syscall result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessStartRequest {
    pub path: [u8; MAX_PROCESS_PATH],
    pub path_len: u16,
    pub priority: u8,
    pub flags: u8,
    pub args_len: u32,
}

impl ProcessStartRequest {
    /// Canonical wire size: path[256] + path_len[u16] + priority[u8] +
    /// flags[u8] + args_len[u32]. Integers are little-endian on the wire.
    pub const WIRE_SIZE: usize = MAX_PROCESS_PATH + 2 + 1 + 1 + 4;

    /// Decode an exact-size byte payload without reading user pointers or allocating.
    ///
    /// Callers must first copy the bytes from userspace using a kernel-validated
    /// copy-in routine. This decoder deliberately accepts a byte slice, never an
    /// address, so it cannot dereference an untrusted pointer itself.
    pub fn decode_wire(bytes: &[u8]) -> Result<Self, AbiError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(AbiError::InvalidPayload);
        }

        let mut path = [0u8; MAX_PROCESS_PATH];
        path.copy_from_slice(&bytes[..MAX_PROCESS_PATH]);
        let request = Self {
            path,
            path_len: u16::from_le_bytes([
                bytes[MAX_PROCESS_PATH],
                bytes[MAX_PROCESS_PATH + 1],
            ]),
            priority: bytes[MAX_PROCESS_PATH + 2],
            flags: bytes[MAX_PROCESS_PATH + 3],
            args_len: u32::from_le_bytes([
                bytes[MAX_PROCESS_PATH + 4],
                bytes[MAX_PROCESS_PATH + 5],
                bytes[MAX_PROCESS_PATH + 6],
                bytes[MAX_PROCESS_PATH + 7],
            ]),
        };
        request.validate()?;
        Ok(request)
    }

    /// Validates bounds and path encoding without allocating.
    pub fn validate(&self) -> Result<(), AbiError> {
        let path_len = self.path_len as usize;
        if path_len == 0 || path_len > MAX_PROCESS_PATH || self.args_len as usize > MAX_PROCESS_ARGS {
            return Err(AbiError::InvalidPayload);
        }
        let path = &self.path[..path_len];
        if path[0] != b'/' || path.iter().any(|byte| *byte == 0) {
            return Err(AbiError::InvalidPayload);
        }
        core::str::from_utf8(path).map_err(|_| AbiError::InvalidPayload)?;
        if self.flags != 0 {
            return Err(AbiError::InvalidPayload);
        }
        Ok(())
    }
}

/// Stable process identifier returned by a future supported ProcessStart syscall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ProcessId(pub u32);

/// Process lifecycle as reported by a supported ProcessStatus syscall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ProcessStatus {
    Starting = 0,
    Ready = 1,
    Running = 2,
    Exited = 3,
    Failed = 4,
    /// Temporarily blocked on I/O, IPC, a signal, or a timer.
    Blocked = 5,
}
