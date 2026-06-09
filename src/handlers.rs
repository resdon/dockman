pub use crate::lib::models::LastState;
use crate::lib::models::WindowDiagnostics;
use crate::lib::icon_utils;
use crate::lib::terminal_graphics;

use smithay_client_toolkit::{
    compositor::{CompositorHandler},
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryHandler, RegistryState},
    seat::{
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::wlr_layer::{Anchor, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
    shm::{Shm, ShmHandler},
};
use wayland_client::event_created_child;
use std::sync::Arc;
use wayland_client::backend::ObjectData;
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
use wayland_client::backend::ObjectId;
use crate::AppState;

fn parse_window_states(state_bytes: &[u8]) -> (bool, bool) {
    let mut activated = false;
    let mut minimized = false;
    
    // The state event sends a list of u32s. 
    // Protocol zwlr_foreign_toplevel_handle_v1::State:
    // 0 = Maximize, 1 = Minimize, 2 = Activated, 3 = Fullscreen
    for chunk in state_bytes.chunks_exact(4) {
        let value = u32::from_ne_bytes(chunk.try_into().unwrap());
        match value {
            2 => activated = true, 
            1 => minimized = true, 
            _ => {} // Ignore Maximize (0) and Fullscreen (3) for now
        }
    }
    (activated, minimized)
}

// =========================================================================
// Registry Handler to Bind Globals
// =========================================================================
impl RegistryHandler<AppState> for AppState {
    fn new_global(
        _data: &mut AppState,
        _conn: &Connection,
        _qh: &QueueHandle<AppState>,
        _name: u32,
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

    fn new_capability(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, seat: WlSeat, cap: Capability) {
        if cap == Capability::Pointer {
            println!("[INPUT DETECTOR] Mouse Pointer Capability Registered!");

            let wl_pointer = self.seat_state
                .get_pointer(qh, &seat)
                .expect("Failed to secure pointer handle");

            self.wl_pointer = Some(wl_pointer);
            self.wl_seat = Some(seat);
        }
    }

    fn remove_capability(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: WlSeat, cap: Capability) {
        if cap == Capability::Pointer {
            println!("[INPUT DETECTOR] Mouse Pointer Capability Unplugged!");
            self.wl_pointer = None;
        }
    }
}

// =========================================================================
// Pointer Interaction Tracking Logic
// =========================================================================
// =========================================================================
// Corrected Pointer Interaction Tracking Logic
// =========================================================================
impl PointerHandler for AppState {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _pointer: &wayland_client::protocol::wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            self.pointer_x = event.position.0 as usize;
            self.pointer_y = event.position.1 as usize;

            if let PointerEventKind::Press { button, .. } = event.kind {
                let box_size = 48;
                let spacing = 12;

                if button == 272 { // BTN_LEFT
                    // 1. Check if we clicked on the context menu
                    if self.menu_state.is_open {
                        let menu_width = 120;
                        let menu_height = 90;
                        let menu_x = self.menu_state.x.min(self.width as usize - menu_width);
                        let menu_y = self.menu_state.y.saturating_sub(menu_height);

                        if self.pointer_x >= menu_x && self.pointer_x <= menu_x + menu_width &&
                           self.pointer_y >= menu_y && self.pointer_y <= menu_y + menu_height {
                            
                            let item_height = menu_height / 3;
                            let clicked_item = (self.pointer_y - menu_y) / item_height;

                            if let Some(handle_id) = self.menu_state.target_window.clone() {
                                if let Some(window_info) = self.open_windows.get_mut(&handle_id) {
                                    match clicked_item {
                                        0 => { // Open/Focus
                                            if let Some(seat) = &self.wl_seat {
                                                window_info.handle.activate(seat);
                                            }
                                        },
                                        1 => { // Close
                                            window_info.handle.close();
                                        },
                                        2 => { // Pin/Unpin (placeholder)
                                            println!("[MENU] Pin/Unpin toggled for {}", window_info.app_id);
                                        },
                                        _ => {}
                                    }
                                }
                            }
                            self.menu_state.is_open = false;
                            self.draw(qh);
                            return;
                        } else {
                            // Clicked outside menu, close it
                            self.menu_state.is_open = false;
                            self.draw(qh);
                            // Continue to check if we clicked an icon
                        }
                    }

                    let mut clicked_handle: Option<ObjectId> = None;
                    let mut sorted_windows: Vec<_> = self.open_windows.iter().collect();
                    sorted_windows.sort_by(|a, b| a.1.app_name.cmp(&b.1.app_name));
                    
                    let total_windows = sorted_windows.len();
                    let content_width = if total_windows > 0 {
                        total_windows * box_size + (total_windows + 1) * spacing
                    } else {
                        0
                    };
                    
                    let start_offset_x = if (self.width as usize) > content_width {
                        (self.width as usize - content_width) / 2
                    } else {
                        0
                    };

                    for (index, (handle, _)) in sorted_windows.iter().enumerate() {
                        let start_x = start_offset_x + spacing + index * (box_size + spacing);
                        let end_x = start_x + box_size;
                        if self.pointer_x >= start_x && self.pointer_x <= end_x {
                            // Dereference the shared reference to clone the ObjectId
                            clicked_handle = Some((*handle).clone());
                            break;
                        }
                    }
                    
                    // Bind the ID safely and perform the action
                    if let Some(handle_id) = clicked_handle {
                        // 1. Capture the current state of the clicked window
                        let was_active = self.open_windows.get(&handle_id)
                            .map(|w| w.is_activated)
                            .unwrap_or(false);

                        // 2. Retrieve the one that was clicked and toggle its state
                        if let Some(window_info) = self.open_windows.get_mut(&handle_id) {
                            // Mark as pending so we know an action is in flight
                            window_info.is_pending = true; 
                            
                            // Issue the protocol request based on PREVIOUS state
                            if was_active {
                                // Already active? Minimize it.
                                window_info.handle.set_minimized();
                            } else {
                                // Not active? Activate it.
                                if let Some(seat) = &self.wl_seat {
                                    window_info.handle.activate(seat);
                                }
                            }
                            
                            self.connection.flush().expect("Flush failed");
                        }
                    }
                } else if button == 273 { // BTN_RIGHT
                    let mut clicked_handle: Option<ObjectId> = None;
                    let mut sorted_windows: Vec<_> = self.open_windows.iter().collect();
                    sorted_windows.sort_by(|a, b| a.1.app_name.cmp(&b.1.app_name));
                    
                    let total_windows = sorted_windows.len();
                    let content_width = if total_windows > 0 {
                        total_windows * box_size + (total_windows + 1) * spacing
                    } else {
                        0
                    };
                    
                    let start_offset_x = if (self.width as usize) > content_width {
                        (self.width as usize - content_width) / 2
                    } else {
                        0
                    };

                    for (index, (handle, _)) in sorted_windows.iter().enumerate() {
                        let start_x = start_offset_x + spacing + index * (box_size + spacing);
                        let end_x = start_x + box_size;
                        if self.pointer_x >= start_x && self.pointer_x <= end_x {
                            clicked_handle = Some((*handle).clone());
                            break;
                        }
                    }

                    if let Some(handle_id) = clicked_handle {
                        self.menu_state.is_open = true;
                        self.menu_state.x = self.pointer_x;
                        self.menu_state.y = self.pointer_y;
                        self.menu_state.target_window = Some(handle_id);
                        self.draw(qh);
                    }
                }
            }
        }
    }
}

// =========================================================================
// Foreign Toplevel Event Handlers
// =========================================================================


impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for AppState {
    fn event(
        state: &mut Self,
        _proxy: &ZwlrForeignToplevelManagerV1,
        event: zwlr_foreign_toplevel_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } = event {
            state.open_windows.entry(toplevel.id()).or_insert_with(|| {
                WindowDiagnostics::new(toplevel.clone())
            });
            state.draw(qh);
        }
    }

	// Use the macro here instead of a manual function
    event_created_child!(AppState, ZwlrForeignToplevelManagerV1, [
        // You must find the opcode in the protocol documentation or by looking at the generated code
        // For ToplevelManager, the "toplevel" event is what creates the child.
        // It is typically index 0 in the protocol definition.
        0 => (ZwlrForeignToplevelHandleV1, ()),
    ]);
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
        println!("[WAYLAND TRACKER EVENT ARRIVED] Got event: {:?}", event);

        state.open_windows.entry(handle.id()).or_insert_with(|| {
            WindowDiagnostics::new(handle.clone())
        });

        match event {
            zwlr_foreign_toplevel_handle_v1::Event::Closed => {
                println!("[WAYLAND DETECTOR] Window Closed: {:?}", handle);
                state.open_windows.remove(&handle.id());
                state.draw(qh);
            }
            zwlr_foreign_toplevel_handle_v1::Event::Done => {
                state.draw(qh);
            }
            zwlr_foreign_toplevel_handle_v1::Event::Title { title } => {
                println!("[WAYLAND DETECTOR] Title changed: {}", title);
                if let Some(window) = state.open_windows.get_mut(&handle.id()) {
                    window.title = title;
                }
                state.draw(qh);
            }
            zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                println!("[WAYLAND DETECTOR] AppId registered string payload: '{}'", app_id);
                if let Some(window) = state.open_windows.get_mut(&handle.id()) {
                    window.app_id = app_id.clone();
                    window.app_name = app_id.clone();
                    let cleaned_app_id = app_id.trim();
                    if !cleaned_app_id.is_empty() {
						let icon_name = crate::lib::icon_utils::extract_icon_name_from_desktop_file(cleaned_app_id);
                        let icon_path = crate::lib::icon_utils::locate_actual_icon_path(&icon_name, cleaned_app_id);
                        let mut raw_pixels = None;
                        let target_size = 48;
                        if let Some(path) = icon_path {
                            if let Some((_, _, rgba_data)) = crate::lib::terminal_graphics::load_image_raw_rgba(&path, target_size) {
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
			    
			    if let Some(window) = state.open_windows.get_mut(&handle.id()) {
			        // The compositor is telling us the current reality. 
			        // Sync our local state to this reality.
			        window.is_activated = activated;
			        window.is_minimized = minimized;
			        
			        // Now that the reality matches our intent, clear pending
			        window.is_pending = false; 
			    }
			    state.draw(qh);
			}
            _ => {println!("[WAYLAND] Unmatched event received: {:?}", event);}
            
        }
    }
}

// =========================================================================
// Standard SCTK Macro Framework Delegates
// =========================================================================
delegate_registry!(AppState);
delegate_compositor!(AppState);
delegate_output!(AppState);
delegate_layer!(AppState);
delegate_shm!(AppState);
delegate_seat!(AppState);
delegate_pointer!(AppState);
