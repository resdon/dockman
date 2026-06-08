use std::collections::HashMap;
use crate::lib::models::WindowDiagnostics;
use smithay_client_toolkit::reexports::protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;

// Standalone function cleanly exposed to your crate root
pub fn render_windows(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    open_windows: &HashMap<wayland_client::backend::ObjectId, WindowDiagnostics>
) {
    // 1. Clear background to dark gray charcoal panel skin (#FF111111)
    for pixel in canvas.chunks_exact_mut(4) {
        pixel[0] = 0x11; // Blue
        pixel[1] = 0x11; // Green
        pixel[2] = 0x11; // Red
        pixel[3] = 0xFF; // Alpha
    }

    // Print active debugging metrics
    println!("[RENDER ENGINE] Active Tracking Map Count = {}", open_windows.len());

    // 2. Fix the random layout shuffle: Sort open windows deterministically by app name
    let mut sorted_windows: Vec<&WindowDiagnostics> = open_windows.values().collect();
    sorted_windows.sort_by_key(|w| &w.app_name);

    // Layout configuration variables
    let box_size: usize = 48; 
    let spacing: usize = 12;
    let start_y: usize = ((height as usize) - box_size) / 2; // Vertically center boxes

    for (index, window) in sorted_windows.iter().enumerate() {
        // Calculate bounded horizontal screen offsets
        let start_x = spacing + index * (box_size + spacing);
        
        // Prevent drawing off-screen horizontally
        if start_x + box_size > width as usize {
            break; 
        }

        // Check if our extraction pipeline successfully populated image pixel vectors
        if let Some(ref icon_pixels) = window.icon_rgba {
            let img_size = window.icon_size as usize;

            // Map and blit raw pixel data onto our allocated canvas memory slice
            for y in 0..box_size {
                for x in 0..box_size {
                    let canvas_x = start_x + x;
                    let canvas_y = start_y + y;

                    // Interpolate box dimensions straight down to texture arrays
                    let src_x = (x * img_size) / box_size;
                    let src_y = (y * img_size) / box_size;
                    let src_idx = (src_y * img_size + src_x) * 4;

                    if src_idx + 3 < icon_pixels.len() && canvas_x < width as usize && canvas_y < height as usize {
                        let canvas_idx = (canvas_y * (width as usize) + canvas_x) * 4;
                        
                        // Perform an alpha blend calculation over the existing pixel data
                        let alpha = icon_pixels[src_idx + 3] as f32 / 255.0;
                        if alpha > 0.0 {
                            canvas[canvas_idx] = ((icon_pixels[src_idx] as f32 * alpha) + (canvas[canvas_idx] as f32 * (1.0 - alpha))) as u8;
                            canvas[canvas_idx + 1] = ((icon_pixels[src_idx + 1] as f32 * alpha) + (canvas[canvas_idx + 1] as f32 * (1.0 - alpha))) as u8;
                            canvas[canvas_idx + 2] = ((icon_pixels[src_idx + 2] as f32 * alpha) + (canvas[canvas_idx + 2] as f32 * (1.0 - alpha))) as u8;
                        }
                    }
                }
            }
        } else {
            // FALLBACK: Draw your solid test blue block if real app id matching data hasn't finished loading yet
            for y in 0..box_size {
                for x in 0..box_size {
                    let canvas_x = start_x + x;
                    let canvas_y = start_y + y;

                    if canvas_x < width as usize && canvas_y < height as usize {
                        let canvas_idx = (canvas_y * (width as usize) + canvas_x) * 4;
                        canvas[canvas_idx] = 0xFF;     // Blue full
                        canvas[canvas_idx + 1] = 0x44; // Green
                        canvas[canvas_idx + 2] = 0x11; // Red
                        canvas[canvas_idx + 3] = 0xFF; // Alpha solid
                    }
                }
            }
        }

        // 3. Render active tracking indicator underline strip if window is currently active/focused
        if window.is_activated {
            let indicator_y = start_y + box_size + 4;
            if indicator_y < height as usize {
                for x in 0..box_size {
                    let canvas_x = start_x + x;
                    if canvas_x < width as usize {
                        let canvas_idx = (indicator_y * (width as usize) + canvas_x) * 4;
                        canvas[canvas_idx] = 0xFF;     // Accent focus line (white)
                        canvas[canvas_idx + 1] = 0xFF;
                        canvas[canvas_idx + 2] = 0xFF;
                        canvas[canvas_idx + 3] = 0xFF;
                    }
                }
            }
        }
    }
}
