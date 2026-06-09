pub mod lib;
pub mod cache;

use crate::lib::models::WindowDiagnostics;

use wayland_client::Dispatch;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::{
    compositor::CompositorState,
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
    shell::wlr_layer::{Anchor, Layer, LayerShell, LayerSurface},
    shm::slot::{Buffer, SlotPool},
    shm::Shm,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_pointer::WlPointer, wl_seat::WlSeat};
use wayland_client::Connection;
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1;
use wayland_client::backend::ObjectId;

use std::collections::HashMap;

// 1. Mount the files as local root modules
pub mod handlers;
pub mod render;
pub mod modules;


// ...
use modules::persistence;

// 2. Mock FontManager structure to fix E0425 and E0433
pub struct FontManager {
    pub font: fontdue::Font,
}
impl FontManager {
    pub fn new(bytes: &[u8]) -> Self { 
        let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).unwrap();
        FontManager { font } 
    }
}

pub struct MenuState {
    pub x: usize,
    pub y: usize,
    pub target_window: Option<ObjectId>,
    pub target_app_id: Option<String>,
    pub is_open: bool,
}

pub struct HoverState {
    pub x: usize,
    pub app_id: Option<String>,
    pub is_visible: bool,
    pub last_leave_time: Option<std::time::Instant>,
}

pub struct AppState {
	pub connection: Connection,
    pub registry_state: RegistryState,
    pub compositor_state: CompositorState,
    pub output_state: OutputState,
    pub layer_shell: LayerShell,
    pub shm_state: Shm,
    pub pool: SlotPool,
    pub seat_state: SeatState,
    pub layer_surface: Option<LayerSurface>,
    pub current_buffer: Option<Buffer>,
    pub width: u32,
    pub height: u32,
    pub toplevel_manager: Option<ZwlrForeignToplevelManagerV1>,
    pub font_manager: FontManager,
    pub wl_seat: Option<WlSeat>,
    pub wl_pointer: Option<WlPointer>,
    pub pointer_x: usize,
    pub pointer_y: usize,
    pub open_windows: HashMap<ObjectId, WindowDiagnostics>,
    pub pinned_apps: Vec<String>,
    pub icon_cache: HashMap<String, (Vec<u8>, u32)>,
    pub menu_state: MenuState,
    pub hover_state: HoverState,
    pub last_interact_time: std::time::Instant,
    pub needs_redraw: bool,
    pub last_mouse_pos: Option<(f64, f64)>,
}

impl AppState {
    pub fn draw(&mut self, qh: &wayland_client::QueueHandle<Self>) {
        let box_size = 48;
        let spacing = 12;
        let max_dock_width = 800; // Hard limit for your screen

        // Grouping logic
        let mut apps_in_dock: Vec<String> = self.pinned_apps.clone();
        for id in self.open_windows.values().map(|w| w.app_id.clone()) {
            if !apps_in_dock.contains(&id) { apps_in_dock.push(id); }
        }

        // Calculate dynamic width, but clamp to max_dock_width
        let total_items = apps_in_dock.len();
        let calculated_width = if total_items > 0 {
            (total_items * box_size + (total_items + 1) * spacing) as u32
        } else { 100 };
        
        self.width = calculated_width.min(max_dock_width); 
        self.height = if self.menu_state.is_open || self.hover_state.is_visible { 200 } else { 60 };

        if let Some(ref surface) = self.layer_surface {
            surface.set_size(self.width, self.height);
            let compositor = self.compositor_state.wl_compositor();
            let region = compositor.create_region(qh, ());
            region.add(0, 0, self.width as i32, self.height as i32);
            surface.wl_surface().set_input_region(Some(&region));
            surface.wl_surface().commit();
        }

        // 4. Create buffer and draw
        let (buffer, canvas) = self.pool
            .create_buffer(
                self.width as i32,
                self.height as i32,
                (self.width * 4) as i32,
                wayland_client::protocol::wl_shm::Format::Argb8888
            )
            .expect("Failed to create layout buffer");

        render::render_windows(
            canvas, self.width, self.height,
            &self.open_windows,
            &self.pinned_apps,
            &self.icon_cache,
            &self.menu_state,
            &self.hover_state,
            &self.font_manager
        );

        // 5. Commit the buffer
        if let Some(ref surface) = self.layer_surface {
            buffer.attach_to(surface.wl_surface()).expect("Buffer attach failed");
            surface.wl_surface().damage_buffer(0, 0, self.width as i32, self.height as i32);
            surface.wl_surface().commit();
        }

        self.current_buffer = Some(buffer);
        self.connection.flush().expect("Flush failed");
    }
}

impl Dispatch<wayland_client::protocol::wl_region::WlRegion, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &wayland_client::protocol::wl_region::WlRegion,
        _event: <wayland_client::protocol::wl_region::WlRegion as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
        // WlRegion has no events, so this body can remain empty.
    }
}
// =========================================================================
// Add the missing .draw() orchestration method to bridge render.rs
// =========================================================================
// Replace the `impl AppState` block inside src/main.rs with this:

fn main() {
    println!("[DEBUG] Starting dock...");
    let conn = Connection::connect_to_env().expect("Failed to connect to Wayland display");
    println!("[DEBUG] Connected to Wayland.");
    
    let (globals, mut event_queue) = registry_queue_init::<AppState>(&conn).unwrap();
    let qh = event_queue.handle();
    
    let registry_state = RegistryState::new(&globals);
    let compositor_state = CompositorState::bind(&globals, &qh).expect("Failed to bind compositor");
    let output_state = OutputState::new(&globals, &qh);
    let layer_shell = LayerShell::bind(&globals, &qh).expect("wlr_layer_shell required");
    let shm_state = Shm::bind(&globals, &qh).expect("wl_shm required");
    let pool = SlotPool::new(1024 * 1024 * 4, &shm_state).expect("Failed to create memory pool");

    let seat_state = SeatState::new(&globals, &qh);
    
	let font_bytes = std::fs::read("font.ttf")
        .or_else(|_| std::fs::read("/usr/share/dockman/font.ttf"))
        .unwrap_or_else(|_| vec![0; 100]);

    // =========================================================================
    // 1. CREATE THE VARIABLES RIGHT BEFORE APPSTATE USES THEM
    // =========================================================================
    let pinned_vector = persistence::load_pinned_apps();
    let mut permanent_icon_cache = HashMap::new();
    for app_id in &pinned_vector {
        if let Some((rgba, size)) = cache::load_cached_icon(app_id) {
            permanent_icon_cache.insert(app_id.clone(), (rgba, size));
        }
    }

    // =========================================================================
    // 2. INITIALIZE THE FULL STATE MATCHING YOUR COMPOSITOR STRUCT
    // =========================================================================
    let mut state = AppState {
        connection: conn.clone(),
        registry_state,
        compositor_state,
        output_state,
        layer_shell,
        shm_state,
        pool,
        seat_state,
        layer_surface: None,
        current_buffer: None,
        width: 100, 
        height: 60,
        toplevel_manager: None, // This gets bound right below this block
        font_manager: FontManager::new(&font_bytes),
        wl_seat: None,
        wl_pointer: None,
        pointer_x: 0,
        pointer_y: 0,
        open_windows: HashMap::new(),
        pinned_apps: pinned_vector.into_iter().collect(), // Looked up safely now!
        icon_cache: permanent_icon_cache,                 // Found in scope now!
        menu_state: MenuState {
            x: 0,
            y: 0,
            target_window: None,
            target_app_id: None,
            is_open: false,
        },
        hover_state: HoverState {
            x: 0,
            app_id: None,
            is_visible: false,
            last_leave_time: None,
        },
        last_interact_time: std::time::Instant::now(),
        needs_redraw: false,
        last_mouse_pos: None,
    };

    // =========================================================================
    // Your existing foreign_toplevel manager binding continues right below here
    // =========================================================================
    state.toplevel_manager = state.registry_state
        .bind_one::<ZwlrForeignToplevelManagerV1, _, _>(&qh, 1..=3, ())
        .ok();
    

    println!("[DEBUG] Doing roundtrip...");
    event_queue.roundtrip(&mut state).unwrap();
    println!("[DEBUG] Roundtrip complete.");

    // Simple Verification Check
    if state.toplevel_manager.is_some() {
        println!("[DEBUG] Active window tracking protocols linked to event loop successfully via direct binding!");
    } else {
        eprintln!("[ERROR] Your compositor does not support zwlr_foreign_toplevel_manager_v1!");
    }
    
    // ... Rest of your layer_surface allocation and blocking_dispatch loop code continues exactly the same

    let raw_surface = state.compositor_state.create_surface(&qh);
	let layer_surface = state.layer_shell.create_layer_surface(
	    &qh,
	    raw_surface,
	    Layer::Top,
	    Some("dock_panel"),
	    None,
	);

	// ADD THIS LINE: This tells the compositor not to focus the dock on click
	layer_surface.set_keyboard_interactivity(
	    smithay_client_toolkit::shell::wlr_layer::KeyboardInteractivity::None
	);
	
    layer_surface.set_size(540, 60);
    layer_surface.set_anchor(Anchor::BOTTOM);
    layer_surface.wl_surface().commit(); 
    state.layer_surface = Some(layer_surface);

    println!("[DEBUG] Starting event loop...");
    loop {
        event_queue.blocking_dispatch(&mut state).unwrap();
    }
}
