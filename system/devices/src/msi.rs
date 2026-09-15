//! PCI MSI/MSI-X capability contracts.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MsiMessage { pub address: u64, pub data: u32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MsixTableEntry { pub message: MsiMessage, pub masked: bool }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptMode { Legacy, Msi { vectors: u16 }, Msix { vectors: u16 } }

pub fn valid_vector_count(count: u16) -> bool { count > 0 && count <= 2048 }
