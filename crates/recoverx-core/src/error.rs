//! Unified error type for RecoverX.

use thiserror::Error;

/// The primary result type used throughout RecoverX.
pub type Result<T> = std::result::Result<T, RecoverXError>;

#[derive(Debug, Error)]
pub enum RecoverXError {
    // ── Storage errors ────────────────────────────────────────────────────
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Storage source not found: {path}")]
    SourceNotFound { path: String },

    #[error(
        "Read beyond source bounds: offset={offset} length={length} source_size={source_size}"
    )]
    ReadOutOfBounds {
        offset: u64,
        length: usize,
        source_size: u64,
    },

    #[error("Source is not readable: {reason}")]
    SourceNotReadable { reason: String },

    // ── Security errors ───────────────────────────────────────────────────
    #[error("Write to source is forbidden: {path}")]
    WriteToSourceForbidden { path: String },

    #[error("Recovery destination is the same as the source: {path}")]
    DestinationIsSource { path: String },

    #[error("Path traversal detected in: {path}")]
    PathTraversalDetected { path: String },

    #[error("Unsafe destination path: {path}")]
    UnsafeDestinationPath { path: String },

    #[error("Unsafe filename: {filename}")]
    UnsafeFilename { filename: String },

    // ── Privilege errors ──────────────────────────────────────────────────
    #[error("Insufficient privileges to access {resource}: {detail}")]
    InsufficientPrivileges { resource: String, detail: String },

    // ── Device errors ─────────────────────────────────────────────────────
    #[error("Device enumeration failed: {0}")]
    DeviceEnumerationFailed(String),

    #[error("Device not found: {path}")]
    DeviceNotFound { path: String },

    // ── Session errors ────────────────────────────────────────────────────
    #[error("Session not found: {id}")]
    SessionNotFound { id: String },

    #[error("Source identity mismatch — refusing to resume scan against a different device")]
    SourceIdentityMismatch,

    // ── Database errors ───────────────────────────────────────────────────
    #[error("Database error: {0}")]
    Database(String),

    #[error("Database migration failed: {0}")]
    DatabaseMigration(String),

    // ── Orchestrator errors ───────────────────────────────────────────────
    #[error("Task not found: {id}")]
    TaskNotFound { id: String },

    #[error("Scan already running for session: {session_id}")]
    ScanAlreadyRunning { session_id: String },

    #[error("Cannot cancel task in state {state}")]
    CannotCancelTask { state: String },

    // ── Image errors ──────────────────────────────────────────────────────
    #[error("Unsupported image format: {format}")]
    UnsupportedImageFormat { format: String },

    #[error("Image file not found: {path}")]
    ImageNotFound { path: String },

    // ── Serialization ─────────────────────────────────────────────────────
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    // ── Generic ───────────────────────────────────────────────────────────
    #[error("{0}")]
    Other(String),
}

impl From<anyhow::Error> for RecoverXError {
    fn from(e: anyhow::Error) -> Self {
        RecoverXError::Other(e.to_string())
    }
}
