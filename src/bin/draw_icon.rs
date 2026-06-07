use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Finds a valid icon or falls back to an alternative on Arch Linux
fn find_plausible_icon() -> Option<PathBuf> {
    let primary_path = PathBuf::from("/usr/share/icons/hicolor/48x48/apps/utilities-terminal.png");
    if primary_path.exists() {
        return Some(primary_path);
    }

    let search_roots = ["/usr/share/icons", "/usr/share/pixmaps"];
    for root_str in &search_roots {
        let base_path = Path::new(root_str);
        if !base_path.exists() { continue; }

        let walker = WalkDir::new(base_path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok());

        for entry in walker {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "png" {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            let lower = name.to_lowercase();
                            if lower.contains("terminal") || lower.contains("utilities") {
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

fn main() {
    println!("=== TERMINAL GRAPHICS RENDERER (Alacritty True-Color Fallback) ===");

    let icon_path = match find_plausible_icon() {
        Some(path) => path,
        None => {
            println!("Error: No fallback PNG icons found on the system.");
            return;
        }
    };

    println!("Rendering image asset: {}\n", icon_path.display());

    // Decode using the image crate
    if let Ok(img) = image::open(&icon_path) {
        // Scale down sharply to fit inside standard text block limits
        // 32 text columns wide by 16 text rows high
        let scaled = img.resize(32, 16, image::imageops::FilterType::Nearest);
        let rgb_image = scaled.to_rgb8();

        // Loop through two horizontal rows of pixels at a time
        // Top pixel becomes foreground color, bottom pixel becomes background color
        for y in (0..rgb_image.height()).step_by(2) {
            for x in 0..rgb_image.width() {
                let top_pixel = rgb_image.get_pixel(x, y);
                
                let bottom_pixel = if y + 1 < rgb_image.height() {
                    rgb_image.get_pixel(x, y + 1)
                } else {
                    top_pixel
                };

                // \x1b[38;2;R;G;Bm -> sets foreground text color
                // \x1b[48;2;R;G;Bm -> sets background block color
                // ▀ -> Unicode half-block character
                print!(
                    "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m▀",
                    top_pixel[0], top_pixel[1], top_pixel[2],
                    bottom_pixel[0], bottom_pixel[1], bottom_pixel[2]
                );
            }
            // Clear colors at end of terminal row and line break
            println!("\x1b[0m");
        }
        println!("\nSuccess! Render completed via standard ANSI typography.");
    } else {
        println!("Failed to parse image arrays via the image utility crate.");
    }
}
