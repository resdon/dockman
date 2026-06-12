use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// Base64 engine for terminal protocol payloads
use base64::{engine::general_purpose::STANDARD, Engine as _};

use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryHandler, RegistryState};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::{
    Event as HandleEvent, ZwlrForeignToplevelHandleV1,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::{
    Event as ManagerEvent, ZwlrForeignToplevelManagerV1,
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

fn load_image_raw_rgba(path: &Path, target_size: u32) -> Option<(u32, u32, Vec<u8>)> {
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
        println!("{:?}", path.display());
        let scaled = img.resize_exact(target_size, target_size, image::imageops::FilterType::Lanczos3);
        let rgba = scaled.to_rgba8();
        Some((rgba.width(), rgba.height(), rgba.into_raw()))
    }
}

fn generate_terminal_image_string(icon_name: &str, app_id: &str, target_size: u32) -> String {
    let icon_path = match locate_actual_icon_path(icon_name, app_id) {
        Some(path) => path,
        None => return "📁 [No Icon Found]".to_string(),
    };

    if let Some((w, h, raw_bytes)) = load_image_raw_rgba(&icon_path, target_size) {
        let b64_data = STANDARD.encode(&raw_bytes);
        // Kitty Graphics Protocol escape sequence
        format!("\x1b_Ga=T,f=32,s={},v={};{}\x1b\\", w, h, b64_data)
    } else {
        "📁 [Rasterize Error]".to_string()
    }
}

impl ProvidesRegistryState for RealtimeTrackerApp {
    fn registry(&mut self) -> &mut RegistryState { &mut self.registry_state }
    smithay_client_toolkit::registry_handlers![RealtimeTrackerApp];
}

// CORRECTED: Remove &mut self and use the explicit data parameter
impl RegistryHandler<RealtimeTrackerApp> for RealtimeTrackerApp {
    fn new_global(
        _data: &mut RealtimeTrackerApp, 
        _conn: &Connection, 
        _qh: &QueueHandle<RealtimeTrackerApp>, 
        _name: u32, 
        _interface: &str, 
        _version: u32
    ) {}

    fn remove_global(
        _data: &mut RealtimeTrackerApp, 
        _conn: &Connection, 
        _qh: &QueueHandle<RealtimeTrackerApp>, 
        _name: u32, 
        _interface: &str
    ) {}
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
                entry.app_name = if app_id.is_empty() { "taskman".to_string() } else { app_id };
                entry.icon_name = extract_icon_name_from_desktop_file(&entry.app_name);
                entry.terminal_icon_code = generate_terminal_image_string(&entry.icon_name, &entry.app_name, 32);
            }
            HandleEvent::Title { title } => entry.title = if title.is_empty() { "[Untitled]".to_string() } else { title },
			HandleEvent::Done => {
			    state.sys_scanner.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

			    if let Some((pid, _proc)) = state.sys_scanner.processes().iter().find(|(_, p)| {
			        let proc_name = p.name().to_string_lossy().to_lowercase();
			        let target_app = entry.app_name.to_lowercase();
			        proc_name.contains(&target_app) || target_app.contains(&proc_name)
			    }) {
			        entry.matched_pid = Some(pid.as_u32());

			        println!("---------------------------------------------------------");
			        println!("[LINK ESTABLISHED]: Wayland Handle -> Linux OS Process");
			        // Added the protocol ID below:
			        println!("  Wayland Protocol ID: {}", id); 
			        println!("  Wayland Window ID  : {}", id); 
			        println!("  Resolved App Name  : {}", entry.app_name);
			        println!("  System Icon Key    : {}", entry.icon_name);
			        println!("  Window Title Text  : {}", entry.title);
			        println!("  Linked Process PID : {}", pid.as_u32());
			        println!("  Application Icon   : {}", entry.terminal_icon_code); 
			        println!("---------------------------------------------------------");
			    }
			}
            HandleEvent::Closed => { state.window_cache.remove(&id); }
            _ => {}
        }
    }
}

smithay_client_toolkit::delegate_registry!(RealtimeTrackerApp);

fn main() {
    let conn = Connection::connect_to_env().expect("Wayland connection failed");
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
