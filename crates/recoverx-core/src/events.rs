//! Structured event system for RecoverX.
//!
//! Events flow from the Recovery Orchestrator up to the UI / CLI.
//! They are designed to be serialisable for Tauri IPC.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::{SessionId, SessionStatus, TaskId, TaskStatus};

/// All events emitted by the RecoverX engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RecoverXEvent {
    /// A new scan session was created.
    ScanStarted(ScanStartedEvent),
    /// A partition table entry was detected.
    PartitionDetected(PartitionDetectedEvent),
    /// A filesystem was identified on a partition or volume.
    FilesystemDetected(FilesystemDetectedEvent),
    /// Periodic progress report during scanning.
    ScanProgress(ScanProgressEvent),
    /// A recoverable file was found.
    FileFound(FileFoundEvent),
    /// A file was successfully written to the recovery destination.
    FileRecovered(FileRecoveredEvent),
    /// A bad or unreadable sector was encountered.
    BadSector(BadSectorEvent),
    /// The scan was paused by the user.
    ScanPaused(ScanPausedEvent),
    /// The scan was resumed.
    ScanResumed(ScanResumedEvent),
    /// The scan completed successfully.
    ScanCompleted(ScanCompletedEvent),
    /// The scan failed with an error.
    ScanFailed(ScanFailedEvent),
    /// A task within the pipeline changed state.
    TaskStateChanged(TaskStateChangedEvent),
    /// Log message (forwarded from the logging layer).
    LogMessage(LogMessageEvent),
}

impl RecoverXEvent {
    /// The session this event belongs to, if applicable.
    pub fn session_id(&self) -> Option<&SessionId> {
        match self {
            RecoverXEvent::ScanStarted(e) => Some(&e.session_id),
            RecoverXEvent::PartitionDetected(e) => Some(&e.session_id),
            RecoverXEvent::FilesystemDetected(e) => Some(&e.session_id),
            RecoverXEvent::ScanProgress(e) => Some(&e.session_id),
            RecoverXEvent::FileFound(e) => Some(&e.session_id),
            RecoverXEvent::FileRecovered(e) => Some(&e.session_id),
            RecoverXEvent::BadSector(e) => Some(&e.session_id),
            RecoverXEvent::ScanPaused(e) => Some(&e.session_id),
            RecoverXEvent::ScanResumed(e) => Some(&e.session_id),
            RecoverXEvent::ScanCompleted(e) => Some(&e.session_id),
            RecoverXEvent::ScanFailed(e) => Some(&e.session_id),
            RecoverXEvent::TaskStateChanged(e) => Some(&e.session_id),
            RecoverXEvent::LogMessage(_) => None,
        }
    }
}

// ── Individual event payloads ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanStartedEvent {
    pub session_id: SessionId,
    pub source_path: String,
    pub source_size: u64,
    pub scan_mode: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionDetectedEvent {
    pub session_id: SessionId,
    pub partition_index: u32,
    pub start_offset: u64,
    pub size_bytes: u64,
    pub partition_type: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemDetectedEvent {
    pub session_id: SessionId,
    pub partition_index: Option<u32>,
    pub filesystem_type: String,
    pub label: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgressEvent {
    pub session_id: SessionId,
    pub processed_bytes: u64,
    pub total_bytes: u64,
    /// Percentage 0.0 – 100.0
    pub percent: f32,
    /// Bytes per second
    pub speed_bps: u64,
    pub files_found: u64,
    pub bad_sectors: u64,
    pub current_stage: String,
    pub eta_seconds: Option<u64>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFoundEvent {
    pub session_id: SessionId,
    pub file_id: Uuid,
    pub name: String,
    pub size_bytes: u64,
    pub category: String,
    pub confidence: String,
    pub recovery_method: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecoveredEvent {
    pub session_id: SessionId,
    pub file_id: Uuid,
    pub destination_path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BadSectorEvent {
    pub session_id: SessionId,
    pub sector_number: u64,
    pub offset: u64,
    pub error: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanPausedEvent {
    pub session_id: SessionId,
    pub last_offset: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResumedEvent {
    pub session_id: SessionId,
    pub resumed_from_offset: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanCompletedEvent {
    pub session_id: SessionId,
    pub total_bytes_processed: u64,
    pub files_found: u64,
    pub bad_sectors: u64,
    pub duration_seconds: u64,
    pub status: SessionStatus,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanFailedEvent {
    pub session_id: SessionId,
    pub error: String,
    pub last_offset: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStateChangedEvent {
    pub session_id: SessionId,
    pub task_id: TaskId,
    pub task_type: String,
    pub old_status: TaskStatus,
    pub new_status: TaskStatus,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessageEvent {
    pub level: String,
    pub message: String,
    pub module: Option<String>,
    pub timestamp: DateTime<Utc>,
}

// ── Event bus ─────────────────────────────────────────────────────────────────

use std::sync::Arc;
use tokio::sync::broadcast;

/// Capacity of the broadcast channel.
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// A cloneable handle to the event broadcast channel sender.
#[derive(Clone)]
pub struct EventBus {
    sender: Arc<broadcast::Sender<RecoverXEvent>>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        EventBus {
            sender: Arc::new(sender),
        }
    }

    /// Publish an event. Returns the number of active receivers.
    /// If there are no receivers the event is silently dropped (not an error).
    pub fn publish(&self, event: RecoverXEvent) -> usize {
        self.sender.send(event).unwrap_or(0)
    }

    /// Subscribe to receive future events.
    pub fn subscribe(&self) -> broadcast::Receiver<RecoverXEvent> {
        self.sender.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
