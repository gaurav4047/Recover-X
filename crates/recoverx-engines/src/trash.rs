//! Trash and Recycle Bin scanner.
//!
//! Scans filesystem-accessible Trash/Recycle Bin directories on mounted volumes.
//! Returns RecoveredFile records for each item found.
//!
//! macOS: ~/.Trash, /Volumes/X/.Trashes/501
//! Linux: ~/.local/share/Trash/files
//! Windows: C:\$Recycle.Bin\<SID>\

use std::path::Path;

use uuid::Uuid;

use crate::models::{FileFragment, FileStatus, RecoveredFile, RecoveryMethod};

pub struct TrashScanner;

impl TrashScanner {
    /// Scan a Trash directory (any platform) and return recoverable files.
    pub fn scan_path(trash_dir: &Path, session_id: &str) -> Vec<RecoveredFile> {
        let mut results = Vec::new();

        // Scan common sub-directories
        let candidates = [
            trash_dir.to_path_buf(),
            trash_dir.join("files"),    // Linux Trash/files
            trash_dir.join("Files"),
        ];

        for dir in &candidates {
            if dir.is_dir() {
                Self::scan_dir(dir, session_id, &mut results);
            }
        }

        // macOS: scan numeric UID subdirectories of .Trashes
        if trash_dir.ends_with(".Trashes") {
            if let Ok(entries) = std::fs::read_dir(trash_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        Self::scan_dir(&path, session_id, &mut results);
                    }
                }
            }
        }

        results
    }

    fn scan_dir(dir: &Path, session_id: &str, results: &mut Vec<RecoveredFile>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_symlink() {
                // Never follow symlinks from recovered data
                continue;
            }

            if path.is_file() {
                if let Ok(meta) = std::fs::metadata(&path) {
                    let name = path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string());

                    // Skip metadata files (.DS_Store, desktop.ini, .trashinfo)
                    if name.starts_with('.') || name.ends_with(".trashinfo") {
                        continue;
                    }

                    let extension = path.extension()
                        .map(|e| e.to_string_lossy().to_lowercase())
                        .map(|s| s.to_string());

                    let original_path = Self::find_original_path(&path);

                    results.push(RecoveredFile {
                        id: Uuid::new_v4(),
                        session_id: session_id.to_string(),
                        name,
                        original_path,
                        size_bytes: meta.len(),
                        source_offset: 0,
                        partition_index: None,
                        filesystem_type: None,
                        recovery_method: RecoveryMethod::FilesystemMetadata,
                        confidence: 90, // High: file is physically present in Trash
                        status: FileStatus::Complete,
                        sha256: None,
                        created_at: None,
                        modified_at: meta.modified().ok().map(|t| {
                            chrono::DateTime::from(t)
                        }),
                        accessed_at: None,
                        extension: extension.clone(),
                        mime_type: None,
                        is_deleted: true,
                        is_fragmented: false,
                        fragments: vec![FileFragment {
                            offset: 0,
                            length: meta.len(),
                            order: 0,
                        }],
                        metadata: serde_json::json!({
                            "trash_path": path.to_string_lossy().as_ref(),
                            "trash_recovery": true,
                        }),
                    });
                }
            } else if path.is_dir() && !path.ends_with(".Trash") {
                // Recurse one level (avoid deep recursion)
                Self::scan_dir(&path, session_id, results);
            }
        }
    }

    /// Try to find the original path from a .trashinfo file (Linux) or
    /// macOS .DS_Store metadata, if present.
    fn find_original_path(trash_file: &Path) -> Option<String> {
        // Linux: companion .trashinfo file in Trash/info/filename.trashinfo
        if let Some(parent) = trash_file.parent() {
            let info_dir = parent.parent()?.join("info");
            let file_name = trash_file.file_name()?.to_string_lossy();
            let info_path = info_dir.join(format!("{}.trashinfo", file_name));
            if let Ok(contents) = std::fs::read_to_string(&info_path) {
                for line in contents.lines() {
                    if let Some(rest) = line.strip_prefix("Path=") {
                        return Some(rest.to_string());
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn scan_empty_trash_dir() {
        let dir = TempDir::new().unwrap();
        let files = TrashScanner::scan_path(dir.path(), "test-session");
        assert!(files.is_empty());
    }

    #[test]
    fn scan_trash_with_file() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("deleted_document.pdf"), b"fake pdf content").unwrap();

        let files = TrashScanner::scan_path(dir.path(), "test-session");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "deleted_document.pdf");
        assert_eq!(files[0].confidence, 90);
        assert!(files[0].is_deleted);
    }
}
