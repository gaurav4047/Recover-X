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
}

/// Return the disk size (bytes) of a file or directory (recursive for dirs).
#[tauri::command]
pub fn get_path_size(path: String) -> Result<u64, String> {
    fn dir_size(p: &std::path::Path) -> u64 {
        let mut total = 0u64;
        if let Ok(entries) = std::fs::read_dir(p) {
            for entry in entries.filter_map(|e| e.ok()) {
                let child = entry.path();
                if child.is_file() {
                    total += child.metadata().map(|m| m.len()).unwrap_or(0);
                } else if child.is_dir() {
                    total += dir_size(&child);
                }
            }
        }
        total
    }

    let p = std::path::Path::new(&path);
    if p.is_file() {
        return p.metadata().map(|m| m.len()).map_err(|e| e.to_string());
    }
    Ok(dir_size(p))
}

/// Return the standard user directories on this system.
#[tauri::command]
pub fn get_common_locations() -> Vec<CommonLocation> {
    let home = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/"));

    let mut locations = vec![
        CommonLocation {
            label: "Downloads".to_string(),
            path: home.join("Downloads").display().to_string(),
            icon: "⬇️".to_string(),
            exists: home.join("Downloads").exists(),
            is_trash: false,
        },
        CommonLocation {
            label: "Documents".to_string(),
            path: home.join("Documents").display().to_string(),
            icon: "📄".to_string(),
            exists: home.join("Documents").exists(),
            is_trash: false,
        },
        CommonLocation {
            label: "Desktop".to_string(),
            path: home.join("Desktop").display().to_string(),
            icon: "🖥️".to_string(),
            exists: home.join("Desktop").exists(),
            is_trash: false,
        },
        CommonLocation {
            label: "Pictures".to_string(),
            path: home.join("Pictures").display().to_string(),
            icon: "🖼️".to_string(),
            exists: home.join("Pictures").exists(),
            is_trash: false,
        },
        CommonLocation {
            label: "Movies".to_string(),
            path: home.join("Movies").display().to_string(),
            icon: "🎬".to_string(),
            exists: home.join("Movies").exists(),
            is_trash: false,
        },
        CommonLocation {
            label: "Music".to_string(),
            path: home.join("Music").display().to_string(),
            icon: "🎵".to_string(),
            exists: home.join("Music").exists(),
            is_trash: false,
        },
        CommonLocation {
            label: "Home".to_string(),
            path: home.display().to_string(),
            icon: "🏠".to_string(),
            exists: home.exists(),
            is_trash: false,
        },
    ];

    // Trash locations (platform-specific)
    #[cfg(target_os = "macos")]
    {
        let trash = home.join(".Trash");
        if trash.exists() {
            locations.push(CommonLocation {
                label: "Trash".to_string(),
                path: trash.display().to_string(),
                icon: "🗑️".to_string(),
                exists: true,
                is_trash: true,
            });
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
        });
    }

    locations
}

/// Check whether this process has Full Disk Access on macOS.
///
/// Attempts to open a protected system path that is only readable with FDA.
/// On Linux/Windows always returns true (no equivalent permission gate).
#[tauri::command]
pub fn check_full_disk_access() -> bool {
    #[cfg(target_os = "macos")]
    {
        // /dev/disk0 is only openable (even read-only) when FDA is granted or
        // the process is running as root.  A plain user without FDA gets EPERM.
        std::fs::OpenOptions::new()
            .read(true)
            .open("/dev/disk0")
            .is_ok()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}
