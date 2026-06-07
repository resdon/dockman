use std::collections::HashMap;
use fontdue::{Font, FontSettings};

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

/// Blits text characters onto a raw 4-byte-per-pixel shared memory buffer.
pub fn render_text_to_canvas(
    fm: &mut FontManager,
    canvas: &mut [u8],
    width: i32,
    height: i32,
    text: &str,
    mut cursor_x: usize,
    baseline_y: i32,
    text_color: u8,
) -> usize {
    for c in text.chars() {
        let glyph = fm.get_glyph(c, 16.0);
        let advance = glyph.metrics.advance_width.round() as usize;

        if !glyph.bitmap.is_empty() {
            let g_width = glyph.metrics.width;
            let g_height = glyph.metrics.height;

            for row in 0..g_height {
                for col in 0..g_width {
                    let bitmap_idx = row * g_width + col;
                    let opacity = glyph.bitmap[bitmap_idx];

                    if opacity == 0 { continue; }

                    let target_x = (cursor_x as i32 + glyph.metrics.xmin + col as i32) as isize;
                    let target_y = (baseline_y - glyph.metrics.ymin - row as i32) as isize;

                    if target_x >= 0 && target_x < width as isize && target_y >= 0 && target_y < height as isize {
                        let pixel_index = (target_y as usize * width as usize + target_x as usize) * 4;
                        if pixel_index + 3 < canvas.len() {
                            let alpha = opacity as f32 / 255.0;
                            // Perform alpha-blending onto Xrgb8888 (BGRA format byte sequence)
                            canvas[pixel_index + 0] = ((text_color as f32 * alpha) + (canvas[pixel_index + 0] as f32 * (1.0 - alpha))) as u8; // Blue
                            canvas[pixel_index + 1] = ((text_color as f32 * alpha) + (canvas[pixel_index + 1] as f32 * (1.0 - alpha))) as u8; // Green
                            canvas[pixel_index + 2] = ((text_color as f32 * alpha) + (canvas[pixel_index + 2] as f32 * (1.0 - alpha))) as u8; // Red
                        }
                    }
                }
            }
        }
        cursor_x += advance;
        if cursor_x >= width as usize { break; }
    }
    cursor_x
}
