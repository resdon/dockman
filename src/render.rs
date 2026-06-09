use std::collections::{HashMap, HashSet};
use crate::lib::models::WindowDiagnostics;
use smithay_client_toolkit::reexports::protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;

use crate::MenuState;

// Standalone function cleanly exposed to your crate root
pub fn render_windows(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    open_windows: &HashMap<wayland_client::backend::ObjectId, WindowDiagnostics>,
    pinned_apps: &HashSet<String>,
    icon_cache: &HashMap<String, (Vec<u8>, u32)>,
    menu_state: &MenuState,
) {
    // 1. Clear background
    let dock_height = 60;
    for y in 0..height as usize {
        for x in 0..width as usize {
            let canvas_idx = (y * (width as usize) + x) * 4;
            if y >= (height as usize - dock_height) {
                canvas[canvas_idx] = 0x11; canvas[canvas_idx + 1] = 0x11; canvas[canvas_idx + 2] = 0x11; canvas[canvas_idx + 3] = 0xFF;
            } else {
                canvas[canvas_idx] = 0x00; canvas[canvas_idx + 1] = 0x00; canvas[canvas_idx + 2] = 0x00; canvas[canvas_idx + 3] = 0x00;
            }
        }
    }

    // 2. Prepare items for rendering
    let mut running_app_ids = HashSet::new();
    let mut sorted_windows: Vec<&WindowDiagnostics> = open_windows.values().collect();
    sorted_windows.sort_by_key(|w| &w.app_name);
    for w in &sorted_windows { running_app_ids.insert(w.app_id.clone()); }

    // Collect pinned apps that are NOT running
    let mut pinned_not_running: Vec<String> = pinned_apps.iter()
        .filter(|id| !running_app_ids.contains(*id))
        .cloned()
        .collect();
    pinned_not_running.sort();

    // Layout configuration
    let box_size: usize = 48; 
    let spacing: usize = 12;
    let total_items = sorted_windows.len() + pinned_not_running.len();
    let content_width = if total_items > 0 { total_items * box_size + (total_items + 1) * spacing } else { 0 };
    let start_offset_x = if (width as usize) > content_width { (width as usize - content_width) / 2 } else { 0 };
    let start_y: usize = (height as usize - dock_height) + (dock_height - box_size) / 2;

    // Render Pinned-but-not-running first, or interleave? Let's just do all together.
    // We'll create a unified list of things to draw.
    struct DrawItem<'a> {
        app_id: String,
        icon: Option<(&'a [u8], u32)>,
        is_activated: bool,
        is_running: bool,
    }

    let mut items_to_draw = Vec::new();
    // Start with pinned not running
    for app_id in pinned_not_running {
        items_to_draw.push(DrawItem {
            app_id: app_id.clone(),
            icon: icon_cache.get(&app_id).map(|(v, s)| (v.as_slice(), *s)),
            is_activated: false,
            is_running: false,
        });
    }
    // Then running windows
    for w in sorted_windows {
        items_to_draw.push(DrawItem {
            app_id: w.app_id.clone(),
            icon: w.icon_rgba.as_ref().map(|v| (v.as_slice(), w.icon_size)),
            is_activated: w.is_activated,
            is_running: true,
        });
    }

    for (index, item) in items_to_draw.iter().enumerate() {
        let start_x = start_offset_x + spacing + index * (box_size + spacing);
        if start_x + box_size > width as usize { break; }

        if let Some((icon_pixels, img_size_u32)) = item.icon {
            let img_size = img_size_u32 as usize;
            for y in 0..box_size {
                for x in 0..box_size {
                    let canvas_x = start_x + x;
                    let canvas_y = start_y + y;
                    let src_x = (x * img_size) / box_size;
                    let src_y = (y * img_size) / box_size;
                    let src_idx = (src_y * img_size + src_x) * 4;

                    if src_idx + 3 < icon_pixels.len() && canvas_x < width as usize && canvas_y < height as usize {
                        let canvas_idx = (canvas_y * (width as usize) + canvas_x) * 4;
                        let alpha = icon_pixels[src_idx + 3] as f32 / 255.0;
                        if alpha > 0.0 {
                            // If not running, make it slightly desaturated or dimmed? 
                            // Let's just dim it a bit.
                            let dim = if item.is_running { 1.0 } else { 0.5 };
                            canvas[canvas_idx] = ((icon_pixels[src_idx] as f32 * alpha * dim) + (canvas[canvas_idx] as f32 * (1.0 - alpha))) as u8;
                            canvas[canvas_idx + 1] = ((icon_pixels[src_idx + 1] as f32 * alpha * dim) + (canvas[canvas_idx + 1] as f32 * (1.0 - alpha))) as u8;
                            canvas[canvas_idx + 2] = ((icon_pixels[src_idx + 2] as f32 * alpha * dim) + (canvas[canvas_idx + 2] as f32 * (1.0 - alpha))) as u8;
                            canvas[canvas_idx + 3] = 255;
                        }
                    }
                }
            }
        } else {
            // Placeholder for missing icon
            for y in 0..box_size {
                for x in 0..box_size {
                    let canvas_x = start_x + x;
                    let canvas_y = start_y + y;
                    if canvas_x < width as usize && canvas_y < height as usize {
                        let canvas_idx = (canvas_y * (width as usize) + canvas_x) * 4;
                        canvas[canvas_idx] = 0x55; canvas[canvas_idx + 1] = 0x55; canvas[canvas_idx + 2] = 0x55; canvas[canvas_idx + 3] = 0xFF;
                    }
                }
            }
        }

        // 3. Render tracking indicator dashes
        if item.is_running {
            let indicator_y = start_y + box_size + 4;
            if indicator_y < height as usize {
                let indicator_width = if item.is_activated { 32 } else { 8 };
                let indicator_offset = (box_size - indicator_width) / 2;
                for x in 0..indicator_width {
                    let canvas_x = start_x + indicator_offset + x;
                    if canvas_x < width as usize {
                        let canvas_idx = (indicator_y * (width as usize) + canvas_x) * 4;
                        let brightness = if item.is_activated { 0xFF } else { 0x66 };
                        canvas[canvas_idx] = brightness; canvas[canvas_idx + 1] = brightness; canvas[canvas_idx + 2] = brightness; canvas[canvas_idx + 3] = 0xFF;
                    }
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
