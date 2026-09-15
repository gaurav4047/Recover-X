//! Platform-specific device enumeration.
//!
//! Provides a `DeviceProvider` trait and three implementations:
//! - `MacOSDeviceProvider`
//! - `LinuxDeviceProvider`
//! - `WindowsDeviceProvider`
//!
//! Each implementation returns REAL device information or an explicit
//! `InsufficientPrivileges` error. Devices are NEVER fabricated.

pub mod provider;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

pub use provider::{DeviceInfo, DeviceProvider, DeviceType};

/// Return the platform-appropriate provider.
pub fn platform_provider() -> Box<dyn DeviceProvider> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOSDeviceProvider::new())
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxDeviceProvider::new())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsDeviceProvider::new())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Box::new(provider::UnsupportedDeviceProvider)
    }
}
