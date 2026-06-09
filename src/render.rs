use std::collections::{HashMap, HashSet};
use crate::lib::models::WindowDiagnostics;
use smithay_client_toolkit::reexports::protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;

use crate::{MenuState, HoverState, FontManager};

pub fn draw_text(
    canvas: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    font: &fontdue::Font,
    text: &str,
    size: f32,
    start_x: usize,
    start_y: usize,
    color: (u8, u8, u8),
) {
    let mut x_offset = start_x;
    for c in text.chars() {
        let (metrics, bitmap) = font.rasterize(c, size);
        for y in 0..metrics.height {
            for x in 0..metrics.width {
                let canvas_x = x_offset + x + metrics.xmin as usize;
                let canvas_y = start_y + y + (size as usize - metrics.height - metrics.ymin as usize);

                if canvas_x < canvas_width as usize && canvas_y < canvas_height as usize {
                    let canvas_idx = (canvas_y * canvas_width as usize + canvas_x) * 4;
                    let alpha = bitmap[y * metrics.width + x] as f32 / 255.0;
                    if alpha > 0.0 {
                        canvas[canvas_idx] = ((color.0 as f32 * alpha) + (canvas[canvas_idx] as f32 * (1.0 - alpha))) as u8;
                        canvas[canvas_idx + 1] = ((color.1 as f32 * alpha) + (canvas[canvas_idx + 1] as f32 * (1.0 - alpha))) as u8;
                        canvas[canvas_idx + 2] = ((color.2 as f32 * alpha) + (canvas[canvas_idx + 2] as f32 * (1.0 - alpha))) as u8;
                        canvas[canvas_idx + 3] = 255;
                    }
                }
            }
        }
        x_offset += metrics.advance_width as usize;
    }
}

// Standalone function cleanly exposed to your crate root
pub fn render_windows(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    open_windows: &HashMap<wayland_client::backend::ObjectId, WindowDiagnostics>,
    pinned_apps: &Vec<String>,
    icon_cache: &HashMap<String, (Vec<u8>, u32)>,
    menu_state: &MenuState,
    hover_state: &HoverState,
    font_manager: &FontManager,
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

    // 2. Prepare items for rendering with GROUPING
    let mut apps_in_dock = Vec::new();
    let mut running_by_app: HashMap<String, Vec<&WindowDiagnostics>> = HashMap::new();
    
    // Order from pinned apps first
    for app_id in pinned_apps {
        if !apps_in_dock.contains(app_id) {
            apps_in_dock.push(app_id.clone());
        }
    }

    // Then add running apps not in pinned
    let mut sorted_windows: Vec<&WindowDiagnostics> = open_windows.values().collect();
    sorted_windows.sort_by_key(|w| &w.app_name);
    
    for w in sorted_windows {
        running_by_app.entry(w.app_id.clone()).or_insert_with(Vec::new).push(w);
        if !apps_in_dock.contains(&w.app_id) {
            apps_in_dock.push(w.app_id.clone());
        }
    }

    // Layout configuration
    let box_size: usize = 48; 
    let spacing: usize = 12;
    let total_items = apps_in_dock.len();
    let content_width = if total_items > 0 { total_items * box_size + (total_items + 1) * spacing } else { 0 };
    let start_offset_x = if (width as usize) > content_width { (width as usize - content_width) / 2 } else { 0 };
    let start_y: usize = (height as usize - dock_height) + (dock_height - box_size) / 2;

    for (index, app_id) in apps_in_dock.iter().enumerate() {
        let start_x = start_offset_x + spacing + index * (box_size + spacing);
        if start_x + box_size > width as usize { break; }

        let windows = running_by_app.get(app_id);
        let is_running = windows.is_some();
        let is_activated = windows.map(|v| v.iter().any(|w| w.is_activated)).unwrap_or(false);

        // Get icon from cache or from first running window
        let icon = windows.and_then(|v| v.first().and_then(|w| w.icon_rgba.as_ref().map(|rgba| (rgba.as_slice(), w.icon_size))))
                    .or_else(|| icon_cache.get(app_id).map(|(v, s)| (v.as_slice(), *s)));

        if let Some((icon_pixels, img_size_u32)) = icon {
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
                            let dim = if is_running { 1.0 } else { 0.5 };
                            canvas[canvas_idx] = ((icon_pixels[src_idx] as f32 * alpha * dim) + (canvas[canvas_idx] as f32 * (1.0 - alpha))) as u8;
                            canvas[canvas_idx + 1] = ((icon_pixels[src_idx + 1] as f32 * alpha * dim) + (canvas[canvas_idx + 1] as f32 * (1.0 - alpha))) as u8;
                            canvas[canvas_idx + 2] = ((icon_pixels[src_idx + 2] as f32 * alpha * dim) + (canvas[canvas_idx + 2] as f32 * (1.0 - alpha))) as u8;
                            canvas[canvas_idx + 3] = 255;
                        }
                    }
                }
            }
        }

        // Render tracking indicator dash
        if is_running {
            let indicator_y = start_y + box_size + 4;
            if indicator_y < height as usize {
                let indicator_width = if is_activated { 32 } else { 8 };
                let indicator_offset = (box_size - indicator_width) / 2;
                for x in 0..indicator_width {
                    let canvas_x = start_x + indicator_offset + x;
                    if canvas_x < width as usize {
                        let canvas_idx = (indicator_y * (width as usize) + canvas_x) * 4;
                        let brightness = if is_activated { 0xFF } else { 0x66 };
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
                    canvas[canvas_idx] = 0x22; canvas[canvas_idx + 1] = 0x22; canvas[canvas_idx + 2] = 0x22; canvas[canvas_idx + 3] = 0xFF;
                }
            }
        }
        
        let item_h = menu_height / 3;
        draw_text(canvas, width, height, &font_manager.font, "Open", 16.0, menu_x + 10, menu_y + 5, (255, 255, 255));
        draw_text(canvas, width, height, &font_manager.font, "Close", 16.0, menu_x + 10, menu_y + 5 + item_h, (255, 255, 255));
        
        let is_pinned = menu_state.target_app_id.as_ref().map(|id| pinned_apps.contains(id)).unwrap_or(false);
        let pin_label = if is_pinned { "Unpin" } else { "Pin" };
        draw_text(canvas, width, height, &font_manager.font, pin_label, 16.0, menu_x + 10, menu_y + 5 + item_h * 2, (255, 255, 255));

        for i in 1..3 {
            let line_y = menu_y + i * item_h;
            for x in 0..menu_width {
                let canvas_x = menu_x + x;
                if canvas_x < width as usize && line_y < height as usize {
                    let canvas_idx = (line_y * (width as usize) + canvas_x) * 4;
                    canvas[canvas_idx] = 0x44; canvas[canvas_idx + 1] = 0x44; canvas[canvas_idx + 2] = 0x44;
                }
            }
        }
    }

    // 5. Render Hover Preview Menu
    if hover_state.is_visible && !menu_state.is_open {
        if let Some(ref app_id) = hover_state.app_id {
            if let Some(windows) = running_by_app.get(app_id) {
                let menu_width = 200;
                let item_h = 30;
                let menu_height = windows.len() * item_h;
                let menu_x = hover_state.x.saturating_sub(menu_width / 2).min((width as usize).saturating_sub(menu_width));
                let menu_y = (height as usize - dock_height).saturating_sub(menu_height + 10);

                for y in 0..menu_height {
                    for x in 0..menu_width {
                        let canvas_x = menu_x + x;
                        let canvas_y = menu_y + y;
                        if canvas_x < width as usize && canvas_y < height as usize {
                            let canvas_idx = (canvas_y * (width as usize) + canvas_x) * 4;
                            canvas[canvas_idx] = 0x33; canvas[canvas_idx + 1] = 0x33; canvas[canvas_idx + 2] = 0x33; canvas[canvas_idx + 3] = 0xEE;
                        }
                    }
                }

                for (i, w) in windows.iter().enumerate() {
                    let title = if w.title.chars().count() > 20 { 
                        format!("{}...", w.title.chars().take(17).collect::<String>()) 
                    } else { 
                        w.title.clone() 
                    };
                    draw_text(canvas, width, height, &font_manager.font, &title, 14.0, menu_x + 5, menu_y + i * item_h + 5, (255, 255, 255));

                    if i > 0 {
                        let line_y = menu_y + i * item_h;
                        for x in 0..menu_width {
                            let canvas_x = menu_x + x;
                            if canvas_x < width as usize && line_y < height as usize {
                                let canvas_idx = (line_y * (width as usize) + canvas_x) * 4;
                                canvas[canvas_idx] = 0x55; canvas[canvas_idx + 1] = 0x55; canvas[canvas_idx + 2] = 0x55;
                            }
                        }
                    }
                }
            }
        }
    }
}
