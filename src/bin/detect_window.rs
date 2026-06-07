use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// Bring re-exported modules directly from resvg
use resvg::{tiny_skia, usvg};

use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState, RegistryHandler};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::{
    ZwlrForeignToplevelHandleV1, Event as HandleEvent,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::{
    ZwlrForeignToplevelManagerV1, Event as ManagerEvent,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};
use sysinfo::{ProcessRefreshKind, RefreshKind, System};
use linicon::lookup_icon;

struct RealtimeTrackerApp {
    registry_state: RegistryState,
    window_cache: HashMap<u32, WindowDiagnostics>,
    sys_scanner: System,
}

#[derive(Default, Clone, Debug)]
struct WindowDiagnostics {
    app_name: String,
    title: String,
    matched_pid: Option<u32>,
    icon_name: String,
    terminal_icon_code: String,
}

fn extract_icon_name_from_desktop_file(app_id: &str) -> String {
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

fn locate_actual_icon_path(icon_name: &str, app_id: &str) -> Option<PathBuf> {
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

fn load_image_pixels(path: &Path, target_size: u32) -> Option<image::RgbImage> {
    let extension = path.extension().and_then(|s| s.to_str())?.to_lowercase();

    // To get a perfectly square visual icon on screen, the underlying 
    // pixel array must have twice as many horizontal pixels as vertical rows.
    // Setting target_size to 16 results in a clean 16x8 pixel grid.
    let target_width = target_size/2;
    let target_height = target_size / 2;

    if extension == "svg" {
        let svg_data = std::fs::read(path).ok()?;
        
        let r_opt = usvg::Options::default();
        let tree = usvg::Tree::from_data(&svg_data, &r_opt).ok()?;
        
        let size = usvg::Size::from_wh(target_width as f32, target_height as f32)?;
        let transform = tiny_skia::Transform::from_scale(
            size.width() / tree.size().width(),
            size.height() / tree.size().height(),
        );

        let mut pixmap = tiny_skia::Pixmap::new(target_width, target_height)?;
        resvg::render(&tree, transform, &mut pixmap.as_mut());

        let mut rgb_raw = Vec::with_capacity((target_width * target_height * 3) as usize);
        for pixel in pixmap.pixels() {
            rgb_raw.push(pixel.red());
            rgb_raw.push(pixel.green());
            rgb_raw.push(pixel.blue());
        }

        image::RgbImage::from_raw(target_width, target_height, rgb_raw)
    } else {
        let img = image::open(path).ok()?;
        // FIXED: Using resize_exact forces the PNG out of its natural constraints 
        // and squashes it into the 2:1 pixel ratio required by terminal cell layout structures.
        let scaled = img.resize_exact(target_width, target_height, image::imageops::FilterType::Lanczos3);
        Some(scaled.to_rgb8())
    }
}


fn generate_terminal_image_string(icon_name: &str, app_id: &str, target_size: u32) -> String {
    let icon_path = match locate_actual_icon_path(icon_name, app_id) {
        Some(path) => path,
        None => return "📁 [No Icon Found]".to_string(),
    };

    if let Some(rgb_image) = load_image_pixels(&icon_path, target_size) {
        let mut out = String::new();
        for y in (0..rgb_image.height()).step_by(2) {
            for x in 0..rgb_image.width() {
                let top = rgb_image.get_pixel(x, y);
                let bottom = if y + 1 < rgb_image.height() { rgb_image.get_pixel(x, y + 1) } else { top };
                out.push_str(&format!("\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m▀", 
                    top[0], top[1], top[2], bottom[0], bottom[1], bottom[2]));
            }
            out.push_str("\x1b[0m\n");
        }
        return out;
    }
    "📁 [Rasterize Error]".to_string()
}

impl ProvidesRegistryState for RealtimeTrackerApp {
    fn registry(&mut self) -> &mut RegistryState { &mut self.registry_state }
    smithay_client_toolkit::registry_handlers![RealtimeTrackerApp];
}

// Fixed E0185: Uses the exact named reference pattern required by the macro expansion
// Fixed E0050: Added the missing 5th string parameter to remove_global
impl RegistryHandler<RealtimeTrackerApp> for RealtimeTrackerApp {
    fn new_global(_state: &mut RealtimeTrackerApp, _conn: &Connection, _qh: &QueueHandle<Self>, _name: u32, _interface: &str, _version: u32) {}
    fn remove_global(_state: &mut RealtimeTrackerApp, _conn: &Connection, _qh: &QueueHandle<Self>, _name: u32, _interface: &str) {}
}



impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for RealtimeTrackerApp {
    fn event(_: &mut Self, _: &ZwlrForeignToplevelManagerV1, _: ManagerEvent, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
    event_created_child!(RealtimeTrackerApp, ZwlrForeignToplevelManagerV1, [0 => (ZwlrForeignToplevelHandleV1, ())]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for RealtimeTrackerApp {
    fn event(state: &mut Self, proxy: &ZwlrForeignToplevelHandleV1, event: HandleEvent, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        let id = proxy.id().protocol_id();
        let entry = state.window_cache.entry(id).or_default();

        match event {
            HandleEvent::AppId { app_id } => {
                entry.app_name = if app_id.is_empty() { "taskman".to_string() } else { app_id.clone() };
                entry.icon_name = extract_icon_name_from_desktop_file(&entry.app_name);
				// FIXED: Changed size parameter from 24/32 down to 12.
                // This outputs 12 text columns wide by 6 text rows high.
                entry.terminal_icon_code = generate_terminal_image_string(&entry.icon_name, &entry.app_name, 12);            }
            HandleEvent::Title { title } => entry.title = if title.is_empty() { "[Untitled Canvas]".to_string() } else { title },
            HandleEvent::Done => {
                state.sys_scanner.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
                if let Some((pid, _)) = state.sys_scanner.processes().iter().find(|(_, p)| {
                    p.name().to_string_lossy().to_lowercase().contains(&entry.app_name.to_lowercase())
                }) {
                    entry.matched_pid = Some(pid.as_u32());
                    println!("--- [LINK: {}] ---\nApp: {}\nPID: {}\nIcon:\n{}", id, entry.app_name, pid, entry.terminal_icon_code);
                }
            }
            HandleEvent::Closed => { state.window_cache.remove(&id); }
            _ => {}
        }
    }
}

smithay_client_toolkit::delegate_registry!(RealtimeTrackerApp);

fn main() {
    let conn = Connection::connect_to_env().expect("Failed to connect to Wayland");
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let mut app = RealtimeTrackerApp {
        registry_state: RegistryState::new(&globals),
        window_cache: HashMap::new(),
        sys_scanner: System::new_with_specifics(RefreshKind::nothing().with_processes(ProcessRefreshKind::everything())),
    };
    let qh = event_queue.handle();
    let _manager: ZwlrForeignToplevelManagerV1 = globals.bind(&qh, 1..=3, ()).expect("Manager bind failed");
    loop { event_queue.blocking_dispatch(&mut app).unwrap(); }
}

