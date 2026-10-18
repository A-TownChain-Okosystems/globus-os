//! Concrete process-start orchestration for the ShivaCore userspace model.
//!
//! The path is deliberately ordered: checked user copy-in -> ABI decode ->
//! capability-gated VFS executable read -> ELF parser -> user context -> scheduler.
//! Architecture code still owns page-table isolation, safe user-memory translation,
//! and the actual IRETQ entry point.

use crate::ats1000::{ExitCode, Pid};
use crate::capability::{CapId, CapabilityTable};
use crate::elf_loader::{ElfLoadError, ElfLoader};
use crate::user_copy::{copy_from_user, CopyInError, UserMemoryReader};
use crate::user_sched::UserScheduler;
use crate::userspace::{UserspaceError, UserspaceManager};
use crate::vfs::Vfs;
use libshivacore::{AbiError, ProcessId, ProcessStartRequest, ProcessStatus};

#[derive(Debug)]
pub enum ProcessRuntimeError {
    Abi(AbiError),
    CopyIn(CopyInError),
    Load(ElfLoadError),
    Userspace(UserspaceError),
    NotFound,
    AlreadyExited,
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
        let pid = runtime
            .start_from_user(
                &memory,
                Pid(7),
                0x2000,
                ProcessStartRequest::WIRE_SIZE,
                &vfs,
                fs_cap.0,
                1024 * 1024,
            )
            .unwrap();

        assert_eq!(runtime.status(pid).unwrap(), ProcessStatus::Ready);
        assert!(runtime.user_context(pid).unwrap().is_user_mode());
        assert!(runtime.scheduler().get_entry(Pid(pid.0)).is_some());
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
