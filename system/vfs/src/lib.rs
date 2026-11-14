//! Virtual filesystem namespace, mount policy, and persistent-disk discovery.

pub mod allocator;
/// Freispeicher-Bitmap ueber Blocknummern.
pub mod bitmap;
/// Verzeichnis-Eintraege im Speicher und ihre Kodierung.
pub mod dir_store;
/// Aufbau eines einzelnen Verzeichniseintrags.
pub mod directory;
/// Abbildung logischer auf physische Bloecke.
pub mod extent;
/// Datei-Typen, Rechte, Inodes und Handle.
pub mod file;
/// Datei-IO ueber Extents auf einem Blockgeraet.
pub mod file_store;
/// Auswertung der GUID-Partitionstabelle.
pub mod gpt;
/// Inode in Plattenform und Inode-Allocator.
pub mod inode;
/// Journal fuer Block-Schreibvorgaenge.
pub mod journal;
/// Lage der Metadatenbereiche auf dem Volume.
pub mod layout;
/// Mount-Tabelle und Standard-Einhaengepunkte.
pub mod mounts;
/// Normalisierung absoluter Pfade.
pub mod path;
/// Persistente Strukturen auf einem Blockgeraet.
pub mod persistent;
/// Wiederherstellung nach einem Abbruch.
pub mod recovery;
/// Mehrblock-Transaktionen ueber das Journal.
pub mod transaction;
/// Verzeichnisbaum ueber Verzeichniseintraegen.
pub mod tree;

pub use allocator::{AllocationError, BlockAllocator};
pub use bitmap::{BitmapError, FreeSpaceBitmap};
pub use dir_store::{DirStoreError, DirectoryStore};
pub use directory::{DirectoryError, DirectoryRecord, DirectoryType};
pub use extent::{Extent, ExtentError, ExtentMap};
pub use file::{FileError, FileHandle, FileMode, FileType, Inode, InodeId, OpenFlags};
pub use file_store::{FileStore, FileStoreError};
pub use inode::{DiskInode, INODE_SIZE, InodeAllocator, InodeDiskError};
pub use journal::{JOURNAL_RECORD_SIZE, Journal, JournalError, JournalOp, JournalRecord};
pub use layout::{LayoutError, MetadataLayout};
pub use mounts::MountTable;
pub use path::{PathError, normalize};
pub use persistent::{FsError as PersistentFsError, PersistentFs, Superblock};
pub use recovery::{RecoveryError, ReplayWrite, replay_pending};
pub use transaction::{PendingWrite, Transaction, TransactionError};
pub use tree::{DirectoryEntry, FsError, InodeTree};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ein Einhaengepunkt.
pub struct Mount {
    /// Pfad des Einhaengepunkts.
    pub mountpoint: String,
    /// Dateisystemtyp, etwa efi oder vfs.
    pub filesystem: String,
    /// True, wenn der Mount nur gelesen wird.
    pub readonly: bool,
}

/// Die sechs Einhaengepunkte, die ein System mindestens braucht.
pub fn required_mounts() -> [Mount; 6] {
    [
        Mount {
            mountpoint: "/boot".into(),
            filesystem: "efi".into(),
            readonly: true,
        },
        Mount {
            mountpoint: "/system".into(),
            filesystem: "system".into(),
            readonly: true,
        },
        Mount {
            mountpoint: "/etc".into(),
            filesystem: "config".into(),
            readonly: false,
        },
        Mount {
            mountpoint: "/home".into(),
            filesystem: "data".into(),
            readonly: false,
        },
        Mount {
            mountpoint: "/var".into(),
            filesystem: "state".into(),
            readonly: false,
        },
        Mount {
            mountpoint: "/run".into(),
            filesystem: "runtime".into(),
            readonly: false,
        },
    ]
}
