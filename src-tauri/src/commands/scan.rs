use recoverx_core::{identity::SourceIdentity, types::SessionId};
use tauri::State;

use crate::state::AppState;

/// Start (or restart) a scan session.
#[tauri::command]
pub async fn start_scan(
    state: State<'_, AppState>,
    session_id: String,
    source_path: String,
    source_size_bytes: u64,
    sector_size: u32,
) -> Result<(), String> {
    let id = session_id.parse::<SessionId>().map_err(|e| e.to_string())?;

    let identity = SourceIdentity {
        label: source_path.clone(),
        path: source_path,
        size_bytes: source_size_bytes,
        sector_size,
        model: None,
        serial: None,
        filesystem_label: None,
        filesystem_type: None,
        partial_hash: None,
    };

    let orchestrator = state.orchestrator().map_err(|e| e.to_string())?;
    orchestrator
        .start_scan(&id, &identity)
        .await
        .map_err(|e| e.to_string())
}

/// Pause a running scan.
#[tauri::command]
pub async fn pause_scan(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let id = session_id.parse::<SessionId>().map_err(|e| e.to_string())?;

    let orchestrator = state.orchestrator().map_err(|e| e.to_string())?;
    orchestrator.pause_scan(&id).map_err(|e| e.to_string())
}

/// Resume a paused scan, verifying source identity.
#[tauri::command]
pub async fn resume_scan(
    state: State<'_, AppState>,
    session_id: String,
    source_path: String,
    source_size_bytes: u64,
    sector_size: u32,
) -> Result<(), String> {
    let id = session_id.parse::<SessionId>().map_err(|e| e.to_string())?;

    let identity = SourceIdentity {
        label: source_path.clone(),
        path: source_path,
        size_bytes: source_size_bytes,
        sector_size,
        model: None,
        serial: None,
        filesystem_label: None,
        filesystem_type: None,
        partial_hash: None,
    };

    let orchestrator = state.orchestrator().map_err(|e| e.to_string())?;
    orchestrator
        .resume_scan(&id, &identity)
        .map_err(|e| e.to_string())
}

/// Cancel a scan — marks cancelled and persists to DB.
#[tauri::command]
pub async fn cancel_scan(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let id = session_id.parse::<SessionId>().map_err(|e| e.to_string())?;

    let orchestrator = state.orchestrator().map_err(|e| e.to_string())?;

    // Try to pause first in case it is running (ignore error if not running)
    let _ = orchestrator.pause_scan(&id);

    // Load session, mark cancelled, persist
    orchestrator
        .cancel_session(&id)
        .map_err(|e| e.to_string())
}
