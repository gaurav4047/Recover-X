use recoverx_engines::{RecoveredFile, RecoveryManager, RecoveryResult};
use recoverx_storage::DiskImageSource;
use recoverx_storage::physical_device::PhysicalDeviceSource;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use tauri::State;

use crate::state::AppState;

/// List all recovered files for a completed scan session.
#[tauri::command]
pub async fn list_recovered_files(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<RecoveredFile>, String> {
    let orchestrator = state.orchestrator().map_err(|e| e.to_string())?;
    orchestrator
        .list_recovered_files(&session_id)
        .map_err(|e| e.to_string())
}

/// List partitions detected during a scan session.
#[tauri::command]
pub async fn list_partitions(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<recoverx_engines::DetectedPartition>, String> {
    let orchestrator = state.orchestrator().map_err(|e| e.to_string())?;
    orchestrator
        .list_partitions(&session_id)
        .map_err(|e| e.to_string())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RecoverFilesRequest {
    pub session_id: String,
    pub source_path: String,
    pub file_ids: Vec<String>, // UUIDs
    pub destination_dir: String,
}

/// Recover selected files from a scan session to a destination directory.
#[tauri::command]
pub async fn recover_files(
    state: State<'_, AppState>,
    request: RecoverFilesRequest,
) -> Result<Vec<RecoveryResult>, String> {
    let orchestrator = state.orchestrator().map_err(|e| e.to_string())?;

    // Load the recovered files for this session
    let all_files = orchestrator
        .list_recovered_files(&request.session_id)
        .map_err(|e| e.to_string())?;

    // Filter to requested file IDs
    let selected: Vec<RecoveredFile> = if request.file_ids.is_empty() {
        all_files
    } else {
        let ids: std::collections::HashSet<String> = request.file_ids.into_iter().collect();
        all_files
            .into_iter()
            .filter(|f| ids.contains(&f.id.to_string()))
            .collect()
    };

    if selected.is_empty() {
        return Ok(vec![]);
    }

    let dest_path = std::path::Path::new(&request.destination_dir);

    // Open source
    let mut source: Box<dyn recoverx_storage::source::StorageSource> =
        if request.source_path.starts_with("/dev/") || request.source_path.starts_with(r"\\.\") {
            match PhysicalDeviceSource::open(&request.source_path) {
                Ok(s) => Box::new(s),
                Err(e) => {
                    return Err(format!(
                        "Cannot open source device '{}': {}",
                        request.source_path, e
                    ))
                }
            }
        } else {
            match DiskImageSource::open(&request.source_path) {
                Ok(s) => Box::new(s),
                Err(e) => {
                    return Err(format!(
                        "Cannot open disk image '{}': {}",
                        request.source_path, e
                    ))
                }
            }
        };

    // For Trash-recovered files (source_offset == 0), recover from their trash path
    let (trash_files, disk_files): (Vec<_>, Vec<_>) = selected
        .into_iter()
        .partition(|f| {
            f.metadata.get("trash_recovery").and_then(|v| v.as_bool()).unwrap_or(false)
        });

    let mut results = Vec::new();

    // Recover Trash files by copying directly from filesystem path
    for file in &trash_files {
        let trash_path = file
            .metadata
            .get("trash_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if trash_path.is_empty() {
            results.push(RecoveryResult {
                file_id: file.id,
                name: file.name.clone(),
                destination_path: request.destination_dir.clone(),
                status: recoverx_engines::RecoveryStatus::Failed,
                sha256: None,
                size_recovered: 0,
                error: Some("Trash file path not recorded".to_string()),
            });
            continue;
        }

        let dest_file = dest_path.join(&file.name);
        match copy_and_hash(std::path::Path::new(trash_path), &dest_file) {
            Ok((size, sha256)) => results.push(RecoveryResult {
                file_id: file.id,
                name: file.name.clone(),
                destination_path: dest_file.display().to_string(),
                status: recoverx_engines::RecoveryStatus::Success,
                sha256: Some(sha256),
                size_recovered: size,
                error: None,
            }),
            Err(e) => results.push(RecoveryResult {
                file_id: file.id,
                name: file.name.clone(),
                destination_path: dest_file.display().to_string(),
                status: recoverx_engines::RecoveryStatus::Failed,
                sha256: None,
                size_recovered: 0,
                error: Some(e.to_string()),
            }),
        }
    }

    // Recover disk-carved/filesystem files via RecoveryManager
    if !disk_files.is_empty() {
        let mgr = RecoveryManager::new(&request.source_path);
        let mut disk_results = mgr.recover_files(source.as_mut(), &disk_files, dest_path);
        results.append(&mut disk_results);
    }

    Ok(results)
}

/// Compute SHA-256 of a recovered file to verify integrity.
#[tauri::command]
pub async fn verify_file_hash(destination_path: String) -> Result<String, String> {
    let mut file = std::fs::File::open(&destination_path)
        .map_err(|e| format!("Cannot open file for verification: {}", e))?;

    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 65536];

    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(e) => return Err(format!("Read error: {}", e)),
        }
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Copy a file from src to dest, returning (bytes_written, sha256).
fn copy_and_hash(src: &std::path::Path, dest: &std::path::Path) -> anyhow::Result<(u64, String)> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};

    let mut input = std::fs::File::open(src)?;
    let tmp = dest.with_extension(format!(
        "{}.tmp",
        dest.extension().unwrap_or_default().to_string_lossy()
    ));
    let mut output = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buf = vec![0u8; 65536];

    loop {
        let n = input.read(&mut buf)?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
        output.write_all(&buf[..n])?;
        total += n as u64;
    }

    output.flush()?;
    drop(output);
    std::fs::rename(&tmp, dest)?;

    Ok((total, hex::encode(hasher.finalize())))
}
