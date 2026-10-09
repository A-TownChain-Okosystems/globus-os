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
}

impl Syscall {
    pub const fn id(self) -> u16 {
        self as u16
    }

    pub const fn from_id(id: u16) -> Option<Self> {
        match id {
            0x0001 => Some(Self::Yield),
            0x0010 => Some(Self::IpcSend),
            0x0011 => Some(Self::IpcReceive),
            0x0020 => Some(Self::CapabilityQuery),
            0x0030 => Some(Self::HandleClose),
            0x0040 => Some(Self::MonotonicTime),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyscallSpec {
    pub syscall: Syscall,
    pub blocking: bool,
    pub required_right: Option<CapabilityRight>,
}

impl SyscallSpec {
    pub const fn for_syscall(syscall: Syscall) -> Self {
        match syscall {
            Syscall::Yield => Self {
                syscall,
                blocking: false,
                required_right: None,
            },
            Syscall::IpcSend => Self {
                syscall,
                blocking: false,
                required_right: Some(CapabilityRight::Write),
            },
            Syscall::IpcReceive => Self {
                syscall,
                blocking: false,
                required_right: Some(CapabilityRight::Read),
            },
            Syscall::CapabilityQuery => Self {
                syscall,
                blocking: false,
                required_right: Some(CapabilityRight::Inspect),
            },
            Syscall::HandleClose => Self {
                syscall,
                blocking: false,
                required_right: None,
            },
            Syscall::MonotonicTime => Self {
                syscall,
                blocking: false,
                required_right: None,
            },
        }
    }
}

pub const SYSCALL_RESERVED_RANGES: &[(u16, u16)] = &[
    (0x0000, 0x000f),
    (0x0010, 0x001f),
    (0x0020, 0x002f),
    (0x0030, 0x003f),
    (0x0040, 0x004f),
    (0x0050, 0x005f),
    (0x0060, 0x006f),
    (0x0070, 0x007f),
    (0x0080, 0x008f),
    (0x0090, 0x009f),
    (0x00a0, 0x00af),
    (0x00b0, 0x00bf),
    (0x00c0, 0x00cf),
];

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
        assert_eq!(Syscall::from_id(0x0011), Some(Syscall::IpcReceive));
        assert_eq!(Syscall::from_id(0xffff), None);
    }

    #[test]
    fn syscall_specs_expose_authority_contract() {
        assert_eq!(
            SyscallSpec::for_syscall(Syscall::IpcSend).required_right,
            Some(CapabilityRight::Write)
        );
        assert_eq!(
            SyscallSpec::for_syscall(Syscall::CapabilityQuery).required_right,
            Some(CapabilityRight::Inspect)
        );
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
