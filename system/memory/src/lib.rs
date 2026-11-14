//! Virtual-memory abstractions above the ShivaCore address-space primitives.

pub mod allocator;
/// Seitenfehler-Klassifikation und fail-closed Behandlung.
pub mod fault;
/// Deterministischer Allocator fuer physische Frames.
pub mod frames;
/// Abbildung virtueller auf physische Adressen.
pub mod paging;
pub use allocator::{PageAllocator, PageRange};
pub use fault::{FaultAccess, FaultAction, PageFault, authorize_fault, classify};
pub use frames::{FrameAllocator, FrameError, FrameRange};
pub use paging::{MapError, Mapping, PageTable, PhysicalAddress};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
/// Identitaet eines Adressraums; jede Zahl bezeichnet genau einen Raum.
pub struct AddressSpace(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
/// Virtuelle Adresse.
pub struct VirtualAddress(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Rechte einer Abbildung.
pub struct PageFlags {
    /// Lesen erlaubt.
    pub read: bool,
    /// Schreiben erlaubt.
    pub write: bool,
    /// Ausfuehren erlaubt.
    pub execute: bool,
    /// Aus dem Userspace erreichbar.
    pub user: bool,
}

impl PageFlags {
    /// Lesen erlaubt, Schreiben und Ausfuehren nicht, aus dem Userspace erreichbar.
    pub const fn user_read_only() -> Self {
        Self {
            read: true,
            write: false,
            execute: false,
            user: true,
        }
    }
}
