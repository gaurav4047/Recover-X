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
        locations.push(CommonLocation {
            label: "Trash".to_string(),
            path: trash.display().to_string(),
            icon: "🗑️".to_string(),
            exists: trash.exists(),
            is_trash: true,
        });
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
