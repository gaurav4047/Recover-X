//! The `DeviceProvider` trait and shared `DeviceInfo` type.

use recoverx_core::error::Result;
use serde::{Deserialize, Serialize};

/// Type of a storage device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    /// Fixed internal disk (HDD / SSD / NVMe)
    Fixed,
    /// Removable USB drive, SD card, etc.
    Removable,
    /// Optical disc
    Optical,
    /// SD card / MMC
    SdCard,
    /// Virtual / loop device
    Virtual,
    Unknown,
}

impl std::fmt::Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceType::Fixed => write!(f, "Fixed"),
            DeviceType::Removable => write!(f, "Removable"),
            DeviceType::Optical => write!(f, "Optical"),
            DeviceType::SdCard => write!(f, "SD Card"),
            DeviceType::Virtual => write!(f, "Virtual"),
            DeviceType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Information about a detected storage device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// OS-level path (e.g., `/dev/disk2`, `/dev/sda`, `\\.\PhysicalDrive0`)
    pub path: String,
    /// Human-readable name or model
    pub name: String,
    /// Total capacity in bytes
    pub size_bytes: u64,
    /// Logical sector size
    pub sector_size: u32,
    /// Device classification
    pub device_type: DeviceType,
    /// Whether this device is an internal (built-in) disk
    pub is_internal: bool,
    /// Detected filesystem type (if a single FS is present), otherwise None
    pub filesystem: Option<String>,
    /// Whether this device can be opened read-only without privilege errors
    pub is_accessible: bool,
    /// Model string from hardware query
    pub model: Option<String>,
    /// Serial number from hardware query
    pub serial: Option<String>,
    /// Whether the OS reports this media as write-protected
    pub is_write_protected: bool,
}

/// Trait for platform-specific device enumeration.
pub trait DeviceProvider: Send + Sync {
    /// List all detected storage devices.
    ///
    /// Returns an explicit error if privileges are insufficient.
    /// Never returns fabricated devices.
    fn list_devices(&self) -> Result<Vec<DeviceInfo>>;

    /// Get detailed information about a single device at `path`.
    fn get_device_info(&self, path: &str) -> Result<DeviceInfo>;
}

// ── Fallback for unsupported platforms ───────────────────────────────────────

#[doc(hidden)]
pub struct UnsupportedDeviceProvider;

impl DeviceProvider for UnsupportedDeviceProvider {
    fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        Err(
            recoverx_core::error::RecoverXError::DeviceEnumerationFailed(
                "Device enumeration is not supported on this platform".to_string(),
            ),
        )
    }

    fn get_device_info(&self, path: &str) -> Result<DeviceInfo> {
        Err(recoverx_core::error::RecoverXError::DeviceNotFound {
            path: path.to_string(),
        })
    }
}
