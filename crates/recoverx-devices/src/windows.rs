//! Windows device enumeration via Win32 APIs.
//!
//! Uses `\\.\PhysicalDriveN` enumeration by attempting to open drives 0–15.
//! This works without WMI and without additional crates.
//! TODO: add WMI queries for richer model/serial/size data.

use crate::provider::{DeviceInfo, DeviceProvider, DeviceType};
use recoverx_core::error::{RecoverXError, Result};

pub struct WindowsDeviceProvider;

impl WindowsDeviceProvider {
    pub fn new() -> Self {
        WindowsDeviceProvider
    }
}

impl Default for WindowsDeviceProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceProvider for WindowsDeviceProvider {
    fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        let mut devices = Vec::new();

        // Probe physical drives 0 through 15
        for i in 0..16u32 {
            let path = format!(r"\\.\PhysicalDrive{}", i);
            match self.get_device_info(&path) {
                Ok(info) => devices.push(info),
                Err(RecoverXError::DeviceNotFound { .. }) => break, // No more drives
                Err(e) => {
                    tracing::debug!("Skipping {}: {}", path, e);
                }
            }
        }

        Ok(devices)
    }

    fn get_device_info(&self, path: &str) -> Result<DeviceInfo> {
        // On Windows we use the winapi crate to open the device and query size.
        #[cfg(target_os = "windows")]
        {
            use std::ffi::OsStr;
            use std::os::windows::ffi::OsStrExt;
            use std::os::windows::io::AsRawHandle;
            use winapi::shared::minwindef::DWORD;
            use winapi::um::fileapi::{CreateFileW, OPEN_EXISTING};
            use winapi::um::handleapi::INVALID_HANDLE_VALUE;
            use winapi::um::winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_READ};

            let wide_path: Vec<u16> = OsStr::new(path)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            let handle = unsafe {
                CreateFileW(
                    wide_path.as_ptr(),
                    GENERIC_READ,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null_mut(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                )
            };

            if handle == INVALID_HANDLE_VALUE {
                let err = std::io::Error::last_os_error();
                return Err(match err.kind() {
                    std::io::ErrorKind::PermissionDenied => RecoverXError::InsufficientPrivileges {
                        resource: path.to_string(),
                        detail: "Run as Administrator to enumerate physical drives".to_string(),
                    },
                    std::io::ErrorKind::NotFound => RecoverXError::DeviceNotFound {
                        path: path.to_string(),
                    },
                    _ => RecoverXError::Io(err),
                });
            }

            // Close the handle — we only needed it to verify existence
            unsafe { winapi::um::handleapi::CloseHandle(handle) };

            return Ok(DeviceInfo {
                path: path.to_string(),
                name: path.to_string(),
                size_bytes: 0, // TODO: IOCTL_DISK_GET_DRIVE_GEOMETRY_EX
                sector_size: 512,
                device_type: DeviceType::Fixed,
                is_internal: true, // assume internal until WMI query
                filesystem: None,
                is_accessible: true,
                model: None,  // TODO: WMI Win32_DiskDrive
                serial: None, // TODO: WMI Win32_DiskDrive
                is_write_protected: false,
            });
        }

        #[cfg(not(target_os = "windows"))]
        Err(RecoverXError::DeviceEnumerationFailed(
            "Windows device provider called on non-Windows platform".to_string(),
        ))
    }
}
