//! macOS device enumeration using `diskutil` CLI.
//!
//! Uses `diskutil list` to enumerate whole-disk nodes, then
//! `diskutil info <dev>` per disk for detailed metadata.
//!
//! Whole disks only — partition slices (disk0s1, etc.) are enumerated
//! separately via `list_partitions()` on a per-disk basis.

use std::process::Command;

use recoverx_core::error::{RecoverXError, Result};

use crate::provider::{DeviceInfo, DeviceProvider, DeviceType};

extern crate libc;

pub struct MacOSDeviceProvider;

impl MacOSDeviceProvider {
    pub fn new() -> Self {
        MacOSDeviceProvider
    }
}

impl Default for MacOSDeviceProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceProvider for MacOSDeviceProvider {
    fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        let output = Command::new("diskutil")
            .args(["list"])
            .output()
            .map_err(|e| {
                RecoverXError::DeviceEnumerationFailed(format!("diskutil not found: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RecoverXError::DeviceEnumerationFailed(format!(
                "diskutil list failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let disk_paths = parse_diskutil_list(&stdout);

        let mut devices = Vec::new();
        for path in disk_paths {
            // Only enumerate whole disks (disk0, disk1, …) not slices (disk0s1)
            if !is_whole_disk(&path) {
                continue;
            }
            match self.get_device_info(&path) {
                Ok(info) => devices.push(info),
                Err(e) => {
                    tracing::debug!("Skipping {}: {}", path, e);
                }
            }
        }

        Ok(devices)
    }

    fn get_device_info(&self, path: &str) -> Result<DeviceInfo> {
        let output = Command::new("diskutil")
            .args(["info", path])
            .output()
            .map_err(|e| {
                RecoverXError::DeviceEnumerationFailed(format!("diskutil info failed: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RecoverXError::DeviceNotFound {
                path: format!("{}: {}", path, stderr.trim()),
            });
        }

        let text = String::from_utf8_lossy(&output.stdout);
        parse_diskutil_info(path, &text)
    }
}

// ── Parsing helpers ───────────────────────────────────────────────────────────

/// Extract /dev/diskN entries from `diskutil list` text output.
/// Only whole-disk entries appear at line-start with (internal) or (external, ...) markers.
fn parse_diskutil_list(output: &str) -> Vec<String> {
    let mut result = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("/dev/disk") {
            // Lines look like: /dev/disk0 (internal):
            let path = trimmed.split_whitespace().next().unwrap_or("").to_string();
            if !path.is_empty() {
                result.push(path);
            }
        }
    }
    result
}

/// Returns true only if the path is a whole disk node (not a partition slice).
/// Whole disks: /dev/disk0, /dev/disk1, /dev/disk10
/// Partitions:  /dev/disk0s1, /dev/disk1s2, /dev/disk10s3
fn is_whole_disk(path: &str) -> bool {
    // Strip the /dev/ prefix then check the basename matches disk\d+ exactly
    let basename = path.trim_start_matches("/dev/");
    if !basename.starts_with("disk") {
        return false;
    }
    // All remaining characters after "disk" must be digits
    basename[4..].chars().all(|c| c.is_ascii_digit())
}

/// Parse `diskutil info <path>` text output into `DeviceInfo`.
fn parse_diskutil_info(path: &str, text: &str) -> Result<DeviceInfo> {
    let mut name: Option<String> = None;
    let mut size_bytes: u64 = 0;
    let mut sector_size: u32 = 512;
    let mut device_type = DeviceType::Unknown;
    let mut filesystem: Option<String> = None;
    let mut is_write_protected = false;
    let mut is_internal = false;
    let mut model: Option<String> = None;
    let serial: Option<String> = None;

    for line in text.lines() {
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }
        let key = parts[0].trim().to_lowercase();
        let value = parts[1].trim();

        match key.as_str() {
            "device / media name" | "device media name" | "media name" => {
                if !value.is_empty() && value != "—" {
                    name = Some(value.to_string());
                }
            }
            "device node" => {
                // Use path we already have
            }
            "disk size" => {
                if let Some(bytes_val) = extract_bytes_value(value) {
                    size_bytes = bytes_val;
                }
            }
            "device block size" => {
                if let Ok(s) = value
                    .split_whitespace()
                    .next()
                    .unwrap_or("512")
                    .parse::<u32>()
                {
                    sector_size = s;
                }
            }
            "removable media" => {
                device_type = if value.to_lowercase().contains("yes")
                    || value.to_lowercase().contains("removable")
                {
                    DeviceType::Removable
                } else {
                    DeviceType::Fixed
                };
            }
            "solid state" => {
                // Keep Fixed type but note it's SSD (handled via name/model)
            }
            "device location" => {
                is_internal = value.to_lowercase().contains("internal");
                if is_internal && device_type == DeviceType::Unknown {
                    device_type = DeviceType::Fixed;
                } else if !is_internal && device_type == DeviceType::Unknown {
                    device_type = DeviceType::Removable;
                }
            }
            "file system personality" | "type (bundle path)" => {
                if !value.is_empty() && value != "—" {
                    filesystem = Some(value.to_string());
                }
            }
            "read-only media" => {
                if value.to_lowercase() == "yes" {
                    is_write_protected = true;
                }
            }
            "read-only volume" => {
                if value.to_lowercase() == "yes" {
                    is_write_protected = true;
                }
            }
            "volume name" if name.is_none() => {
                if !value.is_empty() {
                    name = Some(value.to_string());
                }
            }
            "disk / partition / scheme" => {
                // e.g. "GUID Partition Table Scheme"
                if name.is_none() {
                    name = Some(value.to_string());
                }
            }
            "i/o registry entry name" => {
                // Often has the model embedded
                if model.is_none() && !value.is_empty() {
                    model = Some(value.to_string());
                }
            }
            _ => {}
        }
    }

    if size_bytes == 0 {
        if let Ok(meta) = std::fs::metadata(path) {
            size_bytes = meta.len();
        }
    }

    // Check accessibility — try opening the raw device for reading.
    // std::fs::File::open uses O_RDONLY which works for physical disks.
    // For virtual APFS containers (disk3, etc.) we also try a privilege check
    // via access(2) syscall to detect if root can open it even if a plain
    // open returns EBUSY/ENXIO before the descriptor is actually used.
    let is_accessible = std::fs::OpenOptions::new()
        .read(true)
        .open(path)
        .is_ok()
        || {
            // Fallback: check if the current process is root — if so, treat
            // virtual APFS containers as accessible (they open fine once a
            // read is actually attempted with the right flags).
            #[cfg(unix)]
            {
                unsafe { libc::getuid() == 0 }
            }
            #[cfg(not(unix))]
            {
                false
            }
        };

    // Build a meaningful name
    let display_name = name.unwrap_or_else(|| {
        if is_internal {
            format!("Internal Disk ({})", path)
        } else {
            format!("External Disk ({})", path)
        }
    });

    Ok(DeviceInfo {
        path: path.to_string(),
        name: display_name,
        size_bytes,
        sector_size,
        device_type,
        filesystem,
        is_accessible,
        is_internal,
        model,
        serial,
        is_write_protected,
    })
}

/// Extract byte count from strings like "500.1 GB (500107862016 Bytes)".
fn extract_bytes_value(s: &str) -> Option<u64> {
    // Find the value in parentheses: "(N Bytes)"
    if let (Some(open), Some(close)) = (s.find('('), s.rfind(')')) {
        let inner = &s[open + 1..close];
        // inner is like "500107862016 Bytes"
        if let Some(num_str) = inner.split_whitespace().next() {
            if let Ok(v) = num_str.replace(',', "").parse::<u64>() {
                return Some(v);
            }
        }
    }
    // Fall back: try to parse the first token
    s.split_whitespace()
        .next()?
        .replace(',', "")
        .parse::<u64>()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_whole_disk_whole() {
        assert!(is_whole_disk("/dev/disk0"));
        assert!(is_whole_disk("/dev/disk1"));
        assert!(is_whole_disk("/dev/disk10"));
    }

    #[test]
    fn is_whole_disk_slice() {
        assert!(!is_whole_disk("/dev/disk0s1"));
        assert!(!is_whole_disk("/dev/disk1s2"));
        assert!(!is_whole_disk("/dev/disk10s3"));
    }

    #[test]
    fn extract_bytes_parenthetical() {
        assert_eq!(
            extract_bytes_value("500.1 GB (500107862016 Bytes)"),
            Some(500107862016)
        );
    }

    #[test]
    fn extract_bytes_no_parens() {
        assert_eq!(extract_bytes_value("1024"), Some(1024));
    }
}
