use std::collections::HashMap;
use std::fs;
use std::path::Path;
use lazy_static::lazy_static;

// Simple parser for [Desktop Entry]
fn parse_desktop_file(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let content = fs::read_to_string(path).unwrap_or_default();
    let mut in_desktop_entry = false;
    for line in content.lines() {
        let line = line.trim();
        if line == "[Desktop Entry]" {
            in_desktop_entry = true;
            continue;
        }
        if in_desktop_entry {
            if line.starts_with('[') { break; } // Next section
            if let Some((key, value)) = line.split_once('=') {
                map.insert(key.to_string(), value.to_string());
            }
        }
    }
    map
}

lazy_static! {
    static ref DESKTOP_INDEX: (HashMap<String, String>, HashMap<String, String>) = {
        let mut icon_map = HashMap::new();
        let mut exec_map = HashMap::new();
        let home = std::env::var("HOME").unwrap_or_default();
        let paths = vec![
            "/usr/share/applications".to_string(),
            format!("{}/.local/share/applications", home),
        ];

        for path in paths {
            if let Ok(entries) = fs::read_dir(&path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if path.extension().map_or(false, |e| e == "desktop") {
                            let desktop_entry = parse_desktop_file(&path);
                            
                            let icon = desktop_entry.get("Icon").cloned();
                            let exec = desktop_entry.get("Exec").cloned();
                            let wm_class = desktop_entry.get("StartupWMClass").cloned();
                            let file_stem = path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string());

                            if let (Some(icon), Some(exec)) = (icon, exec) {
                                // Map by StartupWMClass if available
                                if let Some(wm_class) = wm_class {
                                    icon_map.entry(wm_class.clone()).or_insert_with(|| icon.clone());
                                    exec_map.entry(wm_class).or_insert_with(|| exec.clone());
                                }

                                // Map by Exec base name
                                let base_exec = exec.split_whitespace().next()
                                    .and_then(|s| s.split('/').last())
                                    .unwrap_or(&exec);
                                let base_exec_str = base_exec.to_string();
                                icon_map.entry(base_exec_str.clone()).or_insert_with(|| icon.clone());
                                exec_map.entry(base_exec_str.clone()).or_insert_with(|| exec.clone());

                                // Manual alias: steamwebhelper is steam
                                if base_exec_str == "steam" {
                                    icon_map.entry("steamwebhelper".to_string()).or_insert_with(|| icon.clone());
                                    exec_map.entry("steamwebhelper".to_string()).or_insert_with(|| exec.clone());
                                }

                                // Always map by file stem as a strong fallback
                                if let Some(stem) = file_stem {
                                    icon_map.entry(stem.clone()).or_insert_with(|| icon.clone());
                                    exec_map.entry(stem).or_insert_with(|| exec.clone());
                                }
                            }
                        }
                    }
                }
            } else {
                eprintln!("Failed to read directory: {:?}", path);
            }
        }
        (icon_map, exec_map)
    };

    pub static ref WMCLASS_TO_ICON: HashMap<String, String> = {
        DESKTOP_INDEX.0.clone()
    };

    pub static ref WMCLASS_TO_EXEC: HashMap<String, String> = {
        DESKTOP_INDEX.1.clone()
    };
}

pub fn get_icon(app_id: &str) -> Option<String> {
    WMCLASS_TO_ICON.get(app_id).cloned()
}
