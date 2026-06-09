use std::collections::{HashMap, HashSet};
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
            self.last_interact_time = std::time::Instant::now(); // Update interaction time
            self.pointer_x = event.position.0 as usize;
            self.pointer_y = event.position.1 as usize;

            let box_size = 48;
            let spacing = 12;
            let dock_height = 60;

            // Recalculate layout (must match render.rs)
            let mut apps_in_dock = Vec::new();
            let mut running_by_app: HashMap<String, Vec<ObjectId>> = HashMap::new();
            for app_id in &self.pinned_apps { apps_in_dock.push(app_id.clone()); }
            let mut sorted_windows: Vec<(&ObjectId, &WindowDiagnostics)> = self.open_windows.iter().collect();
            sorted_windows.sort_by(|a, b| a.1.app_name.cmp(&b.1.app_name));
            for (id, win) in &sorted_windows {
                running_by_app.entry(win.app_id.clone()).or_insert_with(Vec::new).push((*id).clone());
                if !apps_in_dock.contains(&win.app_id) { apps_in_dock.push(win.app_id.clone()); }
            }
            let total_items = apps_in_dock.len();
            let content_width = if total_items > 0 { total_items * box_size + (total_items + 1) * spacing } else { 0 };
            let start_offset_x = if (self.width as usize) > content_width { (self.width as usize - content_width) / 2 } else { 0 };

            if let PointerEventKind::Motion { .. } = event.kind {
                let mut found_hover = false;
                if self.pointer_y >= (self.height as usize - dock_height) {
                    for (index, app_id) in apps_in_dock.iter().enumerate() {
                        let start_x = start_offset_x + spacing + index * (box_size + spacing);
                        let end_x = start_x + box_size;
                        if self.pointer_x >= start_x && self.pointer_x <= end_x {
                            self.hover_state.is_visible = true;
                            self.hover_state.x = start_x + box_size / 2;
                            self.hover_state.app_id = Some(app_id.clone());
                            found_hover = true;
                            break;
                        }
                    }
                }
                if !found_hover && self.hover_state.is_visible {
                    // Check if we are hovering over the hover menu itself
                    let menu_width = 200;
                    let item_h = 30;
                    let windows_count = self.hover_state.app_id.as_ref().and_then(|id| running_by_app.get(id)).map(|v| v.len()).unwrap_or(0);
                    let menu_height = windows_count * item_h;
                    let menu_x = self.hover_state.x.saturating_sub(menu_width / 2).min(self.width as usize - menu_width);
                    let menu_y = (self.height as usize - dock_height).saturating_sub(menu_height + 10);

                    // STRICTER detection: Only close if mouse is REALLY outside
                    let is_inside_menu = self.pointer_x >= menu_x && self.pointer_x <= menu_x + menu_width &&
                                         self.pointer_y >= menu_y && self.pointer_y <= menu_y + menu_height;

                    if !is_inside_menu {
                        let now = std::time::Instant::now();
                        if let Some(leave_time) = self.hover_state.last_leave_time {
                            if now.duration_since(leave_time) < std::time::Duration::from_millis(500) {
                                return; // Grace period
                            }
                        } else {
                            self.hover_state.last_leave_time = Some(now);
                            return; // Start grace period
                        }
                        
                        self.hover_state.is_visible = false;
                        self.hover_state.app_id = None;
                        self.hover_state.last_leave_time = None;
                        self.draw(qh);
                    } else {
                        self.hover_state.last_leave_time = None; // Reset if inside
                    }
                }
                self.draw(qh);
            }

            if let PointerEventKind::Press { button, .. } = event.kind {
                if button == 272 { // BTN_LEFT
                    // 1. Check if we clicked on the context menu
                    if self.menu_state.is_open {
                        let menu_width = 120;
                        let menu_height = 90;
                        let menu_x = self.menu_state.x.min(self.width as usize - menu_width);
                        let menu_y = self.menu_state.y.saturating_sub(menu_height);

                        if self.pointer_x >= menu_x && self.pointer_x <= menu_x + menu_width &&
                           self.pointer_y >= menu_y && self.pointer_y <= menu_y + menu_height {

                            let clicked_item = (self.pointer_y - menu_y) / 30; 
                            println!("[MENU] Clicked item index: {}", clicked_item);

                            if let Some(app_id) = self.menu_state.target_app_id.clone() {
                                match clicked_item {
                                    0 => { // Focus
                                        println!("[MENU] Action: Focus {}", app_id);
                                        if let Some(handle_id) = &self.menu_state.target_window {
                                            if let Some(window_info) = self.open_windows.get_mut(handle_id) {
                                                if let Some(seat) = &self.wl_seat { window_info.handle.activate(seat); }
                                            }
                                        }
                                    },
                                    1 => { // Minimize
                                        println!("[MENU] Action: Minimize {}", app_id);
                                        if let Some(handle_id) = &self.menu_state.target_window {
                                            if let Some(window_info) = self.open_windows.get_mut(handle_id) { window_info.handle.set_minimized(); }
                                        }
                                    },
                                    2 => { // Open new
                                        println!("[MENU] Action: Open new {}", app_id);
                                        let launcher_path = std::path::Path::new("/usr/share/dockman/launcher.sh");
                                        let path_str = if launcher_path.exists() {
                                            "/usr/share/dockman/launcher.sh".to_string()
                                        } else {
                                            "./launcher.sh".to_string()
                                        };
                                        println!("[LAUNCHER] Spawning {} via {}", app_id, path_str);
                                        std::process::Command::new("sh")
                                            .arg(path_str)
                                            .arg(&app_id)
                                            .spawn()
                                            .expect("Failed to spawn launcher script");
                                    },
                                    3 => { // Close
                                        println!("[MENU] Action: Close {}", app_id);
                                        if let Some(handle_id) = &self.menu_state.target_window {
                                            if let Some(window_info) = self.open_windows.get_mut(handle_id) { window_info.handle.close(); }
                                        }
                                    },
                                    4 => { // Pin/Unpin
                                        println!("[MENU] Action: Pin/Unpin {}", app_id);
                                        let mut pinned = crate::modules::persistence::load_pinned_apps();
                                        if pinned.contains(&app_id) {
                                            pinned.remove(&app_id);
                                        } else {
                                            pinned.insert(app_id.clone());
                                            if let Some(handle_id) = &self.menu_state.target_window {
                                                if let Some(win) = self.open_windows.get(handle_id) {
                                                    if let Some(ref rgba) = win.icon_rgba { self.icon_cache.insert(app_id.clone(), (rgba.clone(), win.icon_size)); }
                                                }
                                            }
                                        }
                                        crate::modules::persistence::save_pinned_apps(&pinned);
                                        self.pinned_apps = pinned.into_iter().collect();
                                    },
                                    _ => {}
                                }
                            }
                            self.menu_state.is_open = false;
                            self.draw(qh);
                            return;
                        } else {
                            self.menu_state.is_open = false;
                            self.draw(qh);
                        }
                    }

                    // 2. Check if we clicked on the hover menu
                    if self.hover_state.is_visible {
                        let menu_width = 200;
                        let item_h = 30;
                        if let Some(ref app_id) = self.hover_state.app_id {
                            if let Some(windows) = running_by_app.get(app_id) {
                                let menu_height = windows.len() * item_h;
                                let menu_x = self.hover_state.x.saturating_sub(menu_width / 2).min(self.width as usize - menu_width);
                                let menu_y = (self.height as usize - dock_height).saturating_sub(menu_height + 10);

                                if self.pointer_x >= menu_x && self.pointer_x <= menu_x + menu_width &&
                                   self.pointer_y >= menu_y && self.pointer_y <= menu_y + menu_height {
                                    let idx = (self.pointer_y - menu_y) / item_h;
                                    if let Some(handle_id) = windows.get(idx) {
                                        if let Some(win) = self.open_windows.get_mut(handle_id) {
                                            if let Some(seat) = &self.wl_seat { win.handle.activate(seat); }
                                        }
                                    }
                                    self.hover_state.is_visible = false;
                                    self.draw(qh);
                                    return;
                                }
                            }
                        }
                    }

                    // 3. Check icons
                    for (index, app_id) in apps_in_dock.iter().enumerate() {
                        let start_x = start_offset_x + spacing + index * (box_size + spacing);
                        let end_x = start_x + box_size;
                        if self.pointer_x >= start_x && self.pointer_x <= end_x {
                            if let Some(windows) = running_by_app.get(app_id) {
                                // Toggle focus/minimize for the LAST active or first window
                                if let Some(handle_id) = windows.first() {
                                    let was_active = self.open_windows.get(handle_id).map(|w| w.is_activated).unwrap_or(false);
                                    if let Some(win) = self.open_windows.get_mut(handle_id) {
                                        if was_active { win.handle.set_minimized(); }
                                        else { if let Some(seat) = &self.wl_seat { win.handle.activate(seat); } }
                                    }
                                }
                            } else {
                                // Launch
                                let launcher_path = if std::path::Path::new("./launcher.sh").exists() { "./launcher.sh".to_string() }
                                                    else { "/usr/share/dockman/launcher.sh".to_string() };
                                let _ = std::process::Command::new("sh").arg(launcher_path).arg(app_id).spawn();
                            }
                            self.connection.flush().expect("Flush failed");
                            break;
                        }
                    }
                } else if button == 273 { // BTN_RIGHT
                    for (index, app_id) in apps_in_dock.iter().enumerate() {
                        let start_x = start_offset_x + spacing + index * (box_size + spacing);
                        let end_x = start_x + box_size;
                        if self.pointer_x >= start_x && self.pointer_x <= end_x {
                            self.menu_state.is_open = true;
                            self.menu_state.x = self.pointer_x;
                            self.menu_state.y = self.pointer_y;
                            self.menu_state.target_app_id = Some(app_id.clone());
                            self.menu_state.target_window = running_by_app.get(app_id).and_then(|v| v.first().cloned());
                            self.draw(qh);
                            break;
                        }
                    }
                }
            }
        }
        
        // Auto-dismiss check (1 second)
        if (self.menu_state.is_open || self.hover_state.is_visible) {
             let now = std::time::Instant::now();
             // Dismiss if no interaction for 1s
             if self.last_interact_time.elapsed() > std::time::Duration::from_secs(1) {
                self.menu_state.is_open = false;
                self.hover_state.is_visible = false;
                self.draw(qh);
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
                                raw_pixels = Some(rgba_data.clone());
                                // Also populate icon_cache for pinning
                                state.icon_cache.insert(cleaned_app_id.to_string(), (rgba_data, target_size));
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
