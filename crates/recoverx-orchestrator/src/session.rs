//! Scan session model.
//!
//! A `ScanSession` represents one complete run of the recovery pipeline.
//! Sessions are persisted to SQLite so they can survive application restarts.

use chrono::{DateTime, Utc};
use recoverx_core::{
    identity::SourceIdentity,
    types::{ScanConfiguration, SessionId, SessionStatus},
};
use serde::{Deserialize, Serialize};

/// A persisted scan session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSession {
    /// Unique identifier for this session.
    pub id: SessionId,
    /// Stable identity of the source media.
    pub source_identity: SourceIdentity,
    /// Total source size at scan start.
    pub source_size_bytes: u64,
    /// Logical sector size of the source.
    pub sector_size: u32,
    /// Scan mode and engine flags.
    pub configuration: ScanConfiguration,
    /// Overall session status.
    pub status: SessionStatus,
    /// Byte offset where the scan last left off (for resume).
    pub last_offset: u64,
    /// Number of bytes processed so far.
    pub processed_bytes: u64,
    /// Number of files found so far.
    pub files_found: u64,
    /// Number of bad sectors encountered.
    pub bad_sectors: u64,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,
    /// When the scan started (None if not yet started).
    pub started_at: Option<DateTime<Utc>>,
    /// When the scan completed/failed (None if in progress).
    pub completed_at: Option<DateTime<Utc>>,
    /// Last error message, if the session failed.
    pub last_error: Option<String>,
    /// Opaque JSON blob used by individual engines to persist their state.
    pub engine_state: Option<serde_json::Value>,
}

impl ScanSession {
    /// Create a new scan session.
    pub fn new(source_identity: SourceIdentity, configuration: ScanConfiguration) -> Self {
        let now = Utc::now();
        ScanSession {
            id: SessionId::new(),
            source_size_bytes: source_identity.size_bytes,
            sector_size: source_identity.sector_size,
            source_identity,
            configuration,
            status: SessionStatus::Created,
            last_offset: 0,
            processed_bytes: 0,
            files_found: 0,
            bad_sectors: 0,
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            last_error: None,
            engine_state: None,
        }
    }

    /// Mark the session as running.
    pub fn mark_running(&mut self) {
        let now = Utc::now();
        self.status = SessionStatus::Running;
        self.started_at.get_or_insert(now);
        self.updated_at = now;
    }

    /// Mark the session as paused at `offset`.
    pub fn mark_paused(&mut self, offset: u64) {
        self.status = SessionStatus::Paused;
        self.last_offset = offset;
        self.updated_at = Utc::now();
    }

    /// Mark the session as completed.
    pub fn mark_completed(&mut self) {
        let now = Utc::now();
        self.status = SessionStatus::Completed;
        self.completed_at = Some(now);
        self.updated_at = now;
    }

    /// Mark the session as failed.
    pub fn mark_failed(&mut self, error: impl Into<String>) {
        let now = Utc::now();
        self.status = SessionStatus::Failed;
        self.last_error = Some(error.into());
        self.completed_at = Some(now);
        self.updated_at = now;
    }

    /// Mark the session as cancelled.
    pub fn mark_cancelled(&mut self) {
        let now = Utc::now();
        self.status = SessionStatus::Cancelled;
        self.completed_at = Some(now);
        self.updated_at = now;
    }

    /// Update progress counters.
    pub fn update_progress(
        &mut self,
        processed_bytes: u64,
        files_found: u64,
        bad_sectors: u64,
        last_offset: u64,
    ) {
        self.processed_bytes = processed_bytes;
        self.files_found = files_found;
        self.bad_sectors = bad_sectors;
        self.last_offset = last_offset;
        self.updated_at = Utc::now();
    }

    /// Calculate scan progress as a percentage (0.0 – 100.0).
    pub fn progress_percent(&self) -> f32 {
        if self.source_size_bytes == 0 {
            return 0.0;
        }
        (self.processed_bytes as f32 / self.source_size_bytes as f32 * 100.0).min(100.0)
    }

    /// Whether this session can be resumed.
    pub fn is_resumable(&self) -> bool {
        matches!(self.status, SessionStatus::Paused | SessionStatus::Failed) && self.last_offset > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use recoverx_core::{identity::SourceIdentity, types::ScanConfiguration};

    fn make_identity() -> SourceIdentity {
        SourceIdentity {
            label: "test".to_string(),
            path: "/dev/test".to_string(),
            size_bytes: 1_000_000,
            sector_size: 512,
            model: None,
            serial: None,
            filesystem_label: None,
            filesystem_type: None,
            partial_hash: None,
        }
    }

    #[test]
    fn new_session_has_created_status() {
        let session = ScanSession::new(make_identity(), ScanConfiguration::quick());
        assert_eq!(session.status, SessionStatus::Created);
        assert_eq!(session.last_offset, 0);
        assert_eq!(session.files_found, 0);
    }

    #[test]
    fn progress_percent_zero_when_no_bytes_processed() {
        let session = ScanSession::new(make_identity(), ScanConfiguration::quick());
        assert_eq!(session.progress_percent(), 0.0);
    }

    #[test]
    fn progress_percent_correct() {
        let mut session = ScanSession::new(make_identity(), ScanConfiguration::quick());
        session.update_progress(500_000, 10, 0, 500_000);
        assert!((session.progress_percent() - 50.0).abs() < 0.01);
    }

    #[test]
    fn mark_paused_sets_last_offset() {
        let mut session = ScanSession::new(make_identity(), ScanConfiguration::quick());
        session.mark_running();
        session.mark_paused(12345);
        assert_eq!(session.status, SessionStatus::Paused);
        assert_eq!(session.last_offset, 12345);
        assert!(session.is_resumable());
    }

    #[test]
    fn mark_failed_stores_error() {
        let mut session = ScanSession::new(make_identity(), ScanConfiguration::quick());
        session.mark_running();
        session.mark_failed("disk read error");
        assert_eq!(session.status, SessionStatus::Failed);
        assert_eq!(session.last_error.as_deref(), Some("disk read error"));
    }
}
