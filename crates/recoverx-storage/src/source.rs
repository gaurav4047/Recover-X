//! The `StorageSource` trait — the single abstraction every recovery engine
//! operates on.  Sources are always read-only from the perspective of the
//! recovery engine.

use std::path::PathBuf;

use recoverx_core::error::Result;
use recoverx_core::identity::SourceIdentity;
use serde::{Deserialize, Serialize};

/// Metadata about a storage source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSourceMetadata {
    /// Human-readable label
    pub label: String,
    /// Canonical path (device path or file path)
    pub path: PathBuf,
    /// Total size in bytes
    pub size_bytes: u64,
    /// Logical sector size (typically 512 or 4096)
    pub sector_size: u32,
    /// What kind of source this is
    pub source_type: StorageSourceType,
    /// Device model, if available
    pub model: Option<String>,
    /// Device serial number, if available
    pub serial: Option<String>,
    /// Detected filesystem type, if available
    pub filesystem_type: Option<String>,
    /// Filesystem volume label, if available
    pub filesystem_label: Option<String>,
    /// Whether the source was opened read-only (must always be true)
    pub is_read_only: bool,
}

impl StorageSourceMetadata {
    /// Convert to a `SourceIdentity` for scan resume verification.
    pub fn to_identity(&self) -> SourceIdentity {
        SourceIdentity {
            label: self.label.clone(),
            path: self.path.display().to_string(),
            size_bytes: self.size_bytes,
            sector_size: self.sector_size,
            model: self.model.clone(),
            serial: self.serial.clone(),
            filesystem_label: self.filesystem_label.clone(),
            filesystem_type: self.filesystem_type.clone(),
            partial_hash: None,
        }
    }
}

/// The kinds of storage source RecoverX can operate on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageSourceType {
    PhysicalDevice,
    Partition,
    DiskImage,
    ForensicImage,
}

/// The core trait every recovery engine uses to access data.
///
/// All implementations MUST be read-only. Writes are never called through
/// this trait — see `SourceWriteGuard` in `recoverx-core`.
pub trait StorageSource: Send + Sync {
    /// Read `length` bytes starting at `offset` into `buf`.
    ///
    /// Returns the number of bytes actually read (may be less at EOF or on a
    /// bad sector).  Returns `Err` for hard I/O errors.
    fn read(&mut self, offset: u64, length: usize, buf: &mut Vec<u8>) -> Result<usize>;

    /// Total size of the source in bytes.
    fn size(&self) -> u64;

    /// Logical sector size in bytes.
    fn sector_size(&self) -> u32;

    /// Rich metadata for display and identity purposes.
    fn metadata(&self) -> &StorageSourceMetadata;

    /// Build a `SourceIdentity` snapshot suitable for persistence.
    fn identity(&self) -> SourceIdentity {
        self.metadata().to_identity()
    }

    /// Whether this source supports random-access reads.
    fn is_seekable(&self) -> bool {
        true
    }

    /// Read an aligned sector by its logical sector number.
    fn read_sector(&mut self, sector_number: u64, buf: &mut Vec<u8>) -> Result<usize> {
        let offset = sector_number * self.sector_size() as u64;
        self.read(offset, self.sector_size() as usize, buf)
    }

    /// Read `count` consecutive sectors starting at `start_sector`.
    fn read_sectors(&mut self, start_sector: u64, count: u32, buf: &mut Vec<u8>) -> Result<usize> {
        let offset = start_sector * self.sector_size() as u64;
        let length = count as usize * self.sector_size() as usize;
        self.read(offset, length, buf)
    }
}
