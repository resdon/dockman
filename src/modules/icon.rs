use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use linicon::{lookup_icon, IconType};

/// Scans standard XDG directories to extract the inner "Icon=..." definition text block
fn extract_icon_name_from_desktop_file(app_id: &str) -> Option<String> {
    let xdg_data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    
    let mut search_paths: Vec<PathBuf> = xdg_data_dirs
        .split(':')
        .map(|s| Path::new(s).join("applications"))
        .collect();
        
    if let Ok(home) = std::env::var("HOME") {
        search_paths.insert(0, Path::new(&home).join(".local/share/applications"));
    }

    // Standard variations for mapping window app_id modifications
    let app_id_lower = app_id.to_lowercase();
    let candidates = [
        format!("{}.desktop", app_id),
        format!("{}.desktop", app_id_lower),
        format!("org.gnome.{}.desktop", app_id),
        format!("org.kde.{}.desktop", app_id),
    ];

    for path in search_paths {
        for candidate in &candidates {
            let desktop_path = path.join(candidate);
            if desktop_path.exists() {
                if let Ok(file) = File::open(desktop_path) {
                    let reader = BufReader::new(file);
                    for line in reader.lines().flatten() {
                        if line.starts_with("Icon=") {
                            return Some(line["Icon=".len()..].trim().to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Takes a window application identifier, extracts its file parameters, 
/// and parses it into raw scaled RGBA image vector pixel sets.
pub fn load_icon_rgba(app_id: &str, target_size: u32) -> Option<(Vec<u8>, u32, u32)> {
    if app_id.is_empty() { return None; }
    
    // Resolve the clean icon asset identifier key
    let icon_name = extract_icon_name_from_desktop_file(app_id)
        .unwrap_or_else(|| app_id.to_string());

    // Pull file system entries from current active theme via linicon
    let icon_search = lookup_icon(&icon_name)
        .from_theme("hicolor") // Fallback baseline target context string
        .size(target_size)
        .min_type(IconType::PNG) // Prioritize bitmap textures
        .next()
        .and_then(|res| res.ok());

    let icon_path = match icon_search {
        Some(icon) => icon.path,
        None => return None,
    };

    // Load and decode the file into an RGBA vector slice layout
    if let Ok(img) = image::open(&icon_path) {
        let scaled = img.resize(target_size, target_size, image::imageops::FilterType::Lanczos3);
        let rgba = scaled.to_rgba();
        return Some((rgba.clone().into_raw(), scaled.width(), scaled.height()));
    }
    None
}
