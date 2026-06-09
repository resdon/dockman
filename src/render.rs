use std::collections::HashMap;
use crate::lib::models::WindowDiagnostics;
use smithay_client_toolkit::reexports::protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;

use crate::MenuState;

// Standalone function cleanly exposed to your crate root
pub fn render_windows(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    open_windows: &HashMap<wayland_client::backend::ObjectId, WindowDiagnostics>,
    menu_state: &MenuState,
) {
    // 1. Clear background to dark gray charcoal panel skin (#FF111111)
    // ONLY clear the dock area (bottom 60 pixels) if menu is closed, 
    // or the whole area if menu is open.
    let dock_height = 60;

    for y in 0..height as usize {
        for x in 0..width as usize {
            let canvas_idx = (y * (width as usize) + x) * 4;
            
            // Background for the dock area
            if y >= (height as usize - dock_height) {
                canvas[canvas_idx] = 0x11; // B
                canvas[canvas_idx + 1] = 0x11; // G
                canvas[canvas_idx + 2] = 0x11; // R
                canvas[canvas_idx + 3] = 0xFF; // A
            } else {
                // Transparent for the rest unless menu is open
                canvas[canvas_idx] = 0x00;
                canvas[canvas_idx + 1] = 0x00;
                canvas[canvas_idx + 2] = 0x00;
                canvas[canvas_idx + 3] = 0x00;
            }
        }
    }

    // Print active debugging metrics
    // println!("[RENDER ENGINE] Active Tracking Map Count = {}", open_windows.len());

    // 2. Fix the random layout shuffle: Sort open windows deterministically by app name
    let mut sorted_windows: Vec<&WindowDiagnostics> = open_windows.values().collect();
    sorted_windows.sort_by_key(|w| &w.app_name);

    // Layout configuration variables
    let box_size: usize = 48; 
    let spacing: usize = 12;
    
    // Calculate total width required for all icons and center them
    let total_windows = sorted_windows.len();
    let content_width = if total_windows > 0 {
        total_windows * box_size + (total_windows + 1) * spacing
    } else {
        0
    };
    
    let start_offset_x = if (width as usize) > content_width {
        (width as usize - content_width) / 2
    } else {
        0
    };

    let start_y: usize = (height as usize - dock_height) + (dock_height - box_size) / 2;

    for (index, window) in sorted_windows.iter().enumerate() {
        // Calculate centered horizontal screen offsets
        let start_x = start_offset_x + spacing + index * (box_size + spacing);
        
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
                            canvas[canvas_idx + 3] = 255;
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

        // 3. Render tracking indicator dashes
        let indicator_y = start_y + box_size + 4;
        if indicator_y < height as usize {
            // "Dashes" for all open windows, a longer "bar" for the active one.
            let indicator_width = if window.is_activated { 32 } else { 8 };
            let indicator_offset = (box_size - indicator_width) / 2;

            for x in 0..indicator_width {
                let canvas_x = start_x + indicator_offset + x;
                if canvas_x < width as usize {
                    let canvas_idx = (indicator_y * (width as usize) + canvas_x) * 4;
                    
                    // Dimmer dash for inactive windows
                    let brightness = if window.is_activated { 0xFF } else { 0x66 };
                    
                    canvas[canvas_idx] = brightness;     // B
                    canvas[canvas_idx + 1] = brightness; // G
                    canvas[canvas_idx + 2] = brightness; // R
                    canvas[canvas_idx + 3] = 0xFF;       // A
                }
            }
        }
    }

    // 4. Render Context Menu
    if menu_state.is_open {
        let menu_width = 120;
        let menu_height = 90;
        let menu_x = menu_state.x.min(width as usize - menu_width);
        let menu_y = menu_state.y.saturating_sub(menu_height);

        for y in 0..menu_height {
            for x in 0..menu_width {
                let canvas_x = menu_x + x;
                let canvas_y = menu_y + y;

                if canvas_x < width as usize && canvas_y < height as usize {
                    let canvas_idx = (canvas_y * (width as usize) + canvas_x) * 4;
                    canvas[canvas_idx] = 0x22;     // B
                    canvas[canvas_idx + 1] = 0x22; // G
                    canvas[canvas_idx + 2] = 0x22; // R
                    canvas[canvas_idx + 3] = 0xFF; // A
                }
            }
        }
        
        // Draw simple separator lines for the 3 items: Open, Close, Pin
        for i in 1..3 {
            let line_y = menu_y + i * (menu_height / 3);
            for x in 0..menu_width {
                let canvas_x = menu_x + x;
                if canvas_x < width as usize && line_y < height as usize {
                    let canvas_idx = (line_y * (width as usize) + canvas_x) * 4;
                    canvas[canvas_idx] = 0x44;
                    canvas[canvas_idx + 1] = 0x44;
                    canvas[canvas_idx + 2] = 0x44;
                }
            }
        }
    }
}
