// src/modules/render.rs

use wayland_client::QueueHandle;
use wayland_client::protocol::wl_shm;
use smithay_client_toolkit::shell::WaylandSurface;
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
        Self::render_windows(&self.open_windows, canvas, width, height);

        // 5. Explicitly update the Wayland surface backbuffers
        layer_surface.wl_surface().attach(Some(buffer.wl_buffer()), 0, 0);
        layer_surface.wl_surface().damage_buffer(0, 0, width as i32, height as i32);
        layer_surface.wl_surface().commit();

        // Keep buffer allocation reference alive in global application scope
        self.current_buffer = Some(buffer);
        println!("[RENDER] --- Frame Committed to Wayland ---");
    }

    fn render_windows(
        open_windows: &std::collections::HashMap<
            wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1,
            dockman_lib::WindowDiagnostics
        >,
        canvas: &mut [u8],
        width: u32,
        height: u32,
    ) {
        let box_size: usize = 32;
        let padding: usize = 16;
        
        let start_y = (height as usize - box_size) / 2; 
        let mut current_x = padding;

        println!("[RENDER MAP CHECK] Size of open_windows HashMap: {}", open_windows.len());

        for (handle, window) in open_windows {
            // Log exactly what window we found and its state variables
            println!(
                "[RENDER LOOP] Found Window -> Handle: {:?}, App Name: '{}', Title: '{}', Active: {}, Minimized: {}", 
                handle, window.app_name, window.title, window.is_activated, window.is_minimized
            );

            // Guardrail to prevent clipping past screen borders
            if current_x + box_size > width as usize {
                println!("[RENDER WARNING] Box skipped: X coordinate ({}) exceeds width limits", current_x + box_size);
                break;
            }

            println!("[RENDER DRAWING] Drawing blue box at X: {}, Y: {}", current_x, start_y);

            // === DRAW THE BLUE BOXES ===
            for y in 0..box_size {
                for x in 0..box_size {
                    let canvas_x = current_x + x;
                    let canvas_y = start_y + y;
                    let pixel_idx = (canvas_y * (width as usize) + canvas_x) * 4;

                    if pixel_idx + 3 < canvas.len() {
                        canvas[pixel_idx]     = 0xFF; // Solid Blue
                        canvas[pixel_idx + 1] = 0x44; // Green
                        canvas[pixel_idx + 2] = 0x11; // Red
                        canvas[pixel_idx + 3] = 0xFF; // Alpha
                    }
                }
            }

            // === DRAW DASH INDICATORS UNDERNEATH ACTIVE WINDOWS ===
            if window.is_activated {
                let dash_width = 16;
                let dash_height = 4;
                
                let dash_start_y = start_y + box_size + 3;
                let dash_start_x = current_x + ((box_size - dash_width) / 2);

                println!("[RENDER DRAWING] Window is active! Drawing white dash at X: {}, Y: {}", dash_start_x, dash_start_y);

                for dy in 0..dash_height {
                    for dx in 0..dash_width {
                        let canvas_x = dash_start_x + dx;
                        let canvas_y = dash_start_y + dy;
                        let pixel_idx = (canvas_y * (width as usize) + canvas_x) * 4;

                        if pixel_idx + 3 < canvas.len() {
                            canvas[pixel_idx]     = 0xFF; // Blue
                            canvas[pixel_idx + 1] = 0xFF; // Green
                            canvas[pixel_idx + 2] = 0xFF; // Red
                            canvas[pixel_idx + 3] = 0xFF; // Alpha
                        }
                    }
                }
            }

            // Step cursor forward for subsequent windows layout slotting
            current_x += box_size + padding;
        }
    }
}
