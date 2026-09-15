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
    /// The path to scan (device path, Trash path, or folder path)
    pub path: String,
    pub icon: String,
    pub exists: bool,
    pub is_trash: bool,
    /// For folder locations: the original folder whose deleted files we want.
    /// The scanner will search the Trash for files that came from this folder.
    pub filter_prefix: Option<String>,
    pub hint: String,
}

/// Return all scan source locations.
#[tauri::command]
pub fn get_common_locations() -> Vec<CommonLocation> {
    let home = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/"));

    let trash = home.join(".Trash");
    let trash_path = trash.display().to_string();
    let trash_exists = trash.exists();

    let mut locations = Vec::new();

    // ── User folder shortcuts ─────────────────────────────────────────────
    // These show files deleted FROM that folder (found in Trash with matching origin).
    // Source path = Trash; filter_prefix = the folder.

    let folders = vec![
        ("Downloads", "⬇️", home.join("Downloads")),
        ("Documents", "📄", home.join("Documents")),
        ("Desktop",   "🖥️", home.join("Desktop")),
        ("Pictures",  "🖼️", home.join("Pictures")),
        ("Movies",    "🎬", home.join("Movies")),
        ("Music",     "🎵", home.join("Music")),
    ];

    for (label, icon, folder_path) in folders {
        locations.push(CommonLocation {
            label: label.to_string(),
            path: trash_path.clone(),          // scan the Trash…
            icon: icon.to_string(),
            exists: trash_exists,
            is_trash: true,
            filter_prefix: Some(folder_path.display().to_string()), // …for files from this folder
            hint: format!(
                "Find files deleted from {} that are still in your Trash.",
                folder_path.display()
            ),
        });
    }

    // ── Trash (all deleted files, no folder filter) ───────────────────────
    locations.push(CommonLocation {
        label: "All Trash".to_string(),
        path: trash_path.clone(),
        icon: "🗑️".to_string(),
        exists: trash_exists,
        is_trash: true,
        filter_prefix: None,
        hint: "All files currently in your Trash, regardless of origin.".to_string(),
    });

    // ── External volume Trashes ───────────────────────────────────────────
    #[cfg(target_os = "macos")]
    if let Ok(vols) = std::fs::read_dir("/Volumes") {
        for entry in vols.flatten() {
            let trashes = entry.path().join(".Trashes");
            if trashes.exists() {
                locations.push(CommonLocation {
                    label: format!("{} Trash", entry.file_name().to_string_lossy()),
                    path: trashes.display().to_string(),
                    icon: "💾".to_string(),
                    exists: true,
                    is_trash: true,
                    filter_prefix: None,
                    hint: "Deleted files from this external volume.".to_string(),
                });
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
            filter_prefix: None,
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
            filter_prefix: None,
            hint: "Files in the Windows Recycle Bin.".to_string(),
        });
    }

    locations
}
