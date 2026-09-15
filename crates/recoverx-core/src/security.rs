//! Centralised write-protection and path-safety layer.
//!
//! Every write operation in RecoverX MUST pass through `SourceWriteGuard`.
//! This module also validates recovery destinations and sanitises filenames.

use std::path::{Component, Path, PathBuf};

use crate::error::{RecoverXError, Result};

/// The single gate that prevents writing to a registered source path.
///
/// Register every source that must never be modified.  All recovery output
/// paths are checked against this registry before any file is written.
#[derive(Debug, Default)]
pub struct SourceWriteGuard {
    protected_paths: Vec<PathBuf>,
}

impl SourceWriteGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a path (device or image) as read-only/protected.
    pub fn register_source(&mut self, path: impl AsRef<Path>) {
        let canonical = canonicalize_best_effort(path.as_ref());
        if !self.protected_paths.contains(&canonical) {
            self.protected_paths.push(canonical);
        }
    }

    /// Assert that `destination` is safe to write to.
    ///
    /// Fails if:
    ///   - destination is (or is inside) a protected source
    ///   - destination equals the source exactly
    pub fn assert_safe_destination(&self, destination: impl AsRef<Path>) -> Result<()> {
        let dest = canonicalize_best_effort(destination.as_ref());

        for protected in &self.protected_paths {
            if dest == *protected || dest.starts_with(protected) {
                return Err(RecoverXError::DestinationIsSource {
                    path: dest.display().to_string(),
                });
            }
        }
        Ok(())
    }

    /// Explicitly reject any attempt to write to a registered source path.
    pub fn reject_write_to_source(&self, path: impl AsRef<Path>) -> Result<()> {
        let p = canonicalize_best_effort(path.as_ref());
        for protected in &self.protected_paths {
            if p == *protected || p.starts_with(protected) {
                return Err(RecoverXError::WriteToSourceForbidden {
                    path: p.display().to_string(),
                });
            }
        }
        Ok(())
    }
}

/// Sanitise a filename extracted from a disk image so it is safe to use as a
/// filesystem path component on the host OS.
///
/// Rejects:
/// - Empty names
/// - Names containing path separators (`/`, `\`)
/// - Names containing null bytes
/// - Names starting with `.` followed by nothing (`.` and `..`)
/// - Names that exceed 255 bytes
///
/// Returns the sanitised name or an error.
pub fn sanitise_filename(name: &str) -> Result<String> {
    if name.is_empty() {
        return Err(RecoverXError::UnsafeFilename {
            filename: "(empty)".to_string(),
        });
    }

    if name.len() > 255 {
        return Err(RecoverXError::UnsafeFilename {
            filename: name[..32].to_string() + "…",
        });
    }

    // Reject directory traversal via separators or null bytes
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err(RecoverXError::UnsafeFilename {
            filename: name.to_string(),
        });
    }

    // Reject bare dot names
    if name == "." || name == ".." {
        return Err(RecoverXError::UnsafeFilename {
            filename: name.to_string(),
        });
    }

    // Replace characters unsafe on Windows NTFS even when running on *nix,
    // so that recovered files can be transferred across platforms.
    let sanitised: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            c if (c as u32) < 32 => '_', // ASCII control characters
            c => c,
        })
        .collect();

    Ok(sanitised)
}

/// Validate that a destination path does not escape a base directory
/// (prevents path-traversal attacks in recovery target paths).
pub fn assert_no_path_traversal(base: impl AsRef<Path>, target: impl AsRef<Path>) -> Result<()> {
    let base = base.as_ref();
    let target = target.as_ref();

    // Strip any `..` components by walking the path
    let mut components: Vec<Component> = Vec::new();
    for component in target.components() {
        match component {
            Component::ParentDir => {
                // Pop or reject if we'd escape the base
                if components.is_empty() {
                    return Err(RecoverXError::PathTraversalDetected {
                        path: target.display().to_string(),
                    });
                }
                components.pop();
            }
            Component::CurDir => {} // skip `.`
            c => components.push(c),
        }
    }

    let resolved: PathBuf = components.iter().collect();

    // The resolved path must still be under `base`
    if !resolved.starts_with(base) && !resolved.is_relative() {
        return Err(RecoverXError::PathTraversalDetected {
            path: target.display().to_string(),
        });
    }

    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Try to canonicalize a path; if it doesn't exist yet, return it as-is.
fn canonicalize_best_effort(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn sanitise_normal_filename() {
        assert_eq!(sanitise_filename("report.pdf").unwrap(), "report.pdf");
    }

    #[test]
    fn sanitise_rejects_slash() {
        assert!(sanitise_filename("../etc/passwd").is_err());
    }

    #[test]
    fn sanitise_rejects_empty() {
        assert!(sanitise_filename("").is_err());
    }

    #[test]
    fn sanitise_rejects_dot_dot() {
        assert!(sanitise_filename("..").is_err());
    }

    #[test]
    fn sanitise_rejects_null_byte() {
        assert!(sanitise_filename("file\0name").is_err());
    }

    #[test]
    fn sanitise_replaces_colon() {
        let name = sanitise_filename("file:name.txt").unwrap();
        assert_eq!(name, "file_name.txt");
    }

    #[test]
    fn write_guard_rejects_source_path() {
        let mut guard = SourceWriteGuard::new();
        guard.register_source("/tmp/test_source.img");
        let result = guard.reject_write_to_source("/tmp/test_source.img");
        assert!(result.is_err());
    }

    #[test]
    fn write_guard_allows_unregistered_path() {
        let mut guard = SourceWriteGuard::new();
        guard.register_source("/tmp/test_source.img");
        let result = guard.reject_write_to_source("/tmp/recovery_output");
        assert!(result.is_ok());
    }

    #[test]
    fn destination_cannot_be_source() {
        let mut guard = SourceWriteGuard::new();
        guard.register_source("/tmp/source.img");
        assert!(guard.assert_safe_destination("/tmp/source.img").is_err());
    }

    #[test]
    fn path_traversal_detected() {
        let base = PathBuf::from("/tmp/recovery");
        let bad = PathBuf::from("/tmp/recovery/../../etc/passwd");
        // The function should detect traversal
        let result = assert_no_path_traversal(&base, &bad);
        assert!(result.is_err());
    }
}
