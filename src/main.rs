pub mod lib;

use crate::lib::models::WindowDiagnostics;


use smithay_client_toolkit::registry::ProvidesRegistryState;
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
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1;
use wayland_client::backend::ObjectId;

use std::collections::{HashMap, HashSet};

// 1. Mount the files as local root modules
pub mod handlers;
pub mod render;

use handlers::*;

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
    pub needs_redraw: bool,
}

// =========================================================================
// Add the missing .draw() orchestration method to bridge render.rs
// =========================================================================
// Replace the `impl AppState` block inside src/main.rs with this:
impl AppState {
    pub fn draw(&mut self, _qh: &wayland_client::QueueHandle<Self>) {
        let box_size = 48;
        let spacing = 12;
        
        // Group running windows by app_id
        let mut apps_in_dock = Vec::new();
        let mut running_by_app: HashMap<String, Vec<ObjectId>> = HashMap::new();
        
        // Use pinned apps as the base order
        for app_id in &self.pinned_apps {
            apps_in_dock.push(app_id.clone());
        }
        
        for (id, win) in &self.open_windows {
            running_by_app.entry(win.app_id.clone()).or_insert_with(Vec::new).push(id.clone());
            if !apps_in_dock.contains(&win.app_id) {
                apps_in_dock.push(win.app_id.clone());
            }
        }
        
        let total_items = apps_in_dock.len();
        
        // Calculate required width based on icons, but maintain a minimum width
        let content_width = if total_items > 0 {
            (total_items * box_size + (total_items + 1) * spacing) as u32
        } else {
            100 // Minimal width for empty dock
        };

        // Increase height if menu or hover is open
        let target_height = if self.menu_state.is_open || self.hover_state.is_visible { 200 } else { 60 };

        // If the surface size needs to change, update it
        if content_width != self.width || target_height != self.height {
            self.width = content_width;
            self.height = target_height;
            if let Some(ref surface) = self.layer_surface {
                surface.set_size(self.width, self.height);
                surface.wl_surface().commit();
            }
        }

        let width = self.width;
        let height = self.height;

        // FIXED: Use the correct wayland_client path for Shm color configurations
        let (buffer, canvas) = self.pool
            .create_buffer(
                width as i32, 
                height as i32, 
                (width * 4) as i32, 
                wayland_client::protocol::wl_shm::Format::Argb8888
            )
            .expect("Failed to create layout backing memory buffer");

        // Execute your local render loop code!
        render::render_windows(
            canvas, width, height, 
            &self.open_windows, 
            &self.pinned_apps, 
            &self.icon_cache, 
            &self.menu_state,
            &self.hover_state,
            &self.font_manager
        );

        if let Some(ref surface) = self.layer_surface {
            buffer.attach_to(surface.wl_surface()).expect("Failed to blit surface memory buffer");
            surface.wl_surface().damage_buffer(0, 0, width as i32, height as i32);
            surface.wl_surface().commit();
        }

        self.current_buffer = Some(buffer);
    }
}

fn main() {
	let connection = Connection::connect_to_env().expect("Failed to connect");
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

    let mut state = AppState {
    	connection,
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
        toplevel_manager: None, // Inject the safely bound manager instance here!
        font_manager: FontManager::new(&font_bytes),
        wl_seat: None,
        wl_pointer: None,
        pointer_x: 0,
        pointer_y: 0,
        open_windows: HashMap::new(),
        pinned_apps: Vec::new(),
        icon_cache: HashMap::new(),
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
		needs_redraw: false,
    };

    // =========================================================================
    // FIX: Revert type back to () to fix mismatched types error.
    // FIX: Lift constraint to 1..=3 to solve the opcode 0 runtime panic.
    // =========================================================================
    state.toplevel_manager = state.registry_state
        .bind_one::<ZwlrForeignToplevelManagerV1, _, _>(&qh, 1..=3, ()) // Pass () here
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
    layer_surface.set_anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer_surface.wl_surface().commit(); 
    state.layer_surface = Some(layer_surface);

    println!("[DEBUG] Starting event loop...");
    loop {
        event_queue.blocking_dispatch(&mut state).unwrap();
    }
}
