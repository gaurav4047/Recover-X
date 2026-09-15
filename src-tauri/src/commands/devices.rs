use recoverx_devices::{platform_provider, DeviceInfo};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceListResponse {
    pub devices: Vec<DeviceInfo>,
    pub error: Option<String>,
}

/// List all detected storage devices on the current platform.
///
/// Returns real device information. If privileges are insufficient,
/// returns an explicit error — never fabricated device data.
#[tauri::command]
pub async fn list_devices(_state: State<'_, AppState>) -> Result<DeviceListResponse, String> {
    let provider = platform_provider();
    match provider.list_devices() {
        Ok(devices) => Ok(DeviceListResponse {
            devices,
            error: None,
        }),
        Err(e) => Ok(DeviceListResponse {
            devices: vec![],
            error: Some(e.to_string()),
        }),
    }
}

/// Get detailed info about a specific device path.
#[tauri::command]
pub async fn get_device_info(
    _state: State<'_, AppState>,
    path: String,
) -> Result<DeviceInfo, String> {
    let provider = platform_provider();
    provider.get_device_info(&path).map_err(|e| e.to_string())
}
