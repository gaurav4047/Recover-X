//! Directory scanner — walks a live filesystem directory to find files
//! that were recently deleted (from Trash metadata) or are currently present.
//!
//! Used when the scan source is a directory path rather than a raw device.
//! On macOS, also uses `mdfind` to query Spotlight for recently deleted files.

use std::path::Path;
use std::time::UNIX_EPOCH;

use uuid::Uuid;

use crate::models::{FileFragment, FileStatus, RecoveredFile, RecoveryMethod};

pub struct DirectoryScanner;

impl DirectoryScanner {
    /// Walk `dir` recursively and return all files as RecoveredFile records.
    /// On macOS also queries Spotlight (`mdfind`) for recently deleted files.
    pub fn scan(
        dir: &Path,
        session_id: &str,
        deleted_after: Option<i64>,
        deleted_before: Option<i64>,
    ) -> Vec<RecoveredFile> {
        let mut results = Vec::new();

        // 1. Walk what's currently in the directory
        Self::walk(dir, dir, session_id, deleted_after, deleted_before, &mut results, 0);

        // 2. On macOS: use mdfind to find recently deleted/modified files in this path
        #[cfg(target_os = "macos")]
        Self::scan_with_mdfind(dir, session_id, deleted_after, deleted_before, &mut results);

        // Deduplicate by path
        results.sort_by(|a, b| a.original_path.cmp(&b.original_path));
        results.dedup_by(|a, b| a.original_path == b.original_path);

        results
    }

    #[cfg(target_os = "macos")]
    fn scan_with_mdfind(
        dir: &Path,
        session_id: &str,
        deleted_after: Option<i64>,
        deleted_before: Option<i64>,
        results: &mut Vec<RecoveredFile>,
    ) {
        use std::process::Command;

        // Build mdfind query for files modified within the timeline
        let dir_str = dir.display().to_string();
        let is_trash = dir_str.contains(".Trash") || dir_str.contains(".Trashes");

        // Build date constraint for mdfind
        let date_query = if let Some(after) = deleted_after {
            // Convert Unix timestamp to date string mdfind understands
            let dt = chrono::DateTime::from_timestamp(after, 0)
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default();
            if dt.is_empty() {
                String::new()
            } else {
                format!(" && kMDItemFSContentChangeDate >= $time.iso(\"{dt}T00:00:00Z\")")
            }
        } else {
            // Default: last 90 days
            " && kMDItemFSContentChangeDate >= $time.today(-90)".to_string()
        };

        let query = format!("kMDItemFSNodeType == 'File'{}", date_query);

        let output = Command::new("mdfind")
            .args(["-onlyin", &dir_str, &query])
            .output();

        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let path = std::path::Path::new(line.trim());
                if !path.exists() { continue; }

                let meta = match std::fs::metadata(path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let size = meta.len();
                if size == 0 { continue; }

                let mtime: Option<i64> = meta.modified().ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64);

                // Apply timeline filter
                if let Some(ts) = mtime {
                    if let Some(after) = deleted_after {
                        if ts < after { continue; }
                    }
                    if let Some(before) = deleted_before {
                        if ts > before { continue; }
                    }
                }

                let name = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                let extension = path.extension()
                    .map(|e| e.to_string_lossy().to_lowercase().to_string());

                let modified_at = mtime.and_then(|ts| {
                    chrono::DateTime::from_timestamp(ts, 0)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                });

                results.push(RecoveredFile {
                    id: Uuid::new_v4(),
                    session_id: session_id.to_string(),
                    name: name.clone(),
                    original_path: Some(path.display().to_string()),
                    size_bytes: size,
                    source_offset: 0,
                    partition_index: None,
                    filesystem_type: Some("live_fs".to_string()),
                    recovery_method: RecoveryMethod::FilesystemMetadata,
                    confidence: if is_trash { 90 } else { 85 },
                    status: FileStatus::Complete,
                    sha256: None,
                    created_at: None,
                    modified_at,
                    accessed_at: None,
                    extension,
                    mime_type: None,
                    is_deleted: is_trash,
                    is_fragmented: false,
                    fragments: vec![FileFragment {
                        offset: 0,
                        length: size,
                        order: 0,
                    }],
                    metadata: serde_json::json!({
                        "directory_scan": true,
                        "full_path": path.display().to_string(),
                        "is_trash": is_trash,
                        "source": "mdfind",
                    }),
                });
            }
        }
    }

    fn walk(
        root: &Path,
        dir: &Path,
        session_id: &str,
        deleted_after: Option<i64>,
        deleted_before: Option<i64>,
        results: &mut Vec<RecoveredFile>,
        depth: usize,
    ) {
        if depth > 20 { return; } // guard against symlink loops

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();

            // Never follow symlinks
            if path.is_symlink() { continue; }

            if path.is_dir() {
                Self::walk(root, &path, session_id, deleted_after, deleted_before, results, depth + 1);
                continue;
            }

            if !path.is_file() { continue; }

            let meta = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let size = meta.len();
            if size == 0 { continue; }

            // Get modification time as Unix timestamp
            let mtime: Option<i64> = meta.modified().ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);

            // Apply timeline filter on mtime
            if let Some(ts) = mtime {
                if let Some(after) = deleted_after {
                    if ts < after { continue; }
                }
                if let Some(before) = deleted_before {
                    if ts > before { continue; }
                }
            }

            let name = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            // Skip metadata/system files
            if name.starts_with('.') && name.len() < 3 { continue; }
            if name == "desktop.ini" || name == "Thumbs.db" { continue; }

            let extension = path.extension()
                .map(|e| e.to_string_lossy().to_lowercase().to_string());

            // Build original path relative to root
            let rel_path = path.strip_prefix(root)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| path.display().to_string());

            // Is this a Trash file? (.Trash or .Trashes)
            let is_trash = path.components().any(|c| {
                let s = c.as_os_str().to_string_lossy();
                s == ".Trash" || s == ".Trashes" || s == "Trash" || s == "files"
            });

            let modified_at = mtime.map(|ts| {
                chrono::DateTime::from_timestamp(ts, 0)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            }).flatten();

            results.push(RecoveredFile {
                id: Uuid::new_v4(),
                session_id: session_id.to_string(),
                name: name.clone(),
                original_path: Some(path.display().to_string()),
                size_bytes: size,
                source_offset: 0,
                partition_index: None,
                filesystem_type: Some("live_fs".to_string()),
                recovery_method: if is_trash {
                    RecoveryMethod::FilesystemMetadata
                } else {
                    RecoveryMethod::FilesystemMetadata
                },
                confidence: if is_trash { 90 } else { 80 },
                status: FileStatus::Complete,
                sha256: None,
                created_at: None,
                modified_at,
                accessed_at: None,
                extension,
                mime_type: None,
                is_deleted: is_trash,
                is_fragmented: false,
                fragments: vec![FileFragment {
                    offset: 0,
                    length: size,
                    order: 0,
                }],
                metadata: serde_json::json!({
                    "directory_scan": true,
                    "full_path": path.display().to_string(),
                    "is_trash": is_trash,
                }),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn scans_directory_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("photo.jpg"), b"fake jpeg").unwrap();
        std::fs::write(dir.path().join("doc.pdf"), b"fake pdf").unwrap();

        let files = DirectoryScanner::scan(dir.path(), "test", None, None);
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.confidence == 80));
    }

    #[test]
    fn respects_timeline_filter() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("recent.txt"), b"new file").unwrap();

        // Filter: only files from the future (should return nothing)
        let far_future = chrono::Utc::now().timestamp() + 86400 * 365;
        let files = DirectoryScanner::scan(dir.path(), "test", Some(far_future), None);
        assert!(files.is_empty(), "Future filter should exclude all files");
    }
}
