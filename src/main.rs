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

use std::collections::HashMap;

// 1. Mount the files as local root modules
pub mod handlers;
pub mod render;

use handlers::*;

// 2. Mock FontManager structure to fix E0425 and E0433
pub struct FontManager;
impl FontManager {
    pub fn new(_bytes: &[u8]) -> Self { FontManager }
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
    pub open_windows: HashMap<ObjectId, WindowDiagnostics>,
}

// =========================================================================
// Add the missing .draw() orchestration method to bridge render.rs
// =========================================================================
// Replace the `impl AppState` block inside src/main.rs with this:
impl AppState {
    pub fn draw(&mut self, _qh: &wayland_client::QueueHandle<Self>) {
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
        render::render_windows(canvas, width, height, &self.open_windows);

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
    
    let font_bytes = std::fs::read("font.ttf").unwrap_or_else(|_| vec![0; 100]);

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
        open_windows: HashMap::new(),
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
