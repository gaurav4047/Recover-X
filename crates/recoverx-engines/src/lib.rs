//! RecoverX Recovery Engines
//!
//! Contains all storage analysis engines:
//! - Partition detection (MBR + GPT)
//! - Filesystem detection and analysis (NTFS, FAT32/exFAT, ext2/3/4, APFS)
//! - File carving engine with extensible signature database
//! - Recovery manager (safe file output with SHA-256 verification)
//! - Trash/Recycle Bin scanner

pub mod carving;
pub mod directory;
pub mod filesystem;
pub mod models;
pub mod partition;
pub mod recovery;
pub mod reports;
pub mod trash;

pub use carving::FileCarver;
pub use directory::DirectoryScanner;
pub use filesystem::{
    ApfsAnalyzer, Ext4Analyzer, Fat32Analyzer, FilesystemDetector, NtfsAnalyzer,
};
pub use models::*;
pub use partition::PartitionDetector;
pub use recovery::RecoveryManager;
pub use trash::TrashScanner;
