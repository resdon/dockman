use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::{BufRead, BufReader};
use walkdir::WalkDir;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use linicon::lookup_icon;

// Re-exports for a clean public API
pub use crate::models::WindowDiagnostics;

pub mod models {
    #[derive(Default, Clone, Debug)]
    pub struct WindowDiagnostics {
        pub app_name: String,
        pub title: String,
        pub matched_pid: Option<u32>,
        pub icon_name: String,
        pub terminal_icon_code: String,
	    pub app_id: String,       
	    pub is_activated: bool,
	    pub is_minimized: bool,
	    
    }
}

pub mod icon_utils {
    use super::*;

    pub fn extract_icon_name_from_desktop_file(app_id: &str) -> String {
        if app_id.is_empty() { return "unknown-icon".to_string(); }
        let xdg_data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
        let mut search_paths: Vec<PathBuf> = xdg_data_dirs.split(':').map(|s| Path::new(s).join("applications")).collect();
        if let Ok(home) = std::env::var("HOME") { search_paths.insert(0, Path::new(&home).join(".local/share/applications")); }

        let candidates = [
            format!("{}.desktop", app_id),
            format!("{}.desktop", app_id.to_lowercase()),
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
                                return line["Icon=".len()..].trim().to_string();
                            }
                        }
                    }
                }
            }
        }
        app_id.to_string()
    }

    pub fn locate_actual_icon_path(icon_name: &str, app_id: &str) -> Option<PathBuf> {
        if let Some(icon) = lookup_icon(icon_name).from_theme("hicolor").next().and_then(|res| res.ok()) {
            return Some(icon.path);
        }
        if let Some(icon) = lookup_icon(app_id).from_theme("hicolor").next().and_then(|res| res.ok()) {
            return Some(icon.path);
        }

        let mut search_roots = vec![PathBuf::from("/usr/share/icons"), PathBuf::from("/usr/share/pixmaps")];
        if let Ok(home) = std::env::var("HOME") {
            search_roots.push(Path::new(&home).join(".icons"));
            search_roots.push(Path::new(&home).join(".local/share/icons"));
        }

        let target_lower = icon_name.to_lowercase();
        let app_lower = app_id.to_lowercase();

        for root in search_roots {
            if !root.exists() { continue; }
            for entry in WalkDir::new(root).follow_links(false).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                        if ext == "png" || ext == "svg" {
                            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                                let name_lower = file_name.to_lowercase();
                                if name_lower.contains(&target_lower) || name_lower.contains(&app_lower) {
                                    return Some(path.to_path_buf());
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

pub mod terminal_graphics {
    use super::*;

    pub fn load_image_raw_rgba(path: &Path, target_size: u32) -> Option<(u32, u32, Vec<u8>)> {
        let extension = path.extension().and_then(|s| s.to_str())?.to_lowercase();

        if extension == "svg" {
            let svg_data = std::fs::read(path).ok()?;
            let tree = resvg::usvg::Tree::from_data(&svg_data, &resvg::usvg::Options::default()).ok()?;
            let mut pixmap = resvg::tiny_skia::Pixmap::new(target_size, target_size)?;
            
            let transform = resvg::tiny_skia::Transform::from_scale(
                target_size as f32 / tree.size().width(),
                target_size as f32 / tree.size().height(),
            );
            resvg::render(&tree, transform, &mut pixmap.as_mut());
            
            Some((target_size, target_size, pixmap.data().to_vec()))
        } else {
            let img = image::open(path).ok()?;
            let scaled = img.resize_exact(target_size, target_size, image::imageops::FilterType::Lanczos3);
            let rgba = scaled.to_rgba8();
            Some((rgba.width(), rgba.height(), rgba.into_raw()))
        }
    }

    pub fn generate_terminal_image_string(icon_name: &str, app_id: &str, target_size: u32) -> String {
        let icon_path = match crate::icon_utils::locate_actual_icon_path(icon_name, app_id) {
            Some(path) => path,
            None => return "📁 [No Icon Found]".to_string(),
        };

        if let Some((w, h, raw_bytes)) = load_image_raw_rgba(&icon_path, target_size) {
            let b64_data = STANDARD.encode(&raw_bytes);
            format!("\x1b_Ga=T,f=32,s={},v={};{}\x1b\\", w, h, b64_data)
        } else {
            "📁 [Rasterize Error]".to_string()
        }
    }
}
