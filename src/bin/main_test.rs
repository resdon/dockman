use wayland_client::globals::registry_queue_init;
use fontdue::{Font, FontSettings};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat,
    delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryHandler, RegistryState},
    seat::{SeatHandler, SeatState},
    shell::{
        wlr_layer::{
            Anchor, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{
        slot::{Buffer, SlotPool},
        Shm, ShmHandler,
    },
};
use std::collections::HashMap;
use wayland_client::{
    protocol::{
        wl_output::{Transform, WlOutput},
        wl_pointer::{self, WlPointer},
        wl_seat::WlSeat,
        wl_shm,
        wl_surface::WlSurface,
    },
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};

#[derive(Debug, Default, Clone)]
pub struct TrackedWindow {
    pub title: String,
    pub app_id: String,
    pub is_activated: bool,
    pub is_minimized: bool,
    pub x_start: usize,
    pub x_end: usize,
}

pub struct CachedGlyph {
    pub bitmap: Vec<u8>,
    pub metrics: fontdue::Metrics,
}

pub struct FontManager {
    font: Font,
    cache: HashMap<(char, u32), CachedGlyph>,
}

impl FontManager {
    pub fn new(font_data: &[u8]) -> Self {
        let font = Font::from_bytes(font_data, FontSettings::default()).expect("Invalid font data");
        Self {
            font,
            cache: HashMap::new(),
        }
    }

    pub fn get_glyph(&mut self, character: char, size: f32) -> &CachedGlyph {
        let key = (character, size as u32);
        self.cache.entry(key).or_insert_with(|| {
            let (metrics, bitmap) = self.font.rasterize(character, size);
            CachedGlyph { bitmap, metrics }
        })
    }
}

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
    pub open_windows: HashMap<ZwlrForeignToplevelHandleV1, TrackedWindow>,
    pub font_manager: FontManager,
    pub wl_seat: Option<WlSeat>,
    pub wl_pointer: Option<WlPointer>,
    pub pointer_x: usize,
    pub mock_windows: Vec<TrackedWindow>,
}

impl AppState {
    pub fn draw(&mut self, _qh: &QueueHandle<Self>) {
        let layer_surface = match &self.layer_surface {
            Some(surface) => surface,
            None => return,
        };
        let width = self.width as i32;
        let height = self.height as i32;
        let stride = width * 4;
        let (buffer, canvas) = self
            .pool
            .create_buffer(width, height, stride, wl_shm::Format::Xrgb8888)
            .expect("Failed to create shared memory drawing buffer");

        for pixel in canvas.chunks_exact_mut(4) {
            pixel[0] = 40;
            pixel[1] = 40;
            pixel[2] = 45;
            pixel[3] = 255;
        }

        let mut cursor_x = 20;
        let icon_size = 32;
        let start_y = (height - icon_size as i32) / 2;
        let operational_mode_mock = self.open_windows.is_empty();

        let display_list: Vec<(Option<ZwlrForeignToplevelHandleV1>, TrackedWindow)> = if operational_mode_mock {
            self.mock_windows.iter().map(|w| (None, w.clone())).collect()
        } else {
            self.open_windows
                .iter()
                .map(|(k, w)| (Some(k.clone()), w.clone()))
                .collect()
        };

        let mode_msg = if operational_mode_mock {
            "[DEMO BRIDGE ACTIVE - CLICK BOXES TO TEST]"
        } else {
            "[LIVE WAYLAND SYSTEM METRICS OVERLAY]"
        };

        Self::blit_text(&mut self.font_manager, canvas, width, height, mode_msg, 20, 20, 110);

        for (handle, mut window) in display_list.into_iter() {
            let item_x_start = cursor_x;
            for row in 0..icon_size {
                for col in 0..icon_size {
                    let target_x = cursor_x as i32 + col as i32;
                    let target_y = start_y + row as i32;
                    if target_x >= 0 && target_x < width && target_y >= 0 && target_y < height {
                        let idx = ((target_y * width) + target_x) as usize * 4;
                        if window.is_activated {
                            canvas[idx + 0] = 245;
                            canvas[idx + 1] = 160;
                            canvas[idx + 2] = 40;
                        } else {
                            canvas[idx + 0] = 180;
                            canvas[idx + 1] = 70;
                            canvas[idx + 2] = 10;
                        }
                    }
                }
            }
            cursor_x += icon_size as usize + 12;
            let text_color = if window.is_activated { 255 } else { 135 };
            let status_indicator = if window.is_minimized {
                "(Minimized)"
            } else if window.is_activated {
                "(Active)"
            } else {
                "(Idle)"
            };
            let label = format!("{}: {} {}", window.app_id, window.title, status_indicator);
            cursor_x = Self::blit_text(
                &mut self.font_manager,
                canvas,
                width,
                height,
                &label,
                cursor_x,
                38,
                text_color,
            );
            window.x_start = item_x_start;
            window.x_end = cursor_x;

            if let Some(real_handle) = handle {
                self.open_windows.insert(real_handle, window);
            } else if let Some(mock_item) = self
                .mock_windows
                .iter_mut()
                .find(|m| m.app_id == window.app_id)
            {
                *mock_item = window;
            }

            cursor_x += 40;
            if cursor_x >= width as usize {
                break;
            }
        }

        layer_surface
            .wl_surface()
            .attach(Some(buffer.wl_buffer()), 0, 0);
        layer_surface
            .wl_surface()
            .damage_buffer(0, 0, width, height);
        layer_surface.wl_surface().commit();
        self.current_buffer = Some(buffer);
    }

    fn blit_text(
        fm: &mut FontManager,
        canvas: &mut [u8],
        width: i32,
        height: i32,
        text: &str,
        mut cx: usize,
        by: i32,
        color: u8,
    ) -> usize {
        for c in text.chars() {
            let glyph = fm.get_glyph(c, 13.0);
            let advance = glyph.metrics.advance_width.round() as usize;
            if !glyph.bitmap.is_empty() {
                for row in 0..glyph.metrics.height {
                    for col in 0..glyph.metrics.width {
                        let op = glyph.bitmap[(row * glyph.metrics.width + col) as usize];
                        if op == 0 {
                            continue;
                        }
                        let tx = (cx as i32 + glyph.metrics.xmin + col as i32) as isize;
                        let ty = (by - glyph.metrics.ymin - row as i32) as isize;
                        if tx >= 0 && tx < width as isize && ty >= 0 && ty < height as isize {
                            let idx = (ty as usize * width as usize + tx as usize) * 4;
                            let a = op as f32 / 255.0;
                            canvas[idx + 0] =
                                ((color as f32 * a) + (canvas[idx + 0] as f32 * (1.0 - a))) as u8;
                            canvas[idx + 1] =
                                ((color as f32 * a) + (canvas[idx + 1] as f32 * (1.0 - a))) as u8;
                            canvas[idx + 2] =
                                ((color as f32 * a) + (canvas[idx + 2] as f32 * (1.0 - a))) as u8;
                        }
                    }
                }
            }
            cx += advance;
        }
        cx
    }
}

impl ProvidesRegistryState for AppState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    smithay_client_toolkit::registry_handlers!(OutputState, SeatState);
}

impl OutputHandler for AppState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
}

impl ShmHandler for AppState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl SeatHandler for AppState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _qh: &QueueHandle<Self>, seat: WlSeat) {
        if self.wl_seat.is_none() {
            self.wl_seat = Some(seat);
        }
    }
    fn new_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        _: smithay_client_toolkit::seat::Capability,
    ) {}
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        _: smithay_client_toolkit::seat::Capability,
    ) {}
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
}

impl Dispatch<WlPointer, ()> for AppState {
    fn event(
        state: &mut Self,
        _proxy: &WlPointer,
        event: <WlPointer as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Motion { surface_x, .. } => {
                state.pointer_x = surface_x as usize;
            }
            wl_pointer::Event::Button {
                button,
                state: btn_state,
                ..
            } => {
                if button == 0x110 && btn_state == WEnum::Value(wl_pointer::ButtonState::Pressed) {
                    let clicked_x = state.pointer_x;

                    if state.open_windows.is_empty() {
                        let mut found_index = None;
                        for (i, m_win) in state.mock_windows.iter().enumerate() {
                            if clicked_x >= m_win.x_start && clicked_x <= m_win.x_end {
                                found_index = Some(i);
                                break;
                            }
                        }

                        if let Some(idx) = found_index {
                            if state.mock_windows[idx].is_activated {
                                state.mock_windows[idx].is_activated = false;
                                state.mock_windows[idx].is_minimized = true;
                            } else {
                                for reset in &mut state.mock_windows {
                                    reset.is_activated = false;
                                }
                                state.mock_windows[idx].is_activated = true;
                                state.mock_windows[idx].is_minimized = false;
                            }
                        }
                        state.draw(qh);
                        return;
                    }

                    let mut target_handle: Option<ZwlrForeignToplevelHandleV1> = None;
                    let mut is_active = false;
                    for (handle, window) in &state.open_windows {
                        if clicked_x >= window.x_start && clicked_x <= window.x_end {
                            target_handle = Some(handle.clone());
                            is_active = window.is_activated;
                            break;
                        }
                    }
                    if let (Some(handle), Some(seat)) = (target_handle, &state.wl_seat) {
                        if is_active {
                            handle.set_minimized();
                        } else {
                            handle.activate(seat);
                        }
                        state.draw(qh);
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlSeat, ()> for AppState {
    fn event(
        state: &mut Self,
        seat: &WlSeat,
        event: <WlSeat as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wayland_client::protocol::wl_seat::Event::Capabilities { capabilities } = event {
            if let WEnum::Value(caps) = capabilities {
                if caps.contains(wayland_client::protocol::wl_seat::Capability::Pointer)
                    && state.wl_pointer.is_none()
                {
                    state.wl_pointer = Some(seat.get_pointer(qh, ()));
                }
            }
        }
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for AppState {
    fn event(
        state: &mut Self,
        _proxy: &ZwlrForeignToplevelManagerV1,
        event: <ZwlrForeignToplevelManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } = event {
            state.open_windows.insert(toplevel.clone(), TrackedWindow::default());
            state.draw(qh);
        }
    }
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for AppState {
    fn event(
        state: &mut Self,
        proxy: &ZwlrForeignToplevelHandleV1,
        event: <ZwlrForeignToplevelHandleV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let Some(window) = state.open_windows.get_mut(proxy) {
            match event {
                zwlr_foreign_toplevel_handle_v1::Event::Title { title } => window.title = title,
                zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => window.app_id = app_id,
                zwlr_foreign_toplevel_handle_v1::Event::State { state: state_bytes } => {
                    let mut activated = false;
                    let mut minimized = false;
                    for chunk in state_bytes.chunks_exact(4) {
                        if let Ok(arr) = chunk.try_into() {
                            let val = u32::from_ne_bytes(arr);
                            if let Ok(st) = zwlr_foreign_toplevel_handle_v1::State::try_from(val) {
                                match st {
                                    zwlr_foreign_toplevel_handle_v1::State::Activated => {
                                        activated = true
                                    }
                                    zwlr_foreign_toplevel_handle_v1::State::Minimized => {
                                        minimized = true
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    window.is_activated = activated;
                    window.is_minimized = minimized;
                }
                zwlr_foreign_toplevel_handle_v1::Event::Closed => {
                    state.open_windows.remove(proxy);
                    state.draw(qh);
                }
                zwlr_foreign_toplevel_handle_v1::Event::Done => {
                    state.draw(qh);
                }
                _ => {}
            }
        }
    }
}

impl RegistryHandler<AppState> for AppState {
    fn new_global(
        data: &mut Self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _name: u32,
        interface: &str,
        version: u32,
    ) {
        if interface == "zwlr_foreign_toplevel_manager_v1" {
            let manager = data
                .registry_state
                .bind_all::<ZwlrForeignToplevelManagerV1, _, _, _>(qh, version..=version, |_| ())
                .ok()
                .and_then(|vec| vec.into_iter().next());
            data.toplevel_manager = manager;
        }
    }

    fn remove_global(
        _data: &mut Self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _name: u32,
        _interface: &str,
    ) {}
}

impl CompositorHandler for AppState {
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: &WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: &WlOutput) {}
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: i32) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: u32) {}
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: Transform,
    ) {}
}

impl LayerShellHandler for AppState {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {}
    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        self.width = configure.new_size.0.max(100);
        self.height = 60;
        layer.set_size(self.width, self.height);
        layer.set_anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_layer(Layer::Top);
        layer.set_exclusive_zone(60);
        layer.wl_surface().commit();
        self.draw(qh);
    }
}

delegate_registry!(AppState);
delegate_compositor!(AppState);
delegate_output!(AppState);
delegate_layer!(AppState);
delegate_shm!(AppState);
delegate_seat!(AppState);

fn main() {
    let conn = Connection::connect_to_env().unwrap();
    
    // 1. Replace manual event queue and registry fetch with registry_queue_init
    let (globals, mut event_queue) = registry_queue_init::<AppState>(&conn).unwrap();
    let qh = event_queue.handle();
    
    // 2. Initialize RegistryState with `&globals`
    let registry_state = RegistryState::new(&globals);

    // 3. Bind all subsequent states using `&globals` (not `&registry_state` or `&wl_registry`)
    let compositor_state =
        CompositorState::bind(&globals, &qh).expect("Failed to bind compositor");
    let output_state = OutputState::new(&globals, &qh);
    let layer_shell = LayerShell::bind(&globals, &qh).expect("wlr_layer_shell required");
    let shm_state = Shm::bind(&globals, &qh).expect("wl_shm required");
    let pool =
        SlotPool::new(1024 * 1024 * 4, &shm_state).expect("Failed to create shared memory pool");
    let seat_state = SeatState::new(&globals, &qh);
    let font_bytes = std::fs::read("font.ttf")
        .expect("font.ttf was not found in root project folder");

    let mock_windows = vec![
        TrackedWindow {
            title: "Browser Window Instance".to_string(),
            app_id: "edge".to_string(),
            is_activated: true,
            ..Default::default()
        },
        TrackedWindow {
            title: "Primary Development Shell".to_string(),
            app_id: "alacritty".to_string(),
            is_activated: false,
            ..Default::default()
        },
        TrackedWindow {
            title: "System Directory Explorer".to_string(),
            app_id: "pcmanfm".to_string(),
            is_activated: false,
            ..Default::default()
        },
    ];

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
        width: 100,
        height: 60,
        toplevel_manager: None,
        open_windows: HashMap::new(),
        font_manager: FontManager::new(&font_bytes),
        wl_seat: None,
        wl_pointer: None,
        pointer_x: 0,
        mock_windows,
    };

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
    layer_surface.wl_surface().commit();
    state.layer_surface = Some(layer_surface);

    event_queue.dispatch_pending(&mut state).unwrap();

    loop {
        event_queue.blocking_dispatch(&mut state).unwrap();
    }
}
