// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! Stable ShivaCore syscall boundary.
//!
//! The kernel owns dispatch and capability validation. Userspace only sees the
//! versioned libshivacore ABI; unknown syscall IDs and malformed requests
//! fail closed before any kernel operation is reached.

use crate::ats1000::Pid;
use crate::capability::{CapId, CapabilityTable, ResourceType, Rights};
use crate::ipc::{ChannelId, IpcError, IpcSubsystem};
use alloc::vec::Vec;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyscallDispatcher {
    monotonic_ticks: u64,
}

impl Default for SyscallDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl SyscallDispatcher {
    pub const fn new() -> Self {
        Self { monotonic_ticks: 0 }
    }

    pub fn dispatch(
        &mut self,
        pid: Pid,
        request: SyscallRequest,
        capabilities: &mut CapabilityTable,
    ) -> SyscallResponse {
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
            x if x == Syscall::CapabilityDelegate.id() => Syscall::CapabilityDelegate,
            x if x == Syscall::CapabilityRevoke.id() => Syscall::CapabilityRevoke,
            x if x == Syscall::HandleClose.id() => Syscall::HandleClose,
            x if x == Syscall::MonotonicTime.id() => Syscall::MonotonicTime,
            _ => return SyscallResponse::err(AbiError::InvalidSyscall),
        };

        match syscall {
            Syscall::Yield => SyscallResponse::ok(0),
            // IPC requires buffers and a live kernel channel registry.
            // Use dispatch_with_ipc; never report a fake success here.
            Syscall::IpcSend | Syscall::IpcReceive => {
                SyscallResponse::err(AbiError::EndpointUnavailable)
            }
            Syscall::CapabilityQuery => {
                let cap = match request.capability {
                    Some(cap) => cap,
                    None => return SyscallResponse::err(AbiError::InvalidHandle),
                };
                if !capabilities.check_any(pid, cap.0, Rights::INSPECT) {
                    return SyscallResponse::err(AbiError::PermissionDenied);
                }
                SyscallResponse::ok(1)
            }
            Syscall::CapabilityDelegate => {
                let Some(handle) = request.capability else {
                    return SyscallResponse::err(AbiError::InvalidHandle);
                };
                let Some(source) = capabilities.get(CapId(handle.0)) else {
                    return SyscallResponse::err(AbiError::InvalidHandle);
                };
                if source.owner != pid || !source.rights.has(Rights::DELEGATE) {
                    return SyscallResponse::err(AbiError::PermissionDenied);
                }
                if request.arg0 > u32::MAX as u64 || request.arg1 > u8::MAX as u64 {
                    return SyscallResponse::err(AbiError::InvalidPayload);
                }
                let rights = Rights(request.arg1 as u8);
                if rights.0 & !Rights::ALL.0 != 0 {
                    return SyscallResponse::err(AbiError::InvalidPayload);
                }
                match capabilities.delegate(pid, CapId(handle.0), Pid(request.arg0 as u32), rights) {
                    Ok(child) => SyscallResponse::ok(child.0),
                    Err(_) => SyscallResponse::err(AbiError::PermissionDenied),
                }
            }
            Syscall::CapabilityRevoke => {
                let Some(handle) = request.capability else {
                    return SyscallResponse::err(AbiError::InvalidHandle);
                };
                let Some(cap) = capabilities.get(CapId(handle.0)) else {
                    return SyscallResponse::err(AbiError::InvalidHandle);
                };
                if cap.owner != pid || !cap.rights.has(Rights::DELEGATE) {
                    return SyscallResponse::err(AbiError::PermissionDenied);
                }
                capabilities.revoke(CapId(handle.0));
                SyscallResponse::ok(0)
            }
            Syscall::HandleClose => {
                let Some(handle) = request.capability else {
                    return SyscallResponse::err(AbiError::InvalidHandle);
                };
                let Some(cap) = capabilities.get(CapId(handle.0)) else {
                    return SyscallResponse::err(AbiError::InvalidHandle);
                };
                if cap.owner != pid {
                    return SyscallResponse::err(AbiError::PermissionDenied);
                }
                capabilities.revoke(CapId(handle.0));
                SyscallResponse::ok(0)
            }
            Syscall::MonotonicTime => {
                self.monotonic_ticks = self.monotonic_ticks.wrapping_add(1);
                SyscallResponse::ok(self.monotonic_ticks)
            }
        }
    }

    /// Dispatch IPC against the kernel-owned channel subsystem.
    /// The syscall entry must copy the send payload in from userspace.
    /// Receive bytes are exposed only after capability and channel checks succeed.
    pub fn dispatch_with_ipc(
        &mut self,
        pid: Pid,
        request: SyscallRequest,
        capabilities: &mut CapabilityTable,
        ipc: &mut IpcSubsystem,
        payload: &[u8],
        receive_buffer: &mut Vec<u8>,
    ) -> SyscallResponse {
        if !validate_abi_version(request.abi_version) {
            return SyscallResponse::err(AbiError::AbiVersionMismatch);
        }
        if request.payload_len > MAX_SYSCALL_PAYLOAD || payload.len() > MAX_SYSCALL_PAYLOAD {
            return SyscallResponse::err(AbiError::InvalidPayload);
        }
        let syscall = match request.syscall_id {
            x if x == Syscall::IpcSend.id() => Syscall::IpcSend,
            x if x == Syscall::IpcReceive.id() => Syscall::IpcReceive,
            _ => return self.dispatch(pid, request, capabilities),
        };
        let Some(handle) = request.capability else {
            return SyscallResponse::err(AbiError::InvalidHandle);
        };
        let channel_id = ChannelId(request.arg0);
        match syscall {
            Syscall::IpcSend => {
                if request.payload_len != payload.len() {
                    return SyscallResponse::err(AbiError::InvalidPayload);
                }
                if !self.check_capability(pid, Some(handle), capabilities,
                    ResourceType::IpcChannel, channel_id.0, Rights::WRITE) {
                    return SyscallResponse::err(AbiError::PermissionDenied);
                }
                match ipc.send(capabilities, pid, channel_id, payload.to_vec()) {
                    Ok(()) => SyscallResponse::ok(payload.len() as u64),
                    Err(IpcError::NoWriteCapability) => SyscallResponse::err(AbiError::PermissionDenied),
                    Err(IpcError::ChannelFull) => SyscallResponse::err(AbiError::ResourceExhausted),
                    Err(_) => SyscallResponse::err(AbiError::EndpointUnavailable),
                }
            }
            Syscall::IpcReceive => {
                let max_payload = request.arg1 as usize;
                if max_payload > MAX_SYSCALL_PAYLOAD {
                    return SyscallResponse::err(AbiError::InvalidPayload);
                }
                if !self.check_capability(pid, Some(handle), capabilities,
                    ResourceType::IpcChannel, channel_id.0, Rights::READ) {
                    return SyscallResponse::err(AbiError::PermissionDenied);
                }
                if ipc.next_message_len(channel_id).is_some_and(|len| len > max_payload) {
                    return SyscallResponse::err(AbiError::InvalidPayload);
                }
                match ipc.recv(capabilities, pid, channel_id) {
                    Ok(message) => {
                        receive_buffer.clear();
                        receive_buffer.extend_from_slice(&message.data);
                        SyscallResponse::ok(message.data.len() as u64)
                    }
                    Err(IpcError::NoReadCapability) => SyscallResponse::err(AbiError::PermissionDenied),
                    Err(_) => SyscallResponse::err(AbiError::EndpointUnavailable),
                }
            }
            _ => unreachable!(),
        }
    }

    fn check_capability(
        &self,
        pid: Pid,
        handle: Option<CapabilityHandle>,
        capabilities: &CapabilityTable,
        resource_type: ResourceType,
        resource_id: u64,
        rights: Rights,
    ) -> bool {
        let Some(handle) = handle else { return false };
        capabilities
            .get(CapId(handle.0))
            .map(|cap| {
                cap.owner == pid
                    && cap.resource_type == resource_type
                    && cap.resource_id == resource_id
                    && cap.rights.has(rights)
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                syscall_id: 0xffff,
                ..request(Syscall::Yield)
            },
            &mut CapabilityTable::new(),
        );
        assert_eq!(r.error, Some(AbiError::InvalidSyscall));
    }

    #[test]
    fn rejects_major_abi_mismatch() {
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                abi_version: 0x0002_0000,
                ..request(Syscall::Yield)
            },
            &mut CapabilityTable::new(),
        );
        assert_eq!(r.error, Some(AbiError::AbiVersionMismatch));
    }

    #[test]
    fn rejects_oversized_payload() {
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                payload_len: MAX_SYSCALL_PAYLOAD + 1,
                ..request(Syscall::Yield)
            },
            &mut CapabilityTable::new(),
        );
        assert_eq!(r.error, Some(AbiError::InvalidPayload));
    }

    #[test]
    fn ipc_requires_matching_capability_and_right() {
        let mut table = CapabilityTable::new();
        let cap = table.create(pid(1), ResourceType::IpcChannel, 42, Rights::WRITE);
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: 42,
                ..request(Syscall::IpcSend)
            },
            &mut table,
        );
        assert_eq!(r.error, None);
        let denied = d.dispatch(
            pid(2),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                arg0: 42,
                ..request(Syscall::IpcSend)
            },
            &mut table,
        );
        assert_eq!(denied.error, Some(AbiError::PermissionDenied));
    }

    #[test]
    fn delegate_syscall_attenuates_and_rejects_rights_expansion() {
        let mut table = CapabilityTable::new();
        let root = table.create(pid(1), ResourceType::Memory, 9, Rights::READ | Rights::DELEGATE);
        let mut d = SyscallDispatcher::new();
        let delegated = d.dispatch(pid(1), SyscallRequest {
            capability: Some(CapabilityHandle(root.0)),
            arg0: 2,
            arg1: Rights::READ.bits() as u64,
            ..request(Syscall::CapabilityDelegate)
        }, &mut table);
        assert_eq!(delegated.error, None);
        assert!(table.check(pid(2), ResourceType::Memory, 9, Rights::READ));
        let expanded = d.dispatch(pid(1), SyscallRequest {
            capability: Some(CapabilityHandle(root.0)),
            arg0: 3,
            arg1: (Rights::READ | Rights::WRITE).bits() as u64,
            ..request(Syscall::CapabilityDelegate)
        }, &mut table);
        assert_eq!(expanded.error, Some(AbiError::PermissionDenied));
        assert!(!table.check(pid(3), ResourceType::Memory, 9, Rights::READ));
    }

    #[test]
    fn revoke_syscall_rejects_foreign_owner_and_cascades() {
        let mut table = CapabilityTable::new();
        let root = table.create(pid(1), ResourceType::Memory, 9, Rights::READ | Rights::DELEGATE);
        let child = table.delegate(pid(1), root, pid(2), Rights::READ).unwrap();
        let mut d = SyscallDispatcher::new();
        let denied = d.dispatch(pid(2), SyscallRequest {
            capability: Some(CapabilityHandle(root.0)),
            ..request(Syscall::CapabilityRevoke)
        }, &mut table);
        assert_eq!(denied.error, Some(AbiError::PermissionDenied));
        let revoked = d.dispatch(pid(1), SyscallRequest {
            capability: Some(CapabilityHandle(root.0)),
            ..request(Syscall::CapabilityRevoke)
        }, &mut table);
        assert_eq!(revoked.error, None);
        assert!(table.get(root).is_none());
        assert!(table.get(child).is_none());
    }

    #[test]
    fn close_rejects_foreign_and_invalid_handles() {
        let mut table = CapabilityTable::new();
        let cap = table.create(pid(1), ResourceType::Memory, 9, Rights::READ);
        let mut d = SyscallDispatcher::new();
        let foreign = d.dispatch(pid(2), SyscallRequest {
            capability: Some(CapabilityHandle(cap.0)),
            ..request(Syscall::HandleClose)
        }, &mut table);
        assert_eq!(foreign.error, Some(AbiError::PermissionDenied));
        let invalid = d.dispatch(pid(1), SyscallRequest {
            capability: Some(CapabilityHandle(u64::MAX)),
            ..request(Syscall::HandleClose)
        }, &mut table);
        assert_eq!(invalid.error, Some(AbiError::InvalidHandle));
        let closed = d.dispatch(pid(1), SyscallRequest {
            capability: Some(CapabilityHandle(cap.0)),
            ..request(Syscall::HandleClose)
        }, &mut table);
        assert_eq!(closed.error, None);
        assert!(table.get(cap).is_none());
    }

    #[test]
    fn ipc_syscalls_transport_fifo_payloads_and_fail_closed() {
        let mut caps = CapabilityTable::new();
        let mut ipc = IpcSubsystem::new();
        let channel = ipc.create_channel(&mut caps, pid(1), 2);
        let write_cap = caps.list_for(pid(1)).iter()
            .find(|cap| cap.resource_type == ResourceType::IpcChannel
                && cap.resource_id == channel.0 && cap.rights.has(Rights::WRITE))
            .unwrap().id;
        let read_cap = caps.list_for(pid(1)).iter()
            .find(|cap| cap.resource_type == ResourceType::IpcChannel
                && cap.resource_id == channel.0 && cap.rights.has(Rights::READ))
            .unwrap().id;
        let mut dispatcher = SyscallDispatcher::new();
        let mut received = Vec::new();

        let sent = dispatcher.dispatch_with_ipc(pid(1), SyscallRequest {
            capability: Some(CapabilityHandle(write_cap.0)),
            arg0: channel.0,
            payload_len: 5,
            ..request(Syscall::IpcSend)
        }, &mut caps, &mut ipc, b"hello", &mut received);
        assert_eq!(sent.error, None);
        assert_eq!(sent.value, 5);
        assert_eq!(ipc.pending_messages(channel), Some(1));

        let too_small = dispatcher.dispatch_with_ipc(pid(1), SyscallRequest {
            capability: Some(CapabilityHandle(read_cap.0)),
            arg0: channel.0,
            arg1: 2,
            ..request(Syscall::IpcReceive)
        }, &mut caps, &mut ipc, &[], &mut received);
        assert_eq!(too_small.error, Some(AbiError::InvalidPayload));
        assert_eq!(ipc.pending_messages(channel), Some(1));

        let received_response = dispatcher.dispatch_with_ipc(pid(1), SyscallRequest {
            capability: Some(CapabilityHandle(read_cap.0)),
            arg0: channel.0,
            arg1: 5,
            ..request(Syscall::IpcReceive)
        }, &mut caps, &mut ipc, &[], &mut received);
        assert_eq!(received_response.error, None);
        assert_eq!(received, b"hello");
        assert_eq!(ipc.pending_messages(channel), Some(0));
    }

    #[test]
    fn ipc_syscall_rejects_foreign_capability_and_missing_channel() {
        let mut caps = CapabilityTable::new();
        let mut ipc = IpcSubsystem::new();
        let channel = ipc.create_channel(&mut caps, pid(1), 1);
        let write_cap = caps.list_for(pid(1)).iter()
            .find(|cap| cap.resource_type == ResourceType::IpcChannel
                && cap.resource_id == channel.0 && cap.rights.has(Rights::WRITE))
            .unwrap().id;
        let mut dispatcher = SyscallDispatcher::new();
        let mut received = Vec::new();
        let denied = dispatcher.dispatch_with_ipc(pid(2), SyscallRequest {
            capability: Some(CapabilityHandle(write_cap.0)),
            arg0: channel.0,
            payload_len: 1,
            ..request(Syscall::IpcSend)
        }, &mut caps, &mut ipc, b"x", &mut received);
        assert_eq!(denied.error, Some(AbiError::PermissionDenied));
        assert_eq!(ipc.pending_messages(channel), Some(0));

        let missing = dispatcher.dispatch_with_ipc(pid(1), SyscallRequest {
            capability: Some(CapabilityHandle(write_cap.0)),
            arg0: u64::MAX,
            payload_len: 1,
            ..request(Syscall::IpcSend)
        }, &mut caps, &mut ipc, b"x", &mut received);
        assert_eq!(missing.error, Some(AbiError::PermissionDenied));
    }

    #[test]
    fn monotonic_time_is_kernel_owned() {
        let mut d = SyscallDispatcher::new();
        let a = d.dispatch(
            pid(1),
            request(Syscall::MonotonicTime),
            &mut CapabilityTable::new(),
        );
        let b = d.dispatch(
            pid(1),
            request(Syscall::MonotonicTime),
            &mut CapabilityTable::new(),
        );
        assert_eq!(a.value + 1, b.value);
    }

    #[test]
    fn capability_query_requires_inspect_right() {
        let mut table = CapabilityTable::new();
        let cap = table.create(pid(1), ResourceType::IpcChannel, 7, Rights::READ);
        let mut d = SyscallDispatcher::new();
        let r = d.dispatch(
            pid(1),
            SyscallRequest {
                capability: Some(CapabilityHandle(cap.0)),
                ..request(Syscall::CapabilityQuery)
            },
            &mut table,
        );
        assert_eq!(r.error, Some(AbiError::PermissionDenied));
    }
}
