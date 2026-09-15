//! Trash / Recycle Bin scanner for live filesystem sources.
//!
//! This module ONLY scans OS Trash/Recycle Bin directories.
//! It never walks a live user folder and reports existing files as "deleted" —
//! that would be misleading. Files are only returned if they are genuinely
//! in a Trash/Recycle Bin location.
//!
//! For raw deleted file recovery from disk sectors, use the partition +
//! filesystem pipeline (Fat32Analyzer, NtfsAnalyzer, Ext4Analyzer, FileCarver).

use std::path::Path;
use std::time::UNIX_EPOCH;

use uuid::Uuid;

use crate::models::{FileFragment, FileStatus, RecoveredFile, RecoveryMethod};

pub struct DirectoryScanner;

/// Known Trash directory name patterns across platforms.
const TRASH_NAMES: &[&str] = &[
    ".Trash",
    ".Trashes",
    "Trash",
    ".local/share/Trash",
    "$Recycle.Bin",
    "RECYCLED",
    "RECYCLER",
];

impl DirectoryScanner {
    /// Scan a path for deleted files.
    ///
    /// IMPORTANT: Only returns files from recognised Trash/Recycle Bin
    /// directories. If `dir` is not a Trash location, returns an empty vec
    /// with a log message explaining that raw device scanning is required.
    pub fn scan(
        dir: &Path,
        session_id: &str,
        deleted_after: Option<i64>,
        deleted_before: Option<i64>,
    ) -> Vec<RecoveredFile> {
        let dir_str = dir.display().to_string();

        // Only proceed if this path is or contains a Trash directory
        let is_trash_path = Self::is_trash_directory(dir);

        if !is_trash_path {
            tracing::info!(
                path = %dir_str,
                "Directory is not a Trash location — skipping live filesystem walk. \
                 To recover deleted files from this location, select the underlying \
                 storage device (/dev/diskN) for raw analysis."
            );
            return vec![];
        }

        // It's a Trash directory — walk it for deleted files
        let mut results = Vec::new();
        Self::walk_trash(dir, dir, session_id, deleted_after, deleted_before, &mut results, 0);

        // On macOS, also check numeric UID subdirectories of .Trashes
        // (e.g. /Volumes/USB/.Trashes/501/)
        if dir_str.ends_with(".Trashes") {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let sub = entry.path();
                    if sub.is_dir() {
                        Self::walk_trash(&sub, &sub, session_id, deleted_after, deleted_before, &mut results, 0);
                    }
                }
            }
        }

        // macOS: also use mdfind to find recently deleted items in the Trash
        #[cfg(target_os = "macos")]
        Self::scan_trash_with_mdfind(dir, session_id, deleted_after, deleted_before, &mut results);

        results
    }

    /// Returns true if `dir` is a recognised Trash/Recycle Bin directory.
    pub fn is_trash_directory(dir: &Path) -> bool {
        let dir_str = dir.display().to_string();
        TRASH_NAMES.iter().any(|t| {
            dir_str.ends_with(t)
                || dir_str.contains(&format!("/{}/", t))
                || dir_str.contains(&format!("\\{}\\", t))
        })
    }

    fn walk_trash(
        root: &Path,
        dir: &Path,
        session_id: &str,
        deleted_after: Option<i64>,
        deleted_before: Option<i64>,
        results: &mut Vec<RecoveredFile>,
        depth: usize,
    ) {
        if depth > 10 { return; }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_symlink() { continue; }

            if path.is_dir() {
                Self::walk_trash(root, &path, session_id, deleted_after, deleted_before, results, depth + 1);
                continue;
            }
            if !path.is_file() { continue; }

            let meta = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let size = meta.len();
            if size == 0 { continue; }

            let name = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            // Skip metadata files
            if name.ends_with(".trashinfo") || name == ".DS_Store" || name == "desktop.ini" {
                continue;
            }

            // Get modification time
            let mtime: Option<i64> = meta.modified().ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);

            // Apply timeline filter
            if let Some(ts) = mtime {
                if let Some(after) = deleted_after { if ts < after { continue; } }
                if let Some(before) = deleted_before { if ts > before { continue; } }
            }

            let extension = path.extension()
                .map(|e| e.to_string_lossy().to_lowercase().to_string());

            // Try to read original path from .trashinfo companion file (Linux)
            let original_path = Self::read_trashinfo_original_path(&path)
                .or_else(|| Some(path.display().to_string()));

            let modified_at = mtime.and_then(|ts| {
                chrono::DateTime::from_timestamp(ts, 0)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            });

            results.push(RecoveredFile {
                id: Uuid::new_v4(),
                session_id: session_id.to_string(),
                name,
                original_path,
                size_bytes: size,
                source_offset: 0,
                partition_index: None,
                filesystem_type: Some("trash".to_string()),
                recovery_method: RecoveryMethod::FilesystemMetadata,
                confidence: 95, // High — file is physically present in Trash
                status: FileStatus::Complete,
                sha256: None,
                created_at: None,
                modified_at,
                accessed_at: None,
                extension,
                mime_type: None,
                is_deleted: true,
                is_fragmented: false,
                fragments: vec![FileFragment {
                    offset: 0,
                    length: size,
                    order: 0,
                }],
                metadata: serde_json::json!({
                    "directory_scan": true,
                    "full_path": path.display().to_string(),
                    "is_trash": true,
                }),
            });
        }
    }

    fn read_trashinfo_original_path(trash_file: &Path) -> Option<String> {
        let parent = trash_file.parent()?;
        let info_dir = parent.parent()?.join("info");
        let file_name = trash_file.file_name()?.to_string_lossy();
        let info_path = info_dir.join(format!("{}.trashinfo", file_name));
        let contents = std::fs::read_to_string(&info_path).ok()?;
        for line in contents.lines() {
            if let Some(rest) = line.strip_prefix("Path=") {
                return Some(rest.to_string());
            }
        }
        None
    }

    #[cfg(target_os = "macos")]
    fn scan_trash_with_mdfind(
        dir: &Path,
        session_id: &str,
        deleted_after: Option<i64>,
        deleted_before: Option<i64>,
        results: &mut Vec<RecoveredFile>,
    ) {
        use std::process::Command;

        let dir_str = dir.display().to_string();
        let date_query = if let Some(after) = deleted_after {
            chrono::DateTime::from_timestamp(after, 0)
                .map(|d| format!(
                    " && kMDItemFSContentChangeDate >= $time.iso(\"{}\") ",
                    d.format("%Y-%m-%dT%H:%M:%SZ")
                ))
                .unwrap_or_default()
        } else {
            " && kMDItemFSContentChangeDate >= $time.today(-365)".to_string()
        };

        let query = format!("kMDItemFSNodeType == 'File'{}", date_query);
        let output = Command::new("mdfind")
            .args(["-onlyin", &dir_str, &query])
            .output();

        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let path = std::path::Path::new(line.trim());
                // Only accept files that are genuinely inside a Trash directory
                if !Self::is_trash_directory(path.parent().unwrap_or(path)) {
                    continue;
                }
                if !path.is_file() { continue; }

                let meta = match std::fs::metadata(path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let size = meta.len();
                if size == 0 { continue; }

                // Skip if already found by the directory walk
                let path_str = path.display().to_string();
                if results.iter().any(|r| {
                    r.metadata.get("full_path").and_then(|v| v.as_str()) == Some(&path_str)
                }) {
                    continue;
                }

                let mtime: Option<i64> = meta.modified().ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64);

                if let Some(ts) = mtime {
                    if let Some(after) = deleted_after { if ts < after { continue; } }
                    if let Some(before) = deleted_before { if ts > before { continue; } }
                }

                let name = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                if name.ends_with(".trashinfo") || name == ".DS_Store" { continue; }

                let extension = path.extension()
                    .map(|e| e.to_string_lossy().to_lowercase().to_string());
                let modified_at = mtime.and_then(|ts| {
                    chrono::DateTime::from_timestamp(ts, 0)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                });

                results.push(RecoveredFile {
                    id: Uuid::new_v4(),
                    session_id: session_id.to_string(),
                    name,
                    original_path: Some(path_str.clone()),
                    size_bytes: size,
                    source_offset: 0,
                    partition_index: None,
                    filesystem_type: Some("trash".to_string()),
                    recovery_method: RecoveryMethod::FilesystemMetadata,
                    confidence: 95,
                    status: FileStatus::Complete,
                    sha256: None,
                    created_at: None,
                    modified_at,
                    accessed_at: None,
                    extension,
                    mime_type: None,
                    is_deleted: true,
                    is_fragmented: false,
                    fragments: vec![FileFragment { offset: 0, length: size, order: 0 }],
                    metadata: serde_json::json!({
                        "directory_scan": true,
                        "full_path": path_str,
                        "is_trash": true,
                        "source": "mdfind",
                    }),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn non_trash_dir_returns_empty() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("photo.jpg"), b"data").unwrap();
        let files = DirectoryScanner::scan(dir.path(), "test", None, None);
        assert!(files.is_empty(), "Non-trash directory should return no files");
    }

    #[test]
    fn trash_dir_returns_files() {
        // Create a directory named .Trash
        let base = TempDir::new().unwrap();
        let trash = base.path().join(".Trash");
        std::fs::create_dir(&trash).unwrap();
        std::fs::write(trash.join("deleted_photo.jpg"), b"jpeg data").unwrap();

        let files = DirectoryScanner::scan(&trash, "test", None, None);
        assert_eq!(files.len(), 1);
        assert!(files[0].is_deleted);
        assert_eq!(files[0].confidence, 95);
    }
}
