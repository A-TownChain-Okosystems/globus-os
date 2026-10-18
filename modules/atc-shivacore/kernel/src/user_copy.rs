//! Checked copy-in boundary for data supplied by untrusted userspace.
//!
//! This module never dereferences a userspace virtual address. The architecture
//! layer must implement `UserMemoryReader` by validating the complete user range,
//! walking the caller's page tables, and copying from mapped user pages while
//! handling faults. Kernel callers use `copy_from_user` so overflow and size
//! checks are consistent before architecture-specific access occurs.

use crate::ats1000::Pid;

pub const MAX_COPY_IN_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyInError {
    NullAddress,
    AddressOverflow,
    EmptyDestination,
    TooLarge,
    UnmappedOrUnreadable,
}

/// Architecture-specific safe reader. Implementations must not use a raw
/// `copy_nonoverlapping` from the supplied address before validating mappings.
pub trait UserMemoryReader {
    fn read_user(
        &self,
        pid: Pid,
        user_address: u64,
        destination: &mut [u8],
    ) -> Result<(), CopyInError>;
}

/// Copies a bounded byte range from a process into kernel-owned memory.
///
/// The destination is always kernel-owned. The reader must validate every page
/// in the range against the given process's address space and return an error
/// rather than panic or fault the kernel on invalid user memory.
pub fn copy_from_user(
    reader: &impl UserMemoryReader,
    pid: Pid,
    user_address: u64,
    destination: &mut [u8],
) -> Result<(), CopyInError> {
    if user_address == 0 {
        return Err(CopyInError::NullAddress);
    }
    if destination.is_empty() {
        return Err(CopyInError::EmptyDestination);
    }
    if destination.len() > MAX_COPY_IN_BYTES {
        return Err(CopyInError::TooLarge);
    }
    let length = u64::try_from(destination.len()).map_err(|_| CopyInError::AddressOverflow)?;
    user_address
        .checked_add(length)
        .ok_or(CopyInError::AddressOverflow)?;
    reader.read_user(pid, user_address, destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestMemory {
        base: u64,
        bytes: Vec<u8>,
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
            let offset = usize::try_from(address - self.base)
                .map_err(|_| CopyInError::UnmappedOrUnreadable)?;
            let end = offset
                .checked_add(destination.len())
                .ok_or(CopyInError::AddressOverflow)?;
            let source = self
                .bytes
                .get(offset..end)
                .ok_or(CopyInError::UnmappedOrUnreadable)?;
            destination.copy_from_slice(source);
            Ok(())
        }
    }

    #[test]
    fn copies_only_from_the_requested_process_range() {
        let memory = TestMemory {
            base: 0x1000,
            bytes: b"/init".to_vec(),
            owner: Pid(7),
        };
        let mut out = [0; 5];
        assert_eq!(copy_from_user(&memory, Pid(7), 0x1000, &mut out), Ok(()));
        assert_eq!(&out, b"/init");
        assert_eq!(
            copy_from_user(&memory, Pid(8), 0x1000, &mut out),
            Err(CopyInError::UnmappedOrUnreadable)
        );
    }

    #[test]
    fn rejects_null_overflow_empty_and_oversized_ranges_before_read() {
        let memory = TestMemory {
            base: 0x1000,
            bytes: vec![0; 8],
            owner: Pid(7),
        };
        let mut byte = [0];
        assert_eq!(
            copy_from_user(&memory, Pid(7), 0, &mut byte),
            Err(CopyInError::NullAddress)
        );
        assert_eq!(
            copy_from_user(&memory, Pid(7), u64::MAX, &mut byte),
            Err(CopyInError::AddressOverflow)
        );
        assert_eq!(
            copy_from_user(&memory, Pid(7), 0x1000, &mut []),
            Err(CopyInError::EmptyDestination)
        );
        let mut oversized = vec![0; MAX_COPY_IN_BYTES + 1];
        assert_eq!(
            copy_from_user(&memory, Pid(7), 0x1000, &mut oversized),
            Err(CopyInError::TooLarge)
        );
    }
}
