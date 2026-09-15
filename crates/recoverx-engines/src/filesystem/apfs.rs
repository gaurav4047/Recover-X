//! APFS filesystem analysis.
//!
//! APFS is Apple's proprietary filesystem. Full parsing requires Apple's
//! private on-disk format documentation. This module provides:
//! - Container/volume detection (already in FilesystemDetector)
//! - Basic metadata extraction where feasible
//! - Falls back to file carving for actual file recovery
//!
//! Note: FileVault-encrypted APFS volumes cannot be scanned without the
//! decryption key. The application will detect encryption and inform the user.

use recoverx_core::error::Result;
use recoverx_storage::source::StorageSource;

use crate::models::RecoveredFile;

pub struct ApfsAnalyzer;

impl ApfsAnalyzer {
    /// Attempt to enumerate files from an APFS container.
    /// Currently returns an empty list — file carving is used as fallback.
    pub fn analyze(
        _source: &mut dyn StorageSource,
        _volume_offset: u64,
        _session_id: &str,
        _max_files: usize,
    ) -> Result<Vec<RecoveredFile>> {
        // APFS internal B-tree structure parsing is not yet implemented.
        // File carving (FileCarver) will handle APFS volumes.
        tracing::info!("APFS: using file carving fallback (native APFS parser not yet implemented)");
        Ok(vec![])
    }
}
