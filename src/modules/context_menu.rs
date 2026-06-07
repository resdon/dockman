use crate::modules::world::World;
use resvg::tiny_skia::{Pixmap, Rect};

/// Renders a context menu with three items: Open, Close, Pin/Unpin.
/// Returns a tuple of (menu_pixmap, item_rects) where item_rects are in menu-local coordinates.
pub fn render_context_menu(
    world: &mut World,
    width: u32,
    height: u32,
    is_pinned: bool,
) -> (Pixmap, Vec<Rect>) {
    // Create a raw RGBA buffer for the menu
    let mut frame: Vec<u8> = vec![0; (width * height * 4) as usize];

    // Fill background with a dark gray color
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            frame[idx..idx + 4].copy_from_slice(&[40, 40, 40, 255]);
        }
    }

    // Menu items
    let items = [
        "Open".to_string(),
        "Close".to_string(), 
        if is_pinned { "Unpin".to_string() } else { "Pin".to_string() },
    ];

    let text_size = 14.0;
    let item_height = height / items.len() as u32;

    let mut item_rects = Vec::new();

    for (idx, label) in items.iter().enumerate() {
        let y = (idx as u32 * item_height) as f32;
        let rect = Rect::from_xywh(0.0, y, width as f32, item_height as f32).unwrap();
        item_rects.push(rect);

        // Draw text centered in the item
        let text_width = label.len() as f32 * (text_size / 2.0); // Simple approximation
        let text_x = ((width as f32 - text_width) / 2.0) as usize;
        let baseline_y = (y + item_height as f32 / 2.0 + text_size / 2.0) as usize;

        // Use the world's draw_text to render onto the raw frame buffer,
        // passing the actual dimensions of this buffer.
        if let Err(e) = world.draw_text(
            &mut frame,
            label,
            text_x,
            baseline_y,
            text_size,
            [255, 255, 255],
            width as usize,
            height as usize,
        ) {
            eprintln!("Failed to draw text: {}", e);
        }
    }

    // Convert the raw frame buffer into a Pixmap.
    let mut pixmap = Pixmap::new(width, height).expect("Failed to create menu pixmap");
    pixmap.data_mut().copy_from_slice(&frame);

    (pixmap, item_rects)
}
