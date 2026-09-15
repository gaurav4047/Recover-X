use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub description: String,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: "RecoverX".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Data Recovery Tool".to_string(),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommonLocation {
    pub label: String,
    pub path: String,
    pub icon: String,
    pub exists: bool,
    pub is_trash: bool,
    /// If true, scanning this location finds genuinely deleted files.
    /// If false, the user must select a storage device instead.
    pub is_recoverable_source: bool,
    pub hint: String,
}

/// Return scan source locations.
///
/// Only Trash directories and physical devices are valid recovery sources.
/// User folders (Downloads, Documents, etc.) are NOT included because walking
/// a live folder and reporting current files as "deleted" is misleading.
#[tauri::command]
pub fn get_common_locations() -> Vec<CommonLocation> {
    let home = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/"));

    let mut locations = Vec::new();

    // ── Trash locations — these are genuine deleted-file sources ─────────────

    #[cfg(target_os = "macos")]
    {
        let trash = home.join(".Trash");
        locations.push(CommonLocation {
            label: "Trash".to_string(),
            path: trash.display().to_string(),
            icon: "🗑️".to_string(),
            exists: trash.exists(),
            is_trash: true,
            is_recoverable_source: true,
            hint: "Files you deleted and moved to Trash but haven't permanently removed yet.".to_string(),
        });

        // External drive Trashes
        if let Ok(vols) = std::fs::read_dir("/Volumes") {
            for entry in vols.flatten() {
                let trashes = entry.path().join(".Trashes");
                if trashes.exists() {
                    locations.push(CommonLocation {
                        label: format!("{} Trash", entry.file_name().to_string_lossy()),
                        path: trashes.display().to_string(),
                        icon: "🗑️".to_string(),
                        exists: true,
                        is_trash: true,
                        is_recoverable_source: true,
                        hint: "Deleted files from this external volume.".to_string(),
                    });
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let trash = home.join(".local/share/Trash/files");
        locations.push(CommonLocation {
            label: "Trash".to_string(),
            path: trash.display().to_string(),
            icon: "🗑️".to_string(),
            exists: trash.exists(),
            is_trash: true,
            is_recoverable_source: true,
            hint: "Files in the system Trash.".to_string(),
        });
    }

    #[cfg(target_os = "windows")]
    {
        locations.push(CommonLocation {
            label: "Recycle Bin".to_string(),
            path: "C:\\$Recycle.Bin".to_string(),
            icon: "🗑️".to_string(),
            exists: std::path::Path::new("C:\\$Recycle.Bin").exists(),
            is_trash: true,
            is_recoverable_source: true,
            hint: "Files in the Windows Recycle Bin.".to_string(),
        });
    }

    locations
}
