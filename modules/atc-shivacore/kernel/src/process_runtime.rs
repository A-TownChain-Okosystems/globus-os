//! Concrete process-start orchestration for the ShivaCore userspace model.
//!
//! The path is deliberately ordered: checked user copy-in -> ABI decode ->
//! capability-gated VFS executable read -> ELF parser -> user context -> scheduler.
//! Architecture code still owns page-table isolation, safe user-memory translation,
//! and the actual IRETQ entry point.

use crate::ats1000::{ExitCode, Pid};
use crate::capability::{CapId, CapabilityError, CapabilityTable, ResourceType, Rights};
use crate::elf_loader::{ElfLoadError, ElfLoader};
use crate::user_copy::{copy_from_user, CopyInError, UserMemoryReader};
use crate::user_sched::UserScheduler;
use crate::userspace::{UserspaceError, UserspaceManager};
use crate::vfs::{Vfs, VfsError};
use crate::syscall::{SyscallRequest, SyscallResponse};
use libshivacore::{AbiError, ProcessId, ProcessStartRequest, ProcessStatus, Syscall};

#[derive(Debug)]
pub enum ProcessRuntimeError {
    Abi(AbiError),
    CopyIn(CopyInError),
    Load(ElfLoadError),
    Userspace(UserspaceError),
    NotFound,
    AlreadyExited,
    Capability(CapabilityError),
}

/// Trusted kernel-side capability grant selected by the init/service policy.
/// Never populate `policy_ceiling` from an untrusted ProcessStart payload.
#[derive(Debug, Clone, Copy)]
pub struct InitialCapabilityGrant {
    pub source: CapId,
    pub rights: Rights,
    pub policy_ceiling: Rights,
}

pub struct ProcessRuntime {
    userspace: UserspaceManager,
    scheduler: UserScheduler,
}

impl Default for ProcessRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessRuntime {
    pub fn new() -> Self {
        Self {
            userspace: UserspaceManager::new(),
            scheduler: UserScheduler::new(),
        }
    }

    /// Start an executable requested by userspace.
    ///
    /// The ABI payload itself is copied into a fixed kernel buffer. Argument
    /// blocks are rejected until their own separately validated copy-in contract
    /// is implemented; silently ignoring args would be unsafe and misleading.
    pub fn start_from_user(
        &mut self,
        reader: &impl UserMemoryReader,
        caller: Pid,
        request_address: u64,
        request_len: usize,
        vfs: &Vfs,
        fs_capability: u64,
        max_executable_bytes: usize,
    ) -> Result<ProcessId, ProcessRuntimeError> {
        if request_len != ProcessStartRequest::WIRE_SIZE {
            return Err(ProcessRuntimeError::Abi(AbiError::InvalidPayload));
        }

        let mut wire = [0u8; ProcessStartRequest::WIRE_SIZE];
        copy_from_user(reader, caller, request_address, &mut wire)
            .map_err(ProcessRuntimeError::CopyIn)?;
        let request = ProcessStartRequest::decode_wire(&wire).map_err(ProcessRuntimeError::Abi)?;
        if request.args_len != 0 {
            return Err(ProcessRuntimeError::Abi(AbiError::UnsupportedOperation));
        }

        let path = core::str::from_utf8(&request.path[..request.path_len as usize])
            .map_err(|_| ProcessRuntimeError::Abi(AbiError::InvalidPayload))?;
        let pid = ElfLoader::load_elf_from_vfs(
            &mut self.userspace,
            &mut self.scheduler,
            vfs,
            path,
            caller.0 as u64,
            fs_capability,
            request.priority,
            max_executable_bytes,
        )
        .map_err(ProcessRuntimeError::Load)?;
        Ok(ProcessId(pid.0))
    }

    /// Handle the three process ABI calls with an explicit kernel context.
    ///
    /// The interrupt/syscall entry layer must provide a `UserMemoryReader`
    /// backed by the calling process's active page tables. The ordinary dispatcher
    /// remains fail-closed unless this context-bearing entry point is used.
    pub fn dispatch_process_syscall(
        &mut self,
        caller: Pid,
        request: SyscallRequest,
        reader: &impl UserMemoryReader,
        vfs: &Vfs,
        process_capabilities: &mut CapabilityTable,
        max_executable_bytes: usize,
    ) -> SyscallResponse {
        if !libshivacore::validate_abi_version(request.abi_version) {
            return SyscallResponse::err(AbiError::AbiVersionMismatch);
        }
        if request.payload_len > libshivacore::MAX_SYSCALL_PAYLOAD {
            return SyscallResponse::err(AbiError::InvalidPayload);
        }

        match request.syscall_id {
            id if id == Syscall::ProcessStart.id() => {
                let Some(fs_cap) = request.capability else {
                    return SyscallResponse::err(AbiError::InvalidHandle);
                };
                match self.start_from_user(
                    reader,
                    caller,
                    request.arg0,
                    request.payload_len,
                    vfs,
                    fs_cap.0,
                    max_executable_bytes,
                ) {
                    Ok(pid) => SyscallResponse::ok(pid.0 as u64),
                    Err(error) => SyscallResponse::err(Self::abi_error(&error)),
                }
            }
            id if id == Syscall::ProcessTerminate.id() => {
                if request.payload_len != 0 || request.arg0 > u32::MAX as u64 || request.arg1 > 255 {
                    return SyscallResponse::err(AbiError::InvalidPayload);
                }
                let target = Pid(request.arg0 as u32);
                if target != caller
                    && !Self::has_process_capability(
                        process_capabilities,
                        caller,
                        target,
                        request.capability,
                        Rights::WRITE,
                    )
                {
                    return SyscallResponse::err(AbiError::PermissionDenied);
                }
                match self.terminate(
                    ProcessId(target.0),
                    request.arg1 as i32,
                    process_capabilities,
                ) {
                    Ok(()) => SyscallResponse::ok(0),
                    Err(error) => SyscallResponse::err(Self::abi_error(&error)),
                }
            }
            id if id == Syscall::ProcessStatus.id() => {
                if request.payload_len != 0 || request.arg0 > u32::MAX as u64 {
                    return SyscallResponse::err(AbiError::InvalidPayload);
                }
                let target = Pid(request.arg0 as u32);
                if target != caller
                    && !Self::has_process_capability(
                        process_capabilities,
                        caller,
                        target,
                        request.capability,
                        Rights::INSPECT,
                    )
                {
                    return SyscallResponse::err(AbiError::PermissionDenied);
                }
                match self.status(ProcessId(target.0)) {
                    Ok(status) => SyscallResponse::ok(status as u64),
                    Err(error) => SyscallResponse::err(Self::abi_error(&error)),
                }
            }
            _ => SyscallResponse::err(AbiError::InvalidSyscall),
        }
    }

    fn has_process_capability(
        capabilities: &CapabilityTable,
        caller: Pid,
        target: Pid,
        handle: Option<libshivacore::CapabilityHandle>,
        required: Rights,
    ) -> bool {
        let Some(handle) = handle else { return false };
        capabilities
            .get(CapId(handle.0))
            .map(|cap| {
                cap.owner == caller
                    && cap.resource_type == ResourceType::Process
                    && cap.resource_id == target.0 as u64
                    && cap.rights.has(required)
            })
            .unwrap_or(false)
    }

    fn abi_error(error: &ProcessRuntimeError) -> AbiError {
        match error {
            ProcessRuntimeError::Abi(error) => *error,
            ProcessRuntimeError::CopyIn(CopyInError::UnmappedOrUnreadable)
            | ProcessRuntimeError::CopyIn(CopyInError::NullAddress)
            | ProcessRuntimeError::CopyIn(CopyInError::AddressOverflow)
            | ProcessRuntimeError::CopyIn(CopyInError::EmptyDestination)
            | ProcessRuntimeError::CopyIn(CopyInError::TooLarge) => AbiError::InvalidPayload,
            ProcessRuntimeError::Load(ElfLoadError::Vfs(VfsError::PermissionDenied)) => {
                AbiError::PermissionDenied
            }
            ProcessRuntimeError::Load(ElfLoadError::Vfs(VfsError::NotFound))
            | ProcessRuntimeError::NotFound
            | ProcessRuntimeError::AlreadyExited => AbiError::InvalidHandle,
            ProcessRuntimeError::Load(ElfLoadError::Userspace(UserspaceError::MaxProcessesReached)) => {
                AbiError::ResourceExhausted
            }
            ProcessRuntimeError::Capability(_) => AbiError::PermissionDenied,
            ProcessRuntimeError::Load(_)
            | ProcessRuntimeError::Userspace(_) => AbiError::InvalidPayload,
        }
    }

    /// Transfer policy-approved initial capabilities to a newly launched process.
    /// Each grant is attenuated against both the source capability and the policy
    /// ceiling. A partial failure revokes grants already made and marks the child
    /// failed, rather than leaving a half-initialized service alive.
    pub fn grant_initial_capabilities(
        &mut self,
        caller: Pid,
        child: ProcessId,
        grants: &[InitialCapabilityGrant],
        capabilities: &mut CapabilityTable,
    ) -> Result<alloc::vec::Vec<CapId>, ProcessRuntimeError> {
        if self.scheduler.process_status(Pid(child.0)).is_none() {
            return Err(ProcessRuntimeError::NotFound);
        }
        let mut delegated = alloc::vec::Vec::new();
        for grant in grants {
            match capabilities.delegate_authorized(
                caller,
                grant.source,
                Pid(child.0),
                grant.rights,
                grant.policy_ceiling,
            ) {
                Ok(id) => delegated.push(id),
                Err(error) => {
                    for id in delegated.iter().copied() {
                        capabilities.revoke(id);
                    }
                    let _ = self.terminate(child, 1, capabilities);
                    return Err(ProcessRuntimeError::Capability(error));
                }
            }
        }
        Ok(delegated)
    }

    /// Report the status from the scheduler's authoritative process entry.
    pub fn status(&self, pid: ProcessId) -> Result<ProcessStatus, ProcessRuntimeError> {
        self.scheduler
            .process_status(Pid(pid.0))
            .ok_or(ProcessRuntimeError::NotFound)
    }

    /// Terminate a scheduled process, update its saved user context, and revoke
    /// every capability owned by that PID from the supplied kernel capability table.
    pub fn terminate(
        &mut self,
        pid: ProcessId,
        exit_code: ExitCode,
        capabilities: &mut CapabilityTable,
    ) -> Result<(), ProcessRuntimeError> {
        let kernel_pid = Pid(pid.0);
        if self.scheduler.process_status(kernel_pid).is_none() {
            return Err(ProcessRuntimeError::NotFound);
        }
        if matches!(
            self.scheduler.process_status(kernel_pid),
            Some(ProcessStatus::Exited | ProcessStatus::Failed)
        ) {
            return Err(ProcessRuntimeError::AlreadyExited);
        }
        let context = self
            .userspace
            .get_context_mut(kernel_pid)
            .ok_or(ProcessRuntimeError::NotFound)?;
        context.exit(exit_code);
        if !self.scheduler.terminate_process(kernel_pid, exit_code) {
            return Err(ProcessRuntimeError::AlreadyExited);
        }

        let owned: alloc::vec::Vec<CapId> = capabilities
            .list_for(kernel_pid)
            .iter()
            .map(|cap| cap.id)
            .collect();
        for cap in owned {
            capabilities.revoke(cap);
        }
        Ok(())
    }

    /// Read-only access for architecture integration and tests.
    pub fn scheduler(&self) -> &UserScheduler {
        &self.scheduler
    }

    pub fn user_context(
        &self,
        pid: ProcessId,
    ) -> Result<&crate::userspace::UserContext, ProcessRuntimeError> {
        self.userspace
            .get_context(Pid(pid.0))
            .ok_or(ProcessRuntimeError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilityTable, ResourceType, Rights};
    use crate::user_copy::CopyInError;
    use crate::vfs::{OpenMode, Vfs};
    use alloc::sync::Arc;
    use spin::Mutex;

    struct TestMemory {
        base: u64,
        bytes: alloc::vec::Vec<u8>,
        owner: Pid,
    }

    impl UserMemoryReader for TestMemory {
        fn read_user(
            &self,
            pid: Pid,
            address: u64,
            destination: &mut [u8],
        ) -> Result<(), CopyInError> {
            if pid != self.owner || address < self.base {
                return Err(CopyInError::UnmappedOrUnreadable);
            }
            let start = usize::try_from(address - self.base)
                .map_err(|_| CopyInError::UnmappedOrUnreadable)?;
            let end = start
                .checked_add(destination.len())
                .ok_or(CopyInError::AddressOverflow)?;
            destination.copy_from_slice(
                self.bytes.get(start..end).ok_or(CopyInError::UnmappedOrUnreadable)?,
            );
            Ok(())
        }
    }

    #[test]
    fn user_request_loads_elf_from_vfs_and_registers_ring3_context() {
        let caps = Arc::new(Mutex::new(CapabilityTable::new()));
        let fs_cap = caps.lock().create(
            Pid(7),
            ResourceType::FileSystem,
            1,
            Rights::READ | Rights::WRITE | Rights::EXEC | Rights::DELEGATE,
        );
        let mut vfs = Vfs::new(caps.clone());
        vfs.create_file("/init", 7, fs_cap.0).unwrap();
        let elf = ElfLoader::create_minimal_elf(0x0040_0000, &[0x90, 0xF4]);
        let fd = vfs.open("/init", OpenMode::Write, 7, fs_cap.0).unwrap();
        vfs.write(fd, &elf, 7, fs_cap.0).unwrap();
        vfs.close(fd).unwrap();
        vfs.set_file_permissions("/init", 7, fs_cap.0, Rights::READ | Rights::EXEC)
            .unwrap();

        let mut wire = [0u8; ProcessStartRequest::WIRE_SIZE];
        wire[..5].copy_from_slice(b"/init");
        let offset = libshivacore::MAX_PROCESS_PATH;
        wire[offset..offset + 2].copy_from_slice(&5u16.to_le_bytes());
        wire[offset + 2] = 10;
        wire[offset + 4..offset + 8].copy_from_slice(&0u32.to_le_bytes());
        let memory = TestMemory {
            base: 0x2000,
            bytes: wire.to_vec(),
            owner: Pid(7),
        };

        let mut runtime = ProcessRuntime::new();
        let general_capabilities = CapabilityTable::new();
        let mut process_capabilities = CapabilityTable::new();
        let mut dispatcher = crate::syscall::SyscallDispatcher::new();
        let start_response = dispatcher.dispatch_with_process_runtime(
            Pid(7),
            SyscallRequest {
                abi_version: libshivacore::ABI_VERSION,
                syscall_id: Syscall::ProcessStart.id(),
                capability: Some(libshivacore::CapabilityHandle(fs_cap.0)),
                arg0: 0x2000,
                arg1: 0,
                payload_len: ProcessStartRequest::WIRE_SIZE,
            },
            &general_capabilities,
            &mut runtime,
            &memory,
            &vfs,
            &mut process_capabilities,
            1024 * 1024,
        );
        assert_eq!(start_response.error, None);
        let pid = ProcessId(start_response.value as u32);

        assert_eq!(runtime.status(pid).unwrap(), ProcessStatus::Ready);
        assert!(runtime.user_context(pid).unwrap().is_user_mode());
        assert!(runtime.scheduler().get_entry(Pid(pid.0)).is_some());

        let source_cap = process_capabilities.create(
            Pid(7),
            ResourceType::IpcChannel,
            42,
            Rights::READ | Rights::WRITE | Rights::DELEGATE,
        );
        let grants = [InitialCapabilityGrant {
            source: source_cap,
            rights: Rights::READ,
            policy_ceiling: Rights::READ,
        }];
        let child_caps = runtime
            .grant_initial_capabilities(Pid(7), pid, &grants, &mut process_capabilities)
            .unwrap();
        assert_eq!(child_caps.len(), 1);
        let delegated = process_capabilities.get(child_caps[0]).unwrap();
        assert_eq!(delegated.owner, Pid(pid.0));
        assert_eq!(delegated.rights, Rights::READ);
        assert!(!delegated.rights.has(Rights::WRITE));

        let process_cap = process_capabilities.create(
            Pid(7),
            ResourceType::Process,
            pid.0 as u64,
            Rights::INSPECT | Rights::WRITE,
        );
        let status_response = dispatcher.dispatch_with_process_runtime(
            Pid(7),
            SyscallRequest {
                abi_version: libshivacore::ABI_VERSION,
                syscall_id: Syscall::ProcessStatus.id(),
                capability: Some(libshivacore::CapabilityHandle(process_cap.0)),
                arg0: pid.0 as u64,
                arg1: 0,
                payload_len: 0,
            },
            &general_capabilities,
            &mut runtime,
            &memory,
            &vfs,
            &mut process_capabilities,
            1024 * 1024,
        );
        assert_eq!(status_response.error, None);
        assert_eq!(status_response.value, ProcessStatus::Ready as u64);

        let terminate_response = dispatcher.dispatch_with_process_runtime(
            Pid(7),
            SyscallRequest {
                abi_version: libshivacore::ABI_VERSION,
                syscall_id: Syscall::ProcessTerminate.id(),
                capability: Some(libshivacore::CapabilityHandle(process_cap.0)),
                arg0: pid.0 as u64,
                arg1: 0,
                payload_len: 0,
            },
            &general_capabilities,
            &mut runtime,
            &memory,
            &vfs,
            &mut process_capabilities,
            1024 * 1024,
        );
        assert_eq!(terminate_response.error, None);
        assert_eq!(runtime.status(pid).unwrap(), ProcessStatus::Exited);
    }

    #[test]
    fn rejects_argument_block_until_its_copy_in_contract_exists() {
        let caps = Arc::new(Mutex::new(CapabilityTable::new()));
        let fs_cap = caps.lock().create(
            Pid(7),
            ResourceType::FileSystem,
            1,
            Rights::READ | Rights::WRITE | Rights::EXEC,
        );
        let vfs = Vfs::new(caps);
        let mut wire = [0u8; ProcessStartRequest::WIRE_SIZE];
        wire[..5].copy_from_slice(b"/init");
        let offset = libshivacore::MAX_PROCESS_PATH;
        wire[offset..offset + 2].copy_from_slice(&5u16.to_le_bytes());
        wire[offset + 4..offset + 8].copy_from_slice(&1u32.to_le_bytes());
        let memory = TestMemory { base: 0x2000, bytes: wire.to_vec(), owner: Pid(7) };
        let mut runtime = ProcessRuntime::new();
        assert!(matches!(
            runtime.start_from_user(
                &memory, Pid(7), 0x2000, ProcessStartRequest::WIRE_SIZE,
                &vfs, fs_cap.0, 1024
            ),
            Err(ProcessRuntimeError::Abi(AbiError::UnsupportedOperation))
        ));
    }
}
