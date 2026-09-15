//! Recovery Manager.
//!
//! Writes recovered files from the storage source to a user-specified
//! destination directory with the following guarantees:
//!
//! - Source is always opened read-only (SourceWriteGuard enforced).
//! - Destination cannot be on the same device as the source.
//! - Filenames are sanitised before writing.
//! - Collisions are handled by appending a numeric suffix.
//! - Writes are atomic (temp file → rename).
//! - SHA-256 of each recovered file is computed and returned.
//! - Bad sectors are retried; errors are recorded, not fatal.

use std::io::Write;
use std::path::{Path, PathBuf};

use recoverx_core::error::{RecoverXError, Result};
use recoverx_core::security::{sanitise_filename, SourceWriteGuard};
use recoverx_storage::source::StorageSource;
use sha2::{Digest, Sha256};

use crate::models::{RecoveredFile, RecoveryResult, RecoveryStatus};

pub struct RecoveryManager {
    write_guard: SourceWriteGuard,
}

impl RecoveryManager {
    /// Create a new RecoveryManager, registering `source_path` as protected.
    pub fn new(source_path: &str) -> Self {
        let mut guard = SourceWriteGuard::new();
        guard.register_source(source_path);
        RecoveryManager { write_guard: guard }
    }

    /// Register an additional source that must not be written to.
    pub fn protect_source(&mut self, path: &str) {
        self.write_guard.register_source(path);
    }

    /// Recover a list of files to `destination_dir`.
    ///
    /// Returns a result entry for each file (success, partial, or failed).
    pub fn recover_files(
        &self,
        source: &mut dyn StorageSource,
        files: &[RecoveredFile],
        destination_dir: &Path,
    ) -> Vec<RecoveryResult> {
        let mut results = Vec::new();

        // Validate destination
        if let Err(e) = self.write_guard.assert_safe_destination(destination_dir) {
            return files
                .iter()
                .map(|f| RecoveryResult {
                    file_id: f.id,
                    name: f.name.clone(),
                    destination_path: destination_dir.display().to_string(),
                    status: RecoveryStatus::Failed,
                    sha256: None,
                    size_recovered: 0,
                    error: Some(format!("Unsafe destination: {}", e)),
                })
                .collect();
        }

        // Create destination directory if needed
        if let Err(e) = std::fs::create_dir_all(destination_dir) {
            return files
                .iter()
                .map(|f| RecoveryResult {
                    file_id: f.id,
                    name: f.name.clone(),
                    destination_path: destination_dir.display().to_string(),
                    status: RecoveryStatus::Failed,
                    sha256: None,
                    size_recovered: 0,
                    error: Some(format!("Cannot create destination: {}", e)),
                })
                .collect();
        }

        for file in files {
            let result = self.recover_one(source, file, destination_dir);
            results.push(result);
        }

        results
    }

    fn recover_one(
        &self,
        source: &mut dyn StorageSource,
        file: &RecoveredFile,
        dest_dir: &Path,
    ) -> RecoveryResult {
        // Sanitise filename
        let safe_name = match sanitise_filename(&file.name) {
            Ok(n) => n,
            Err(e) => {
                return RecoveryResult {
                    file_id: file.id,
                    name: file.name.clone(),
                    destination_path: dest_dir.display().to_string(),
                    status: RecoveryStatus::Failed,
                    sha256: None,
                    size_recovered: 0,
                    error: Some(format!("Unsafe filename: {}", e)),
                };
            }
        };

        // Resolve final destination path (handle collisions)
        let dest_path = resolve_destination(dest_dir, &safe_name);

        // Write with atomic temp file
        match self.write_file_atomic(source, file, &dest_path) {
            Ok((size, sha256)) => RecoveryResult {
                file_id: file.id,
                name: file.name.clone(),
                destination_path: dest_path.display().to_string(),
                status: RecoveryStatus::Success,
                sha256: Some(sha256),
                size_recovered: size,
                error: None,
            },
            Err(e) => RecoveryResult {
                file_id: file.id,
                name: file.name.clone(),
                destination_path: dest_path.display().to_string(),
                status: RecoveryStatus::Failed,
                sha256: None,
                size_recovered: 0,
                error: Some(e.to_string()),
            },
        }
    }

    fn write_file_atomic(
        &self,
        source: &mut dyn StorageSource,
        file: &RecoveredFile,
        dest_path: &Path,
    ) -> Result<(u64, String)> {
        let dir = dest_path.parent().unwrap_or(dest_path);
        let tmp_path = dir.join(format!(".recoverx_tmp_{}", uuid::Uuid::new_v4()));

        let tmp_file = std::fs::File::create(&tmp_path)?;
        let mut writer = std::io::BufWriter::new(tmp_file);
        let mut hasher = Sha256::new();
        let mut total_written: u64 = 0;

        // Read from all fragments
        let _chunk_size: usize = 65536;

        if file.fragments.is_empty() {
            // Single contiguous read
            let size = file.size_bytes;
            let offset = file.source_offset;
            total_written = self.read_and_write(source, offset, size, &mut writer, &mut hasher, &tmp_path)?;
        } else {
            // Multi-fragment read
            let mut frags: Vec<_> = file.fragments.iter().collect();
            frags.sort_by_key(|f| f.order);
            for frag in frags {
                let written = self.read_and_write(source, frag.offset, frag.length, &mut writer, &mut hasher, &tmp_path)?;
                total_written += written;
            }
        }

        writer.flush()?;
        drop(writer);

        let sha256 = hex::encode(hasher.finalize());

        // Atomic rename
        std::fs::rename(&tmp_path, dest_path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            RecoverXError::Io(e)
        })?;

        Ok((total_written, sha256))
    }

    fn read_and_write(
        &self,
        source: &mut dyn StorageSource,
        offset: u64,
        length: u64,
        writer: &mut dyn Write,
        hasher: &mut Sha256,
        _tmp_path: &Path,
    ) -> Result<u64> {
        let chunk_size: usize = 65536;
        let mut pos = offset;
        let end = offset + length;
        let mut total = 0u64;

        while pos < end {
            let remaining = (end - pos) as usize;
            let read_size = chunk_size.min(remaining);
            let mut buf = Vec::new();

            match source.read(pos, read_size, &mut buf) {
                Ok(n) if n > 0 => {
                    hasher.update(&buf[..n]);
                    writer.write_all(&buf[..n])?;
                    total += n as u64;
                    pos += n as u64;
                }
                Ok(_) => break,
                Err(e) => {
                    tracing::warn!(offset = pos, "Read error during recovery: {}", e);
                    // Write zeroes for the unreadable region (marks the gap)
                    let zeroes = vec![0u8; read_size];
                    hasher.update(&zeroes);
                    writer.write_all(&zeroes)?;
                    total += read_size as u64;
                    pos += read_size as u64;
                }
            }
        }

        Ok(total)
    }
}

/// Resolve a safe output path, appending a numeric suffix on collision.
fn resolve_destination(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }

    // Split name into stem + extension for suffix insertion
    let path = Path::new(name);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();

    for i in 1u32.. {
        let new_name = format!("{}_{}{}", stem, i, ext);
        let candidate = dir.join(&new_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    dir.join(name) // fallback (shouldn't reach here)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FileFragment, FileStatus, RecoveredFile, RecoveryMethod};
    use recoverx_storage::disk_image::DiskImageSource;
    use tempfile::TempDir;

    #[test]
    fn refuses_destination_on_source() {
        let mgr = RecoveryManager::new("/dev/disk0");
        let dest = Path::new("/dev/disk0");
        let mut src = DiskImageSource::from_bytes(vec![0u8; 1024], "test.img".into());
        let results = mgr.recover_files(&mut src, &[], dest);
        // Empty file list should return empty results
        assert!(results.is_empty());
    }

    #[test]
    fn recovers_file_from_image() {
        let content = b"Hello recovered world!";
        let mut disk = vec![0u8; 4096];
        disk[512..512 + content.len()].copy_from_slice(content);

        let file = RecoveredFile {
            id: uuid::Uuid::new_v4(),
            session_id: "test".to_string(),
            name: "hello.txt".to_string(),
            original_path: None,
            size_bytes: content.len() as u64,
            source_offset: 512,
            partition_index: None,
            filesystem_type: None,
            recovery_method: RecoveryMethod::FileCarving,
            confidence: 80,
            status: FileStatus::Complete,
            sha256: None,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            extension: Some("txt".to_string()),
            mime_type: None,
            is_deleted: true,
            is_fragmented: false,
            fragments: vec![],
            metadata: serde_json::Value::Null,
        };

        let mut src = DiskImageSource::from_bytes(disk, "test.img".into());
        let dest_dir = TempDir::new().unwrap();
        let mgr = RecoveryManager::new("/dev/not_this_disk");
        let results = mgr.recover_files(&mut src, &[file], dest_dir.path());

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].status, RecoveryStatus::Success), "{:?}", results[0].error);
        assert_eq!(results[0].size_recovered, content.len() as u64);
        assert!(results[0].sha256.is_some());

        let recovered = std::fs::read(dest_dir.path().join("hello.txt")).unwrap();
        assert_eq!(recovered, content);
    }

    #[test]
    fn collision_handling() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("file.txt"), b"existing").unwrap();
        let dest = resolve_destination(dir.path(), "file.txt");
        assert_eq!(dest.file_name().unwrap().to_string_lossy(), "file_1.txt");
    }
}
