//! Process and thread identity for GlobusOS user space.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessId(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThreadId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState { Created, Ready, Running, Blocked, Exited }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessInfo { pub id: ProcessId, pub state: ProcessState, pub priority: u8 }
