use linicon::{lookup_icon};
use std::path::PathBuf;

/// Searches for an icon path using the Freedesktop specification.
/// 'theme_name' is typically retrieved from your system settings (e.g., "Adwaita").
pub fn get_icon_path(icon_name: &str, size: u32, theme_name: &str) -> Option<PathBuf> {
    // 1. linicon handles the directory search order ($HOME/.icons, /usr/share/icons, etc.)
    // 2. lookup_icon searches for the base name
    // 3. from_theme ensures we respect user configuration
    lookup_icon(icon_name)
        .from_theme(theme_name)
        .with_size(size as u16)
        .next()
        .and_then(|result| result.ok())
        .map(|icon| icon.path)
}

pub fn main() {
    let icon_name = "firefox"; // Example: icon name from a .desktop file
    let theme = "Adwaita";     // You can read this from ~/.config/gtk-3.0/settings.ini
    let size = 48;

    match get_icon_path(icon_name, size, theme) {
        Some(path) => println!("Found icon at: {:?}", path),
        None => println!("Icon '{}' not found in theme '{}'", icon_name, theme),
    }
}
