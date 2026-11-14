//! Process and thread identity for GlobusOS user space.

/// Deterministic priority scheduler over a process table.
pub mod scheduler;
pub use scheduler::Scheduler;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
/// Identifier of a process, unique while it exists.
pub struct ProcessId(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Identifier of a thread, unique within its process.
pub struct ThreadId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Lifecycle state of a process.
pub enum ProcessState {
    /// Registered, but never scheduled yet.
    Created,
    /// Runnable and waiting for a slot.
    Ready,
    /// Currently holding the CPU.
    Running,
    /// Waiting for an event or a resource.
    Blocked,
    /// Finished; the entry stays until reaped.
    Exited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Process table entry: identity, state and scheduling priority.
pub struct ProcessInfo {
    /// Process this entry describes.
    pub id: ProcessId,
    /// Current lifecycle state.
    pub state: ProcessState,
    /// Scheduling priority; higher values are scheduled first.
    pub priority: u8,
}
