//! Linux device enumeration via `/sys/block` and `/proc/partitions`.
//!
//! Reads sysfs and proc filesystems — no elevated privileges required for
//! the enumeration step itself. Opening a raw device for reading does
//! require privileges (which we report explicitly).

use std::fs;
use std::path::Path;

use recoverx_core::error::{RecoverXError, Result};

use crate::provider::{DeviceInfo, DeviceProvider, DeviceType};

pub struct LinuxDeviceProvider;

impl LinuxDeviceProvider {
    pub fn new() -> Self {
        LinuxDeviceProvider
    }
}

impl Default for LinuxDeviceProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceProvider for LinuxDeviceProvider {
    fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        let sys_block = Path::new("/sys/block");

        if !sys_block.exists() {
            return Err(RecoverXError::DeviceEnumerationFailed(
                "/sys/block not found — is this Linux?".to_string(),
            ));
        }

        let mut devices = Vec::new();

        let entries = fs::read_dir(sys_block).map_err(|e| {
            RecoverXError::DeviceEnumerationFailed(format!("Cannot read /sys/block: {}", e))
        })?;

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // Skip loop devices and ram disks in basic listing
            if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("zram") {
                continue;
            }
            let dev_path = format!("/dev/{}", name);
            match self.get_device_info(&dev_path) {
                Ok(info) => devices.push(info),
                Err(e) => {
                    tracing::debug!("Skipping /dev/{}: {}", name, e);
                }
            }
        }

        Ok(devices)
    }

    fn get_device_info(&self, path: &str) -> Result<DeviceInfo> {
        let dev_name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .ok_or_else(|| RecoverXError::DeviceNotFound {
                path: path.to_string(),
            })?;

        let sys_path = format!("/sys/block/{}", dev_name);

        // Read size in 512-byte sectors
        let size_bytes = read_sysfs_u64(&format!("{}/size", sys_path))
            .map(|sectors| sectors * 512)
            .unwrap_or(0);

        // Read logical sector size
        let sector_size =
            read_sysfs_u64(&format!("{}/queue/logical_block_size", sys_path)).unwrap_or(512) as u32;

        // Removable flag
        let removable = read_sysfs_u64(&format!("{}/removable", sys_path))
            .map(|v| v == 1)
            .unwrap_or(false);

        // Model (may not be present for all devices)
        let model = fs::read_to_string(format!("{}/device/model", sys_path))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        // Serial (usually requires root to read from /sys)
        let serial = fs::read_to_string(format!("{}/device/serial", sys_path))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let device_type = if removable {
            DeviceType::Removable
        } else if dev_name.starts_with("sd")
            || dev_name.starts_with("nvme")
            || dev_name.starts_with("vd")
        {
            DeviceType::Fixed
        } else if dev_name.starts_with("sr") {
            DeviceType::Optical
        } else if dev_name.starts_with("mmcblk") {
            DeviceType::SdCard
        } else {
            DeviceType::Unknown
        };

        let is_accessible = std::fs::File::open(path).is_ok();

        let is_internal = !removable
            && (dev_name.starts_with("nvme")
                || dev_name.starts_with("sd")
                || dev_name.starts_with("vd"));

        Ok(DeviceInfo {
            path: path.to_string(),
            name: model.clone().unwrap_or_else(|| dev_name.clone()),
            size_bytes,
            sector_size,
            device_type,
            is_internal,
            filesystem: None,
            is_accessible,
            model,
            serial,
            is_write_protected: false,
        })
    }
}

fn read_sysfs_u64(path: &str) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse::<u64>().ok()
}
