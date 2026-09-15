use recoverx_core::{
    identity::SourceIdentity,
    types::{ScanConfiguration, ScanMode, SessionId},
};
use recoverx_orchestrator::ScanSession;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub source_path: String,
    pub source_size_bytes: u64,
    pub sector_size: u32,
    pub scan_mode: String,
    pub enable_filesystem_analysis: bool,
    pub enable_deleted_file_recovery: bool,
    pub enable_file_carving: bool,
    pub enable_duplicate_detection: bool,
    pub enable_forensic_hashing: bool,
    /// Unix timestamp seconds — only recover files deleted after this time.
    pub deleted_after: Option<i64>,
    /// Unix timestamp seconds — only recover files deleted before this time.
    pub deleted_before: Option<i64>,
}

/// Create a new scan session.
#[tauri::command]
pub async fn create_session(
    state: State<'_, AppState>,
    request: CreateSessionRequest,
) -> Result<ScanSession, String> {
    let mode = match request.scan_mode.to_lowercase().as_str() {
        "quick" => ScanMode::Quick,
        "deep" => ScanMode::Deep,
        "file_carving" | "filecarving" => ScanMode::FileCarving,
        _ => ScanMode::Quick,
    };

    let configuration = ScanConfiguration {
        mode,
        enable_filesystem_analysis: request.enable_filesystem_analysis,
        enable_deleted_file_recovery: request.enable_deleted_file_recovery,
        enable_file_carving: request.enable_file_carving,
        enable_duplicate_detection: request.enable_duplicate_detection,
        enable_forensic_hashing: request.enable_forensic_hashing,
        file_categories: vec![],
        deleted_after: request.deleted_after,
        deleted_before: request.deleted_before,
    };

    let identity = SourceIdentity {
        label: request.source_path.clone(),
        path: request.source_path,
        size_bytes: request.source_size_bytes,
        sector_size: request.sector_size,
        model: None,
        serial: None,
        filesystem_label: None,
        filesystem_type: None,
        partial_hash: None,
    };

    let orchestrator = state.orchestrator().map_err(|e| e.to_string())?;
    orchestrator
        .create_session(identity, configuration)
        .map_err(|e| e.to_string())
}

/// List all scan sessions.
#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> Result<Vec<ScanSession>, String> {
    let orchestrator = state.orchestrator().map_err(|e| e.to_string())?;
    orchestrator.list_sessions().map_err(|e| e.to_string())
}

/// Load a single scan session by ID.
#[tauri::command]
pub async fn load_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<ScanSession, String> {
    let id = session_id.parse::<SessionId>().map_err(|e| e.to_string())?;

    let orchestrator = state.orchestrator().map_err(|e| e.to_string())?;
    orchestrator.load_session(&id).map_err(|e| e.to_string())
}

/// Delete a scan session.
#[tauri::command]
pub async fn delete_session(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let id = session_id.parse::<SessionId>().map_err(|e| e.to_string())?;

    let orchestrator = state.orchestrator().map_err(|e| e.to_string())?;
    orchestrator.delete_session(&id).map_err(|e| e.to_string())
}
