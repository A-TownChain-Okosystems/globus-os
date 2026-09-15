//! Persistent storage contracts. Actual on-disk implementations remain behind VFS services.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesystemKind {
    Efi,
    Ext4,
    Btrfs,
    GlobusState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockDevice {
    pub major: u16,
    pub minor: u16,
    pub block_size: u32,
    pub blocks: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentMount {
    pub mountpoint: String,
    pub filesystem: FilesystemKind,
    pub device: BlockDevice,
    pub readonly: bool,
}

impl PersistentMount {
    pub fn is_writable(&self) -> bool {
        !self.readonly
    }
}
