use std::fs;
use std::collections::HashSet;
use std::path::Path;

const CONFIG_FILE: &str = "/home/resdon/.config/dockman/pins.json";

pub fn load_pinned_apps() -> HashSet<String> {
    if let Ok(data) = fs::read_to_string(CONFIG_FILE) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        HashSet::new()
    }
}

pub fn save_pinned_apps(pinned: &HashSet<String>) {
    if let Some(parent) = Path::new(CONFIG_FILE).parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_string(pinned) {
        let _ = fs::write(CONFIG_FILE, data);
    }
}
