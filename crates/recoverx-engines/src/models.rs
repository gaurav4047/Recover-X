//! Core data models for recovery results.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A file found during recovery analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveredFile {
    pub id: Uuid,
    pub session_id: String,
    pub name: String,
    /// Original path if reconstructed from filesystem metadata.
    pub original_path: Option<String>,
    pub size_bytes: u64,
    pub source_offset: u64,
    pub partition_index: Option<u32>,
    pub filesystem_type: Option<String>,
    pub recovery_method: RecoveryMethod,
    /// Confidence score 0–100.
    pub confidence: u8,
    pub status: FileStatus,
    pub sha256: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
    pub accessed_at: Option<DateTime<Utc>>,
    pub extension: Option<String>,
    pub mime_type: Option<String>,
    pub is_deleted: bool,
    pub is_fragmented: bool,
    pub fragments: Vec<FileFragment>,
    pub metadata: serde_json::Value,
}

impl RecoveredFile {
    pub fn confidence_label(&self) -> &'static str {
        match self.confidence {
            80..=100 => "High",
            50..=79 => "Medium",
            20..=49 => "Low",
            _ => "Very Low",
        }
    }
}

/// A contiguous byte range belonging to a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFragment {
    pub offset: u64,
    pub length: u64,
    pub order: u32,
}

/// How a file was found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryMethod {
    FilesystemMetadata,
    MftRecord,
    InodeRecord,
    DirectoryEntry,
    FileCarving,
    HybridReconstruction,
}

impl std::fmt::Display for RecoveryMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecoveryMethod::FilesystemMetadata => write!(f, "Filesystem Metadata"),
            RecoveryMethod::MftRecord => write!(f, "MFT Record"),
            RecoveryMethod::InodeRecord => write!(f, "Inode Record"),
            RecoveryMethod::DirectoryEntry => write!(f, "Directory Entry"),
            RecoveryMethod::FileCarving => write!(f, "File Carving"),
            RecoveryMethod::HybridReconstruction => write!(f, "Hybrid Reconstruction"),
        }
    }
}

/// Recoverability status of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Complete,
    Partial,
    Fragmented,
    Corrupted,
    Encrypted,
    Overwritten,
}

impl std::fmt::Display for FileStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileStatus::Complete => write!(f, "Complete"),
            FileStatus::Partial => write!(f, "Partial"),
            FileStatus::Fragmented => write!(f, "Fragmented"),
            FileStatus::Corrupted => write!(f, "Corrupted"),
            FileStatus::Encrypted => write!(f, "Encrypted"),
            FileStatus::Overwritten => write!(f, "Overwritten"),
        }
    }
}

/// A detected partition entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedPartition {
    pub index: u32,
    pub partition_table_type: PartitionTableType,
    pub type_guid: Option<String>,
    pub type_id: Option<u8>,
    pub start_lba: u64,
    pub end_lba: u64,
    pub start_offset: u64,
    pub size_bytes: u64,
    pub name: Option<String>,
    pub filesystem: Option<String>,
    pub is_bootable: bool,
    pub is_active: bool,
}

/// Partition table format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionTableType {
    Mbr,
    Gpt,
    ApfsContainer,
    Unknown,
}

impl std::fmt::Display for PartitionTableType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PartitionTableType::Mbr => write!(f, "MBR"),
            PartitionTableType::Gpt => write!(f, "GPT"),
            PartitionTableType::ApfsContainer => write!(f, "APFS Container"),
            PartitionTableType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// A detected filesystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedFilesystem {
    pub fs_type: FilesystemType,
    pub label: Option<String>,
    pub uuid: Option<String>,
    pub total_size: u64,
    pub free_size: Option<u64>,
    pub cluster_size: u32,
    pub sector_size: u32,
    pub offset_in_source: u64,
}

/// Filesystem type identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilesystemType {
    Ntfs,
    Fat32,
    ExFat,
    Ext2,
    Ext3,
    Ext4,
    Apfs,
    HfsPlus,
    Xfs,
    Btrfs,
    Iso9660,
    Unknown,
}

impl std::fmt::Display for FilesystemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilesystemType::Ntfs => write!(f, "NTFS"),
            FilesystemType::Fat32 => write!(f, "FAT32"),
            FilesystemType::ExFat => write!(f, "exFAT"),
            FilesystemType::Ext2 => write!(f, "ext2"),
            FilesystemType::Ext3 => write!(f, "ext3"),
            FilesystemType::Ext4 => write!(f, "ext4"),
            FilesystemType::Apfs => write!(f, "APFS"),
            FilesystemType::HfsPlus => write!(f, "HFS+"),
            FilesystemType::Xfs => write!(f, "XFS"),
            FilesystemType::Btrfs => write!(f, "Btrfs"),
            FilesystemType::Iso9660 => write!(f, "ISO 9660"),
            FilesystemType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// A bad sector encountered during scanning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BadSector {
    pub id: Uuid,
    pub session_id: String,
    pub sector_number: u64,
    pub byte_offset: u64,
    pub error_message: String,
    pub detected_at: DateTime<Utc>,
}

/// Result of recovering a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryResult {
    pub file_id: Uuid,
    pub name: String,
    pub destination_path: String,
    pub status: RecoveryStatus,
    pub sha256: Option<String>,
    pub size_recovered: u64,
    pub error: Option<String>,
}

/// Status of a file recovery operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryStatus {
    Success,
    Partial,
    Failed,
}

/// Preview of a recovered file (never executes content).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePreview {
    pub file_id: Uuid,
    pub preview_type: PreviewType,
    /// Base64-encoded image data, or UTF-8 text, or hex dump.
    pub content: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub note: Option<String>,
}

/// The kind of preview available.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewType {
    Image,
    Text,
    HexDump,
    MetadataOnly,
    Unavailable,
}
