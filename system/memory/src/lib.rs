//! Virtual-memory abstractions above the ShivaCore address-space primitives.

pub mod allocator;
pub mod fault;
pub mod paging;
pub use allocator::{PageAllocator, PageRange};
pub use fault::{authorize_fault, classify, FaultAccess, FaultAction, PageFault};
pub use paging::{MapError, Mapping, PageTable, PhysicalAddress};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AddressSpace(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VirtualAddress(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageFlags { pub read: bool, pub write: bool, pub execute: bool, pub user: bool }

impl PageFlags {
    pub const fn user_read_only() -> Self { Self { read: true, write: false, execute: false, user: true } }
}
