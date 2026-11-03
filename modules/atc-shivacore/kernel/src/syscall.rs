// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! Stable ShivaCore syscall boundary.
//!
//! The kernel owns dispatch and capability validation. Userspace only sees the
//! versioned libshivacore ABI; unknown syscall IDs and malformed requests
//! fail closed before any kernel operation is reached.

use crate::ats1000::Pid;
use crate::capability::{CapId, Rights, SyscallCapabilityPort};
use crate::rights::ObjectRights;
use libshivacore::{
    validate_abi_version, AbiError, CapabilityHandle, Syscall, MAX_SYSCALL_PAYLOAD,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyscallRequest {
    pub abi_version: u32,
    pub syscall_id: u16,
    pub capability: Option<CapabilityHandle>,
    pub arg0: u64,
    pub arg1: u64,
    pub payload_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyscallResponse {
    pub error: Option<AbiError>,
    pub value: u64,
}

impl SyscallResponse {
    const fn ok(value: u64) -> Self {
        Self { error: None, value }
    }
    const fn err(error: AbiError) -> Self {
        Self {
            error: Some(error),
            value: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchError {
    Abi(AbiError),
}

impl From<AbiError> for DispatchError {
    fn from(error: AbiError) -> Self {
        Self::Abi(error)
    }
}

pub struct SyscallDispatcher {
    monotonic_ticks: u64,
    capabilities: SyscallCapabilityPort,
}

impl SyscallDispatcher {
    pub(crate) fn new(capabilities: SyscallCapabilityPort) -> Self {
        Self {
            monotonic_ticks: 0,
            capabilities,
        }
    }

    pub fn dispatch(&mut self, pid: Pid, request: SyscallRequest) -> SyscallResponse {
        if !validate_abi_version(request.abi_version) {
            return SyscallResponse::err(AbiError::AbiVersionMismatch);
        }
        if request.payload_len > MAX_SYSCALL_PAYLOAD {
            return SyscallResponse::err(AbiError::InvalidPayload);
        }

        let syscall = match request.syscall_id {
            x if x == Syscall::Yield.id() => Syscall::Yield,
            x if x == Syscall::IpcSend.id() => Syscall::IpcSend,
            x if x == Syscall::IpcReceive.id() => Syscall::IpcReceive,
            x if x == Syscall::CapabilityQuery.id() => Syscall::CapabilityQuery,
            x if x == Syscall::HandleClose.id() => Syscall::HandleClose,
            x if x == Syscall::MonotonicTime.id() => Syscall::MonotonicTime,
            _ => return SyscallResponse::err(AbiError::InvalidSyscall),
        };

        match syscall {
            Syscall::Yield => SyscallResponse::ok(0),
            Syscall::IpcSend => {
                if !self.check_typed_capability(
                    pid,
                    request.capability,
                    request.arg0,
                    crate::rights::endpoint::SEND,
                ) {
                    return SyscallResponse::err(AbiError::PermissionDenied);
                }
                SyscallResponse::ok(0)
            }
            Syscall::IpcReceive => {
                if request.arg1 as usize > MAX_SYSCALL_PAYLOAD {
                    return SyscallResponse::err(AbiError::InvalidPayload);
                }
                if !self.check_typed_capability(
                    pid,
                    request.capability,
                    request.arg0,
                    crate::rights::endpoint::RECV,
                ) {
                    return SyscallResponse::err(AbiError::PermissionDenied);
                }
                SyscallResponse::ok(request.arg1)
            }
            Syscall::CapabilityQuery => {
                let cap = match request.capability {
                    Some(cap) => cap,
                    None => return SyscallResponse::err(AbiError::InvalidHandle),
                };
                if !self.capabilities.check_any(pid, cap.0, Rights::INSPECT) {
                    return SyscallResponse::err(AbiError::PermissionDenied);
                }
                SyscallResponse::ok(1)
            }
            Syscall::HandleClose => {
                if request.capability.is_none() {
                    return SyscallResponse::err(AbiError::InvalidHandle);
                }
                // BLOCKED / NOT IMPLEMENTED: ABI ownership and close/revocation
                // semantics are not defined. Never report a close that did not occur.
                SyscallResponse::err(AbiError::InvalidSyscall)
            }
            Syscall::MonotonicTime => {
                self.monotonic_ticks = self.monotonic_ticks.wrapping_add(1);
                SyscallResponse::ok(self.monotonic_ticks)
            }
        }
    }

    fn check_typed_capability(
        &self,
        pid: Pid,
        handle: Option<CapabilityHandle>,
        resource_id: u64,
        required: ObjectRights,
    ) -> bool {
        let Some(handle) = handle else { return false };
        self.capabilities
            .check_typed(pid, CapId(handle.0), resource_id, required)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{new_authority, ResourceType};

    fn pid(n: u32) -> Pid {
        Pid(n)
    }

    fn request(syscall: Syscall) -> SyscallRequest {
        SyscallRequest {
            abi_version: libshivacore::ABI_VERSION,
            syscall_id: syscall.id(),
            capability: None,
            arg0: 0,
            arg1: 0,
            payload_len: 0,
        }
    }

    #[test]
    fn rejects_unknown_syscall() {
        let mut d = SyscallDispatcher::new(new_authority().owner().syscall_port());
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                syscall_id: 0xffff,
                ..request(Syscall::Yield)
            },
        );
        assert_eq!(r.error, Some(AbiError::InvalidSyscall));
    }

    #[test]
    fn rejects_major_abi_mismatch() {
        let mut d = SyscallDispatcher::new(new_authority().owner().syscall_port());
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                abi_version: 0x0002_0000,
                ..request(Syscall::Yield)
            },
        );
        assert_eq!(r.error, Some(AbiError::AbiVersionMismatch));
    }

    #[test]
    fn rejects_oversized_payload() {
        let mut d = SyscallDispatcher::new(new_authority().owner().syscall_port());
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                payload_len: MAX_SYSCALL_PAYLOAD + 1,
                ..request(Syscall::Yield)
            },
        );
        assert_eq!(r.error, Some(AbiError::InvalidPayload));
    }

    #[test]
    fn ipc_requires_matching_capability_and_right() {
        let table = new_authority();
        let owner = table.owner();
        let cap = owner.create_typed(pid(1), 42, crate::rights::endpoint::SEND).unwrap();
        let mut d = SyscallDispatcher::new(table.owner().syscall_port());
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: 42,
                ..request(Syscall::IpcSend)
            },
        );
        assert_eq!(r.error, None);
        let denied = d.dispatch(
            pid(2),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: 42,
                ..request(Syscall::IpcSend)
            },
        );
        assert_eq!(denied.error, Some(AbiError::PermissionDenied));
    }

    #[test]
    fn ipc_receive_requires_endpoint_recv_right() {
        let table = new_authority();
        let cap = table
            .owner()
            .create_typed(pid(1), 42, crate::rights::endpoint::RECV).unwrap();
        let mut d = SyscallDispatcher::new(table.owner().syscall_port());
        let allowed = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: 42,
                arg1: 8,
                ..request(Syscall::IpcReceive)
            },
        );
        assert_eq!(allowed.error, None);

        let denied = d.dispatch(
            pid(2),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: 42,
                arg1: 8,
                ..request(Syscall::IpcReceive)
            },
        );
        assert_eq!(denied.error, Some(AbiError::PermissionDenied));
    }

    #[test]
    fn handle_close_is_blocked_and_never_reports_success() {
        let authority = new_authority();
        let mut dispatcher = SyscallDispatcher::new(authority.owner().syscall_port());
        let response = dispatcher.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(999)),
                ..request(Syscall::HandleClose)
            },
        );
        assert_eq!(response.error, Some(AbiError::InvalidSyscall));
        assert_eq!(response.value, 0);
    }

    #[test]
    fn monotonic_time_is_kernel_owned() {
        let mut d = SyscallDispatcher::new(new_authority().owner().syscall_port());
        let a = d.dispatch(pid(1), request(Syscall::MonotonicTime));
        let b = d.dispatch(pid(1), request(Syscall::MonotonicTime));
        assert_eq!(a.value + 1, b.value);
    }

    #[test]
    fn capability_query_requires_inspect_right() {
        let table = new_authority();
        let owner = table.owner();
        let cap = owner.create(pid(1), ResourceType::IpcChannel, 7, Rights::READ).unwrap();
        let mut d = SyscallDispatcher::new(table.owner().syscall_port());
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                ..request(Syscall::CapabilityQuery)
            },
        );
        assert_eq!(r.error, Some(AbiError::PermissionDenied));
    }
}
