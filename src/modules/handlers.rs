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
use crate::modules::window_manager::{TrackedWindow, parse_window_states};
use crate::WindowDiagnostics;

// =========================================================================
// Registry Handler to Bind Globals
// =========================================================================
impl RegistryHandler<AppState> for AppState {
    fn new_global(
        data: &mut AppState,
        _conn: &Connection,
        qh: &QueueHandle<AppState>,
        name: u32,
        interface: &str,
        version: u32,
    ) {
        eprintln!("[DEBUG] Global detected: {} (v{})", interface, version);
        
        if interface == "zwlr_foreign_toplevel_manager_v1" {
            let manager = data.registry_state
                .bind_all::<ZwlrForeignToplevelManagerV1, _, _, _>(qh, version..=version, |_| ())
                .ok()
                .and_then(|vec| vec.into_iter().next())
                .expect("Failed to bind toplevel manager");
            data.toplevel_manager = Some(manager);
            eprintln!("[DEBUG] Successfully bound zwlr_foreign_toplevel_manager_v1");
        }
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

// Inside src/modules/handlers.rs
impl LayerShellHandler for AppState {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {}
    
    fn configure(&mut self, _: &Connection, qh: &QueueHandle<Self>, layer: &LayerSurface, configure: LayerSurfaceConfigure, _: u32) {
        // Update dimensions based on what the compositor told us
        self.width = configure.new_size.0.max(100);
        self.height = 60;
        
        layer.set_size(self.width, self.height);
        layer.set_anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        
        // This is where drawing safely happens!
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

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for AppState {
    fn event(
        state: &mut Self,
        _proxy: &ZwlrForeignToplevelManagerV1,
        event: <ZwlrForeignToplevelManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        eprintln!("[DEBUG] Toplevel Manager event received");
        if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel: handle } = event {
            eprintln!("[DEBUG] Toplevel detected: {:?}", handle);
            state.open_windows.entry(handle).or_insert_with(WindowDiagnostics::default);
            state.draw(_qh);
        }
    }
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for AppState {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: <ZwlrForeignToplevelHandleV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        // 1. Handle closing/done events first to avoid map borrow locks
        match &event {
            zwlr_foreign_toplevel_handle_v1::Event::Closed => {
                eprintln!("[DEBUG] Toplevel {:?} closed", handle);
                state.open_windows.remove(handle);
                state.draw(qh);
                return;
            }
            zwlr_foreign_toplevel_handle_v1::Event::Done => {
                state.draw(qh);
                return;
            }
            _ => {}
        }

        // 2. Safely mutate the individual window fields
        if let Some(window) = state.open_windows.get_mut(handle) {
            match event {
                zwlr_foreign_toplevel_handle_v1::Event::Title { title } => {
                    eprintln!("[DEBUG] Toplevel {:?} Title changed: {}", handle, title);
                    window.title = title;
                }
                zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                    eprintln!("[DEBUG] Toplevel {:?} AppID changed: {}", handle, app_id);
                    window.app_id = app_id;
                }
                zwlr_foreign_toplevel_handle_v1::Event::State { state: state_bytes } => {
                    let (activated, minimized) = parse_window_states(&state_bytes);
                    eprintln!("[DEBUG] Toplevel {:?} State: Activated={}, Minimized={}", handle, activated, minimized);
                    window.is_activated = activated;
                    window.is_minimized = minimized;
                }
                _ => {}
            }
        }
    }
}

