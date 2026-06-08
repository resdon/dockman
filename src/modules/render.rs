// src/modules/render.rs

use wayland_client::QueueHandle;
use wayland_client::protocol::wl_shm;
use smithay_client_toolkit::shell::WaylandSurface;
use std::collections::HashMap;
use dockman_lib::WindowDiagnostics;
use crate::AppState;

impl AppState {
    pub fn draw(&mut self, _qh: &QueueHandle<Self>) {
        // 1. Check if the layer surface is ready
        let layer_surface = match &self.layer_surface {
            Some(surface) => surface,
            None => {
                println!("[RENDER ERROR] Drawing bypassed: layer_surface is None");
                return;
            }
        };

        // Fallback dimensions if configure events haven't updated state yet
        let width = if self.width == 0 { 1200 } else { self.width };
        let height = if self.height == 0 { 60 } else { self.height };
        let stride = width * 4;

        println!("[RENDER] --- New Drawing Frame Started ---");
        println!("[RENDER] Dock Geometry: {}x{} (Stride: {})", width, height, stride);

        // 2. Request backbuffer canvas space from our memory pool
        let (buffer, canvas) = match self.pool.create_buffer(
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
        ) {
            Ok(res) => res,
            Err(e) => {
                eprintln!("[RENDER ERROR] Failed to allocate canvas buffer: {:?}", e);
                return;
            }
        };

        // 3. Clear background with solid charcoal black (#FF111111)
        for pixel in canvas.chunks_exact_mut(4) {
            pixel[0] = 0x11; // Blue
            pixel[1] = 0x11; // Green
            pixel[2] = 0x11; // Red
            pixel[3] = 0xFF; // Alpha (fully opaque visibility)
        }

        // 4. Render the layout elements cleanly
        Self::render_windows(canvas, width, height, &self.open_windows);
        
        // 5. Explicitly update the Wayland surface backbuffers
        layer_surface.wl_surface().attach(Some(buffer.wl_buffer()), 0, 0);
        layer_surface.wl_surface().damage_buffer(0, 0, width as i32, height as i32);
        layer_surface.wl_surface().commit();

        // Keep buffer allocation reference alive in global application scope
        self.current_buffer = Some(buffer);
        println!("[RENDER] --- Frame Committed to Wayland ---");
    }

	pub fn render_windows(
        canvas: &mut [u8],
        width: u32,
        height: u32,
        open_windows: &HashMap<
            smithay_client_toolkit::reexports::protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1,
            WindowDiagnostics
        >
    ) {
        // 1. Force clear background to charcoal dark gray (#FF111111)
        for pixel in canvas.chunks_exact_mut(4) {
            pixel[0] = 0x11; // Blue
            pixel[1] = 0x11; // Green
            pixel[2] = 0x11; // Red
            pixel[3] = 0xFF; // Alpha
        }
    
        // 2. FORCE PRINT THE MAP COUNT TO TERMINAL
        println!("[RENDER ENGINE] Active Tracking Map Count = {}", open_windows.len());
    
        // 3. FORCE DRAW ONE SINGLE BLUE BOX DIRECTLY IN THE CENTER OF THE PANEL
        let box_size: usize = 48;
        let start_x: usize = 20; // Hardcoded horizontal location offset 
        let start_y: usize = ((height as usize) - box_size) / 2; // Vertically centered
    
        for y in 0..box_size {
            for x in 0..box_size {
                let canvas_x = start_x + x;
                let canvas_y = start_y + y;
    
                if canvas_x < width as usize && canvas_y < height as usize {
                    let canvas_idx = (canvas_y * (width as usize) + canvas_x) * 4;
                    canvas[canvas_idx] = 0xFF;     // Blue channel full max
                    canvas[canvas_idx + 1] = 0x44; // Green
                    canvas[canvas_idx + 2] = 0x11; // Red
                    canvas[canvas_idx + 3] = 0xFF; // Alpha channel solid opaque
                }
            }
        }
    }
}
