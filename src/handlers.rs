use std::collections::HashMap;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat,
    delegate_shm,
    registry::{ProvidesRegistryState, RegistryHandler, RegistryState},
    shell::{
        wlr_layer::{Anchor, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
    },
    output::{OutputHandler, OutputState},
    shm::{Shm, ShmHandler},
    seat::{SeatHandler, SeatState},
};
use wayland_client::{
    protocol::{
        wl_output::{Transform, WlOutput},
        wl_seat::WlSeat,
    },
    Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};

use crate::AppState;
use dockman_lib::models::WindowDiagnostics; // FIXED: Point straight to dockman_lib model paths

// Quick custom state flag parser to remove the broken crate::modules::window_manager dependency
fn parse_window_states(state_bytes: &[u8]) -> (bool, bool) {
    let mut activated = false;
    let mut minimized = false;
    for chunk in state_bytes.chunks_exact(4) {
        if let Ok(bytes) = chunk.try_into() {
            let value = u32::from_ne_bytes(bytes);
            if value == 3 { activated = true; } // ZWLR_FOREIGN_TOPLEVEL_HANDLE_V1_STATE_ACTIVATED
            if value == 4 { minimized = true; } // ZWLR_FOREIGN_TOPLEVEL_HANDLE_V1_STATE_MINIMIZED
        }
    }
    (activated, minimized)
}

// =========================================================================
// Registry Handler to Bind Globals
// =========================================================================
impl RegistryHandler<AppState> for AppState {
    fn new_global(
        data: &mut AppState,
        _conn: &Connection,
        qh: &QueueHandle<AppState>,
        _name: u32, // Prefixed with underscore to fix unused variable warning
        interface: &str,
        version: u32,
    ) {
        eprintln!("[DEBUG] Global detected: {} (v{})", interface, version);       
    }   
    fn remove_global(_data: &mut AppState, _conn: &Connection, _qh: &QueueHandle<AppState>, _name: u32, _interface: &str) {}
}

// =========================================================================
// Existing SCTK Registry Glue Implementations
// =========================================================================
impl ProvidesRegistryState for AppState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    smithay_client_toolkit::registry_handlers!(OutputState, SeatState);
}

impl CompositorHandler for AppState {
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wayland_client::protocol::wl_surface::WlSurface, _: &WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wayland_client::protocol::wl_surface::WlSurface, _: &WlOutput) {}
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wayland_client::protocol::wl_surface::WlSurface, _: i32) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wayland_client::protocol::wl_surface::WlSurface, _: u32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wayland_client::protocol::wl_surface::WlSurface, _: Transform) {}
}

impl LayerShellHandler for AppState {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {}
    
    fn configure(&mut self, _: &Connection, qh: &QueueHandle<Self>, layer: &LayerSurface, configure: LayerSurfaceConfigure, _: u32) {
        self.width = configure.new_size.0.max(100);
        self.height = 60;
        
        layer.set_size(self.width, self.height);
        layer.set_anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        
        self.draw(qh); 
    }
}

impl ShmHandler for AppState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl OutputHandler for AppState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}
    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}
    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}
}

impl SeatHandler for AppState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: WlSeat) {}
    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: WlSeat) {}
    fn new_capability(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: WlSeat, _cap: smithay_client_toolkit::seat::Capability) {}
    fn remove_capability(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: WlSeat, _cap: smithay_client_toolkit::seat::Capability) {}
}

delegate_registry!(AppState);
delegate_compositor!(AppState);
delegate_output!(AppState);
delegate_layer!(AppState);
delegate_shm!(AppState);
delegate_seat!(AppState);


// =========================================================================
// Foreign Toplevel Event Delegates
// =========================================================================
// A. Update the Manager Dispatch block header back to ():
impl wayland_client::Dispatch<ZwlrForeignToplevelManagerV1, ()> for AppState {
    
    // =========================================================================
    // CRITICAL FIX: Intercept opcode 0 and map spawned handle child types!
    // =========================================================================
    wayland_client::event_created_child!(AppState, ZwlrForeignToplevelManagerV1, [
        0 => (ZwlrForeignToplevelHandleV1, ()) // Opcode 0 spawns handles with () user data
    ]);

    fn event(
        state: &mut Self,
        _proxy: &ZwlrForeignToplevelManagerV1,
        event: <ZwlrForeignToplevelManagerV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qh: &wayland_client::QueueHandle<Self>,
    ) {
        eprintln!("[DEBUG] Toplevel Manager event received");
        if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel: handle } = event {
            eprintln!("[DEBUG] Toplevel detected: {:?}", handle);
            state.open_windows.entry(handle.clone()).or_default();
            state.draw(_qh);
        }
    }
}

// B. Update the Handle Dispatch block header back to ():
impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for AppState {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: <ZwlrForeignToplevelHandleV1 as Proxy>::Event,
        _data: &(), // Changed back to ()
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        println!("[WAYLAND TRACKER EVENT ARRIVED] Got event: {:?}", event);

        state.open_windows.entry(handle.clone()).or_default();

        match event {
            zwlr_foreign_toplevel_handle_v1::Event::Closed => {
                println!("[WAYLAND DETECTOR] Window Closed: {:?}", handle);
                state.open_windows.remove(handle);
                state.draw(qh);
            }
            zwlr_foreign_toplevel_handle_v1::Event::Done => {
                state.draw(qh);
            }
            zwlr_foreign_toplevel_handle_v1::Event::Title { title } => {
                println!("[WAYLAND DETECTOR] Title changed: {}", title);
                if let Some(window) = state.open_windows.get_mut(handle) {
                    window.title = title;
                }
                state.draw(qh);
            }
            zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                println!("[WAYLAND DETECTOR] AppId registered string payload: '{}'", app_id);
                if let Some(window) = state.open_windows.get_mut(handle) {
                    window.app_id = app_id.clone();
                    window.app_name = app_id.clone();
                    
                    let cleaned_app_id = app_id.trim();
                    if !cleaned_app_id.is_empty() {
                        let icon_name = dockman_lib::icon_utils::extract_icon_name_from_desktop_file(cleaned_app_id);
                        let icon_path = dockman_lib::icon_utils::locate_actual_icon_path(&icon_name, cleaned_app_id);

                        let mut raw_pixels = None;
                        let target_size = 48;

                        if let Some(path) = icon_path {
                            if let Some((_, _, rgba_data)) = dockman_lib::terminal_graphics::load_image_raw_rgba(&path, target_size) {
                                raw_pixels = Some(rgba_data);
                            }
                        }

                        window.icon_name = icon_name;
                        window.icon_rgba = raw_pixels;
                        window.icon_size = target_size;
                    }
                }
                state.draw(qh);
            }
            zwlr_foreign_toplevel_handle_v1::Event::State { state: state_bytes } => {
                let (activated, minimized) = parse_window_states(&state_bytes);
                if let Some(window) = state.open_windows.get_mut(handle) {
                    window.is_activated = activated;
                    window.is_minimized = minimized;
                }
                state.draw(qh);
            }
            _ => {}
        }
    }
}

