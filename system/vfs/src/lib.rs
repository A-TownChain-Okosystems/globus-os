//! Virtual filesystem namespace, mount policy, and persistent-disk discovery.

pub mod file;
pub mod gpt;
pub mod mounts;
pub mod path;

pub use file::{FileError, FileHandle, FileMode, FileType, Inode, InodeId, OpenFlags};
pub use mounts::MountTable;
pub use path::{normalize, PathError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount { pub mountpoint: String, pub filesystem: String, pub readonly: bool }

pub fn required_mounts() -> [Mount; 6] {
    [
        Mount { mountpoint: "/boot".into(), filesystem: "efi".into(), readonly: true },
        Mount { mountpoint: "/system".into(), filesystem: "system".into(), readonly: true },
        Mount { mountpoint: "/etc".into(), filesystem: "config".into(), readonly: false },
        Mount { mountpoint: "/home".into(), filesystem: "data".into(), readonly: false },
        Mount { mountpoint: "/var".into(), filesystem: "state".into(), readonly: false },
        Mount { mountpoint: "/run".into(), filesystem: "runtime".into(), readonly: false },
    ]
}
