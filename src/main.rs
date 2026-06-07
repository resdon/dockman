use dockman_lib::WindowDiagnostics;
use dockman_lib::icon_utils::{extract_icon_name_from_desktop_file};
use dockman_lib::terminal_graphics::{generate_terminal_image_string};

use smithay_client_toolkit::registry::ProvidesRegistryState;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::{
    compositor::CompositorState,
    output::OutputState,
    registry::{RegistryState},
    seat::SeatState,
    shell::wlr_layer::{Anchor, Layer, LayerShell, LayerSurface},
    shm::slot::{Buffer, SlotPool},
    shm::Shm,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_shm, wl_seat::WlSeat, wl_pointer::WlPointer};
use wayland_client::{Connection, QueueHandle};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1;
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;

pub mod modules;

use modules::window_manager::TrackedWindow;
use modules::font::FontManager;
use std::collections::HashMap;

pub struct AppState {
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
    pub open_windows: HashMap<ZwlrForeignToplevelHandleV1, WindowDiagnostics>,
}

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
    
    let font_bytes = std::fs::read("font.ttf").unwrap_or_else(|_| vec![0; 100]);

    let mut state = AppState {
        registry_state,
        compositor_state,
        output_state,
        layer_shell,
        shm_state,
        pool,
        seat_state,
        layer_surface: None,
        current_buffer: None,
        width: 0, 
        height: 60,
        toplevel_manager: None,
        font_manager: FontManager::new(&font_bytes),
        wl_seat: None,
        wl_pointer: None,
        pointer_x: 0,
        open_windows: HashMap::new(),
    };

	// Inside src/main.rs (right after initializing your `state` variable)
    println!("[DEBUG] Doing roundtrip...");
    event_queue.roundtrip(&mut state).unwrap();
    println!("[DEBUG] Roundtrip complete.");

    // =========================================================================
    // CRITICAL FIX: Instantiate the Toplevel Manager event listener callback
    // =========================================================================
    if let Some(manager) = state.toplevel_manager.take() {
        // Re-bind the manager but with an active fallback closure attached to the QueueHandle (`qh`)
        let active_manager = state.registry_state
            .bind_one::<ZwlrForeignToplevelManagerV1, _, _>(&qh, 1..=1, ())
            .expect("Failed to initialize toplevel manager event stream");
        
        state.toplevel_manager = Some(active_manager);
        println!("[DEBUG] Active window tracking protocols linked to event loop.");
    } else {
        eprintln!("[ERROR] Your compositor does not support zwlr_foreign_toplevel_manager_v1!");
    }
    // =========================================================================

    let raw_surface = state.compositor_state.create_surface(&qh);
    let layer_surface = state.layer_shell.create_layer_surface(
        &qh,
        raw_surface,
        Layer::Top,
        Some("dock_panel"),
        None,
    );

    layer_surface.set_size(100, 60);
    layer_surface.set_anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer_surface.wl_surface().commit(); // Triggers the first configure layout callback safely
    state.layer_surface = Some(layer_surface);

    println!("[DEBUG] Starting event loop...");
    loop {
        event_queue.blocking_dispatch(&mut state).unwrap();
    }


}
