//! Pipeline task model.
//!
//! Each stage of the recovery pipeline is represented as a `PipelineTask`.
//! Tasks are ordered by priority and tracked individually.

use chrono::{DateTime, Utc};
use recoverx_core::types::{SessionId, TaskId, TaskStatus};
use serde::{Deserialize, Serialize};

/// The type of work a pipeline task performs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    /// Validate and characterise the source.
    SourceValidation,
    /// Detect partition table (MBR / GPT).
    PartitionDetection,
    /// Identify filesystems on each partition.
    FilesystemDetection,
    /// Parse filesystem metadata (directory trees, inodes, MFT).
    FilesystemMetadataAnalysis,
    /// Recover deleted directory entries.
    DeletedFileAnalysis,
    /// Signature-based file carving across raw sectors.
    FileCarving,
    /// Normalise and deduplicate discovered file records.
    ResultNormalization,
    /// Detect duplicate files by content hash.
    DuplicateDetection,
    /// Assign confidence scores to recovered files.
    ConfidenceScoring,
    /// Build the final results index.
    IndexResults,
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskType::SourceValidation => write!(f, "Source Validation"),
            TaskType::PartitionDetection => write!(f, "Partition Detection"),
            TaskType::FilesystemDetection => write!(f, "Filesystem Detection"),
            TaskType::FilesystemMetadataAnalysis => write!(f, "Filesystem Metadata Analysis"),
            TaskType::DeletedFileAnalysis => write!(f, "Deleted File Analysis"),
            TaskType::FileCarving => write!(f, "File Carving"),
            TaskType::ResultNormalization => write!(f, "Result Normalization"),
            TaskType::DuplicateDetection => write!(f, "Duplicate Detection"),
            TaskType::ConfidenceScoring => write!(f, "Confidence Scoring"),
            TaskType::IndexResults => write!(f, "Index Results"),
        }
    }
}

/// A single task within the recovery pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTask {
    /// Unique task identifier.
    pub id: TaskId,
    /// Session this task belongs to.
    pub session_id: SessionId,
    /// What this task does.
    pub task_type: TaskType,
    /// Execution priority (lower = higher priority).
    pub priority: u8,
    /// Current status.
    pub status: TaskStatus,
    /// Progress percentage (0.0 – 100.0).
    pub progress: f32,
    /// Bytes processed by this task so far.
    pub bytes_processed: u64,
    /// When the task was created.
    pub created_at: DateTime<Utc>,
    /// When the task actually started executing.
    pub started_at: Option<DateTime<Utc>>,
    /// When the task finished (success, failure, or cancellation).
    pub completed_at: Option<DateTime<Utc>>,
    /// Error message if the task failed.
    pub error: Option<String>,
}

impl PipelineTask {
    pub fn new(session_id: SessionId, task_type: TaskType, priority: u8) -> Self {
        PipelineTask {
            id: TaskId::new(),
            session_id,
            task_type,
            priority,
            status: TaskStatus::Pending,
            progress: 0.0,
            bytes_processed: 0,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error: None,
        }
    }

    pub fn mark_running(&mut self) {
        self.status = TaskStatus::Running;
        self.started_at = Some(Utc::now());
    }

    pub fn mark_completed(&mut self) {
        self.status = TaskStatus::Completed;
        self.progress = 100.0;
        self.completed_at = Some(Utc::now());
    }

    pub fn mark_failed(&mut self, error: impl Into<String>) {
        self.status = TaskStatus::Failed;
        self.error = Some(error.into());
        self.completed_at = Some(Utc::now());
    }

    pub fn mark_cancelled(&mut self) {
        self.status = TaskStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }

    pub fn update_progress(&mut self, progress: f32, bytes_processed: u64) {
        self.progress = progress.clamp(0.0, 100.0);
        self.bytes_processed = bytes_processed;
    }

    /// Duration of this task in seconds, if it has started.
    pub fn duration_seconds(&self) -> Option<i64> {
        let start = self.started_at?;
        let end = self.completed_at.unwrap_or_else(Utc::now);
        Some((end - start).num_seconds())
    }
}

/// Build the default ordered pipeline for a given scan configuration.
pub fn build_pipeline(
    session_id: &SessionId,
    config: &recoverx_core::types::ScanConfiguration,
) -> Vec<PipelineTask> {
    let mut tasks = Vec::new();
    let mut priority: u8 = 0;

    let mut add = |task_type: TaskType| {
        tasks.push(PipelineTask::new(session_id.clone(), task_type, priority));
        priority += 1;
    };

    // Always run these
    add(TaskType::SourceValidation);
    add(TaskType::PartitionDetection);
    add(TaskType::FilesystemDetection);

    if config.enable_filesystem_analysis {
        add(TaskType::FilesystemMetadataAnalysis);
    }

    if config.enable_deleted_file_recovery {
        add(TaskType::DeletedFileAnalysis);
    }

    if config.enable_file_carving {
        add(TaskType::FileCarving);
    }

    // Post-processing
    add(TaskType::ResultNormalization);

    if config.enable_duplicate_detection {
        add(TaskType::DuplicateDetection);
    }

    add(TaskType::ConfidenceScoring);
    add(TaskType::IndexResults);

    tasks
}

#[cfg(test)]
mod tests {
    use super::*;
    use recoverx_core::types::{ScanConfiguration, SessionId};

    #[test]
    fn deep_scan_pipeline_includes_carving() {
        let session_id = SessionId::new();
        let config = ScanConfiguration::deep();
        let tasks = build_pipeline(&session_id, &config);
        assert!(tasks.iter().any(|t| t.task_type == TaskType::FileCarving));
        assert!(tasks
            .iter()
            .any(|t| t.task_type == TaskType::DuplicateDetection));
    }

    #[test]
    fn quick_scan_pipeline_excludes_carving() {
        let session_id = SessionId::new();
        let config = ScanConfiguration::quick();
        let tasks = build_pipeline(&session_id, &config);
        assert!(!tasks.iter().any(|t| t.task_type == TaskType::FileCarving));
    }

    #[test]
    fn all_tasks_start_as_pending() {
        let session_id = SessionId::new();
        let tasks = build_pipeline(&session_id, &ScanConfiguration::forensic());
        assert!(tasks.iter().all(|t| t.status == TaskStatus::Pending));
    }
}
