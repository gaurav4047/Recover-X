//! Shared primitive types.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A scan session identifier (UUID v4).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        SessionId(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SessionId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(SessionId(Uuid::parse_str(s)?))
    }
}

/// A task identifier (UUID v4).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

impl TaskId {
    pub fn new() -> Self {
        TaskId(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Byte offset into a storage source.
pub type ByteOffset = u64;

/// Logical sector number.
pub type SectorNumber = u64;

/// Scan mode chosen by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanMode {
    /// Fast scan: filesystem metadata + deleted entries only. No carving.
    Quick,
    /// Full scan: filesystem + deleted entries + file carving + duplicate detection.
    Deep,
    /// Signature-based carving only, skipping filesystem analysis.
    FileCarving,
    /// Scan existing files on a live filesystem for structural corruption and repair them.
    CorruptedDataRecovery,
}

impl std::fmt::Display for ScanMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanMode::Quick                 => write!(f, "Quick"),
            ScanMode::Deep                  => write!(f, "Deep"),
            ScanMode::FileCarving           => write!(f, "File Carving"),
            ScanMode::CorruptedDataRecovery => write!(f, "Corrupted Data Recovery"),
        }
    }
}

/// Which recovery engines are enabled for this scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfiguration {
    pub mode: ScanMode,
    pub enable_filesystem_analysis: bool,
    pub enable_deleted_file_recovery: bool,
    pub enable_file_carving: bool,
    pub enable_duplicate_detection: bool,
    pub enable_forensic_hashing: bool,
    /// File categories of interest (empty means all).
    pub file_categories: Vec<FileCategory>,
    /// Only return files deleted on or after this Unix timestamp (seconds). None = no filter.
    pub deleted_after: Option<i64>,
    /// Only return files deleted on or before this Unix timestamp (seconds). None = no filter.
    pub deleted_before: Option<i64>,
    /// Only return files whose original path starts with this prefix (e.g. ~/Downloads).
    pub filter_prefix: Option<String>,
}

impl ScanConfiguration {
    pub fn quick() -> Self {
        Self {
            mode: ScanMode::Quick,
            enable_filesystem_analysis: true,
            enable_deleted_file_recovery: true,
            enable_file_carving: false,
            enable_duplicate_detection: false,
            enable_forensic_hashing: false,
            file_categories: vec![],
            deleted_after: None,
            deleted_before: None,
            filter_prefix: None,
        }
    }

    pub fn deep() -> Self {
        Self {
            mode: ScanMode::Deep,
            enable_filesystem_analysis: true,
            enable_deleted_file_recovery: true,
            enable_file_carving: true,
            enable_duplicate_detection: true,
            enable_forensic_hashing: true,
            file_categories: vec![],
            deleted_after: None,
            deleted_before: None,
            filter_prefix: None,
        }
    }

    pub fn file_carving() -> Self {
        Self {
            mode: ScanMode::FileCarving,
            enable_filesystem_analysis: false,
            enable_deleted_file_recovery: false,
            enable_file_carving: true,
            enable_duplicate_detection: false,
            enable_forensic_hashing: false,
            file_categories: vec![],
            deleted_after: None,
            deleted_before: None,
            filter_prefix: None,
        }
    }

    pub fn forensic() -> Self {
        Self::deep()
    }
}

/// Categories of recoverable file types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileCategory {
    Images,
    Documents,
    Videos,
    Audio,
    Archives,
    Databases,
    Other,
}

/// Confidence level assigned to a recovered file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    Low = 1,
    Medium = 2,
    High = 3,
    Certain = 4,
}

/// Status of a scan pipeline task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "Pending"),
            TaskStatus::Running => write!(f, "Running"),
            TaskStatus::Paused => write!(f, "Paused"),
            TaskStatus::Completed => write!(f, "Completed"),
            TaskStatus::Failed => write!(f, "Failed"),
            TaskStatus::Cancelled => write!(f, "Cancelled"),
        }
    }
}

/// Overall status of a scan session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Created,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Created => write!(f, "Created"),
            SessionStatus::Running => write!(f, "Running"),
            SessionStatus::Paused => write!(f, "Paused"),
            SessionStatus::Completed => write!(f, "Completed"),
            SessionStatus::Failed => write!(f, "Failed"),
            SessionStatus::Cancelled => write!(f, "Cancelled"),
        }
    }
}
