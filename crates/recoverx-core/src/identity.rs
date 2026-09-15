//! Source identity — used to prevent resuming a scan against a different device.

use serde::{Deserialize, Serialize};

/// Uniquely identifies a storage source.
///
/// For physical devices this is derived from model, serial number, and size.
/// For disk images it is the file path + file size + optional hash.
///
/// Before resuming a paused scan, the engine compares the stored identity
/// against the current source.  If they do not match, the scan is aborted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceIdentity {
    /// Human-readable label (e.g., "Samsung SSD 860 EVO" or "backup.img")
    pub label: String,
    /// Device path or image path
    pub path: String,
    /// Total source size in bytes
    pub size_bytes: u64,
    /// Logical sector size in bytes (e.g. 512 or 4096)
    pub sector_size: u32,
    /// Device model string, if available
    pub model: Option<String>,
    /// Device serial number, if available
    pub serial: Option<String>,
    /// Filesystem label found at the top level, if available
    pub filesystem_label: Option<String>,
    /// Detected filesystem type, if available
    pub filesystem_type: Option<String>,
    /// SHA-256 of the first 64 KiB for image files (optional, expensive for devices)
    pub partial_hash: Option<String>,
}

impl SourceIdentity {
    /// Compare two identities to decide whether they represent the same source.
    ///
    /// The comparison is intentionally strict: if the size or serial/path
    /// changed, we refuse to resume.
    pub fn is_same_source(&self, other: &SourceIdentity) -> bool {
        // Size must always match
        if self.size_bytes != other.size_bytes {
            return false;
        }

        // If both have serial numbers, compare them
        if let (Some(a), Some(b)) = (&self.serial, &other.serial) {
            return a == b && self.size_bytes == other.size_bytes;
        }

        // Fall back to path + size
        self.path == other.path && self.size_bytes == other.size_bytes
    }

    /// Build a summary string for display / logging.
    pub fn summary(&self) -> String {
        let model = self.model.as_deref().unwrap_or("Unknown");
        let serial = self.serial.as_deref().unwrap_or("N/A");
        format!(
            "{} | {} | path={} | size={} bytes | sector_size={}",
            model, serial, self.path, self.size_bytes, self.sector_size
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_identity(path: &str, size: u64, serial: Option<&str>) -> SourceIdentity {
        SourceIdentity {
            label: path.to_string(),
            path: path.to_string(),
            size_bytes: size,
            sector_size: 512,
            model: None,
            serial: serial.map(|s| s.to_string()),
            filesystem_label: None,
            filesystem_type: None,
            partial_hash: None,
        }
    }

    #[test]
    fn same_path_and_size_matches() {
        let a = make_identity("/dev/disk2", 1_000_000, None);
        let b = make_identity("/dev/disk2", 1_000_000, None);
        assert!(a.is_same_source(&b));
    }

    #[test]
    fn different_size_does_not_match() {
        let a = make_identity("/dev/disk2", 1_000_000, None);
        let b = make_identity("/dev/disk2", 2_000_000, None);
        assert!(!a.is_same_source(&b));
    }

    #[test]
    fn same_serial_matches_regardless_of_path() {
        let a = make_identity("/dev/disk2", 1_000_000, Some("SN1234"));
        let b = make_identity("/dev/disk3", 1_000_000, Some("SN1234"));
        assert!(a.is_same_source(&b));
    }

    #[test]
    fn different_serial_does_not_match() {
        let a = make_identity("/dev/disk2", 1_000_000, Some("SN1234"));
        let b = make_identity("/dev/disk2", 1_000_000, Some("SN9999"));
        assert!(!a.is_same_source(&b));
    }
}
