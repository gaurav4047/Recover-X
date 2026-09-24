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

/// Paginated + filtered query — used by the Results screen for fast loading.
#[tauri::command]
pub async fn query_recovered_files(
    state: State<'_, AppState>,
    session_id: String,
    category: Option<String>,  // "Images", "Videos", "Documents", etc. or None for all
    search: Option<String>,    // filename substring filter
    limit: i64,
    offset: i64,
) -> Result<Vec<RecoveredFile>, String> {
    let orchestrator = state.orchestrator().map_err(|e| e.to_string())?;
    orchestrator
        .query_recovered_files(
            &session_id,
            category.as_deref(),
            search.as_deref(),
            limit,
            offset,
        )
        .map_err(|e| e.to_string())
}

/// Get file counts per category for the tab bar.
#[tauri::command]
pub async fn get_category_counts(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<(String, i64)>, String> {
    let orchestrator = state.orchestrator().map_err(|e| e.to_string())?;
    orchestrator
        .category_counts(&session_id)
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

    // Create destination directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(dest_path) {
        return Err(format!("Cannot create destination folder: {}", e));
    }

    // Open source only if we have disk-offset files that need raw reads.
    // Directory-scan and trash files are copied by path — no raw source needed.
    let needs_raw_source = selected.iter().any(|f| {
        !f.metadata.get("directory_scan").and_then(|v| v.as_bool()).unwrap_or(false)
            && !f.metadata.get("trash_recovery").and_then(|v| v.as_bool()).unwrap_or(false)
    });

    let mut raw_source: Option<Box<dyn recoverx_storage::source::StorageSource>> = if needs_raw_source {
        if request.source_path.starts_with("/dev/") || request.source_path.starts_with("\\\\.\\") {
            match PhysicalDeviceSource::open(&request.source_path) {
                Ok(s) => Some(Box::new(s)),
                Err(e) => return Err(format!("Cannot open source device '{}': {}", request.source_path, e)),
            }
        } else if std::path::Path::new(&request.source_path).is_file() {
            match DiskImageSource::open(&request.source_path) {
                Ok(s) => Some(Box::new(s)),
                Err(e) => return Err(format!("Cannot open disk image '{}': {}", request.source_path, e)),
            }
        } else {
            None
        }
    } else {
        None
    };

    // Partition files into three groups:
    // 1. directory_scan files — copy directly from full_path in metadata
    // 2. trash_recovery files — copy from trash_path in metadata
    // 3. disk files — read from raw storage source by offset
    let (fs_files, disk_files): (Vec<_>, Vec<_>) = selected
        .into_iter()
        .partition(|f| {
            f.metadata.get("directory_scan").and_then(|v| v.as_bool()).unwrap_or(false)
                || f.metadata.get("trash_recovery").and_then(|v| v.as_bool()).unwrap_or(false)
        });

    let mut results = Vec::new();

    // Recover filesystem files (directory scan + trash) by direct file copy
    for file in &fs_files {
        // Try full_path first (directory_scan), fall back to trash_path
        let src_path = file.metadata.get("full_path")
            .or_else(|| file.metadata.get("trash_path"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if src_path.is_empty() || !std::path::Path::new(src_path).exists() {
            results.push(RecoveryResult {
                file_id: file.id,
                name: file.name.clone(),
                destination_path: request.destination_dir.clone(),
                status: recoverx_engines::RecoveryStatus::Failed,
                sha256: None,
                size_recovered: 0,
                error: Some(format!("Source file not found: {}", src_path)),
            });
            continue;
        }

        // Resolve collision-safe destination path
        let dest_file = {
            let candidate = dest_path.join(&file.name);
            if candidate.exists() {
                let stem = std::path::Path::new(&file.name)
                    .file_stem().unwrap_or_default().to_string_lossy().to_string();
                let ext = std::path::Path::new(&file.name)
                    .extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
                let mut i = 1u32;
                loop {
                    let new_name = format!("{}_{}{}", stem, i, ext);
                    let c = dest_path.join(&new_name);
                    if !c.exists() { break c; }
                    i += 1;
                }
            } else {
                candidate
            }
        };

        match copy_and_hash(std::path::Path::new(src_path), &dest_file) {
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
        match raw_source.as_mut() {
            Some(source) => {
                let mgr = RecoveryManager::new(&request.source_path);
                let mut disk_results = mgr.recover_files(source.as_mut(), &disk_files, dest_path);
                results.append(&mut disk_results);
            }
            None => {
                // No raw source available — mark these as failed
                for file in &disk_files {
                    results.push(RecoveryResult {
                        file_id: file.id,
                        name: file.name.clone(),
                        destination_path: request.destination_dir.clone(),
                        status: recoverx_engines::RecoveryStatus::Failed,
                        sha256: None,
                        size_recovered: 0,
                        error: Some("Cannot open source for raw read".to_string()),
                    });
                }
            }
        }
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

// ── Corrupted Data Recovery commands ──────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanCorruptionRequest {
    /// Directory or file paths to scan for corruption.
    pub paths: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RepairCorruptedRequest {
    /// Corruption reports to attempt repair on.
    pub reports: Vec<recoverx_engines::CorruptionReport>,
    /// Optional output directory. When non-empty the repaired file is written
    /// there instead of replacing the original in-place.
    pub output_dir: String,
}

/// Open a native folder-picker dialog and return the chosen path.
/// Returns `None` if the user cancels.
#[tauri::command]
pub async fn open_folder_dialog(
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let folder = app
        .dialog()
        .file()
        .set_title("Choose Output Folder for Repaired Files")
        .blocking_pick_folder();
    Ok(folder.map(|p| p.to_string()))
}

/// Open a native file-picker dialog and return the chosen path(s).
/// Returns an empty vec if the user cancels.
#[tauri::command]
pub async fn open_file_dialog(
    app: tauri::AppHandle,
) -> Result<Vec<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let files = app
        .dialog()
        .file()
        .set_title("Select Files to Securely Delete")
        .blocking_pick_files();
    Ok(files
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.to_string())
        .collect())
}

/// Open a native folder-picker for secure-delete targets.
/// Returns `None` if the user cancels.
#[tauri::command]
pub async fn open_target_folder_dialog(
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let folder = app
        .dialog()
        .file()
        .set_title("Select Folder to Securely Delete")
        .blocking_pick_folder();
    Ok(folder.map(|p| p.to_string()))
}

/// Scan a list of files / directories and return a corruption report for each.
#[tauri::command]
pub async fn scan_for_corruption(
    request: ScanCorruptionRequest,
) -> Result<Vec<recoverx_engines::CorruptionReport>, String> {
    use recoverx_engines::CorruptionRepairEngine;

    let engine = CorruptionRepairEngine::new();
    let mut reports = Vec::new();

    for raw_path in &request.paths {
        let path = std::path::Path::new(raw_path);

        if path.is_file() {
            reports.push(engine.scan(path));
        } else if path.is_dir() {
            // Walk directory up to 3 levels deep, skip hidden files
            collect_files(path, 0, 3, &mut |file_path| {
                reports.push(engine.scan(file_path));
            });
        } else {
            reports.push(recoverx_engines::CorruptionReport {
                path: raw_path.clone(),
                size_bytes: 0,
                format: String::new(),
                corruption: recoverx_engines::CorruptionKind::Unknown,
                description: format!("Path not found: {}", raw_path),
                repairable: false,
                repair_confidence: 0,
            });
        }
    }

    Ok(reports)
}

/// Attempt to repair files described by the provided corruption reports.
///
/// Behaviour depends on `output_dir`:
///   - Empty string  → in-place repair (original overwritten, .bak backup created beside it)
///   - Non-empty     → repaired copy written to output_dir/<filename>, original untouched
#[tauri::command]
pub async fn repair_corrupted_files(
    request: RepairCorruptedRequest,
) -> Result<Vec<recoverx_engines::RepairResult>, String> {
    use recoverx_engines::CorruptionRepairEngine;

    let output_to_dir = !request.output_dir.trim().is_empty();

    // When writing to a separate output dir, create it first
    if output_to_dir {
        let out = std::path::Path::new(&request.output_dir);
        std::fs::create_dir_all(out)
            .map_err(|e| format!("Cannot create output directory '{}': {}", request.output_dir, e))?;
    }

    let engine = CorruptionRepairEngine::new();

    let results = request.reports
        .iter()
        .map(|report| {
            if output_to_dir {
                // Derive a destination path inside the chosen output dir
                let src = std::path::Path::new(&report.path);
                let filename = src.file_name().unwrap_or_default();
                let dest_path = std::path::Path::new(&request.output_dir).join(filename);

                // If a file with the same name already exists, add a counter suffix
                let dest_path = if dest_path.exists() {
                    let stem = src.file_stem().unwrap_or_default().to_string_lossy();
                    let ext  = src.extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default();
                    let mut i = 1u32;
                    loop {
                        let candidate = std::path::Path::new(&request.output_dir)
                            .join(format!("{}_{}{}", stem, i, ext));
                        if !candidate.exists() { break candidate; }
                        i += 1;
                    }
                } else {
                    dest_path
                };

                // Copy the original to dest, then repair dest in-place
                match std::fs::copy(src, &dest_path) {
                    Err(e) => recoverx_engines::RepairResult {
                        original_path: report.path.clone(),
                        repaired_path: String::new(),
                        success: false,
                        action: "Copy to output dir".into(),
                        bytes_written: 0,
                        error: Some(format!("Cannot copy to output folder: {}", e)),
                    },
                    Ok(_) => {
                        // Build a synthetic report pointing at the copy so the
                        // engine repairs the copy without touching the original
                        let mut copy_report = report.clone();
                        copy_report.path = dest_path.display().to_string();
                        // output_dir arg is unused by the engine (in-place logic)
                        let mut result = engine.repair(&copy_report, std::path::Path::new(&request.output_dir));
                        // Always report back the original source path so the UI
                        // shows the right file name
                        result.original_path = report.path.clone();
                        result
                    }
                }
            } else {
                // In-place: engine backs up and overwrites the original
                engine.repair(report, std::path::Path::new(&request.output_dir))
            }
        })
        .collect();

    Ok(results)
}

/// Recursively collect files up to `max_depth` levels, calling `f` on each file.
fn collect_files(
    dir: &std::path::Path,
    depth: usize,
    max_depth: usize,
    f: &mut impl FnMut(&std::path::Path),
) {
    if depth > max_depth { return; }
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if name.starts_with('.') { continue; } // skip hidden
        if path.is_dir() {
            collect_files(&path, depth + 1, max_depth, f);
        } else if path.is_file() {
            f(&path);
        }
    }
}
