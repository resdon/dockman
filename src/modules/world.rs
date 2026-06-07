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

        // FIX: Solves the borrow checker conflict using Entry API
        self.cache.entry(key).or_insert_with(|| {
            let (metrics, bitmap) = self.font.rasterize(character, size);
            CachedGlyph { bitmap, metrics }
        })
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.cache.shrink_to_fit();
    }
}

pub struct World {
    pub font_manager: FontManager,
    pub width: usize,
    pub height: usize,
}

impl World {
    pub fn new(width: usize, height: usize, font_data: &[u8]) -> Self {
        Self {
            font_manager: FontManager::new(font_data),
            width,
            height,
        }
    }

	pub fn draw_blue_box(
	        &self,
	        frame: &mut [u8],
	        x_start: usize,
	        x_end: usize,
	        y_start: usize,
	        y_end: usize,
	    ) {
	        // Neon Blue color array in standard Wayland format (assuming BGRA8888 or RGBA8888)
	        // Blue = 255, Green = 100, Red = 0, Alpha = 255
	        let blue_pixel: [u8; 4] = [255, 100, 0, 255]; 
	
	        for y in y_start..=y_end {
	            for x in x_start..=x_end {
	                // Only draw pixels that lie on the edges of the box
	                let is_edge = x == x_start || x == x_end || y == y_start || y == y_end;
	                
	                if is_edge {
	                    // Calculate pixel index based on the taskbar width
	                    let pixel_index = (y * self.width + x) * 4;
	
	                    // Ensure we don't overflow the shared memory buffer slice boundary
	                    if pixel_index + 3 < frame.len() {
	                        frame[pixel_index..pixel_index + 4].copy_from_slice(&blue_pixel);
	                    }
	                }
	            }
	        }
	    }

    pub fn draw_text(
        &mut self,
        frame: &mut [u8],
        text: &str,
        start_x: usize,
        baseline_y: usize,
        size: f32,
        color: [u8; 3],
        frame_width: usize,
        frame_height: usize,
    ) -> Result<(), String> {
        let mut cursor_x = start_x;

        for c in text.chars() {
            let glyph = self.font_manager.get_glyph(c, size);
            let advance = glyph.metrics.advance_width.round() as usize;

            // Only blit if the glyph actually contains pixel information
            if !glyph.bitmap.is_empty() {
                Self::blit_glyph(
                    frame,
                    frame_width,
                    frame_height,
                    glyph,
                    cursor_x,
                    baseline_y,
                    color,
                );
            }

            cursor_x += advance;
            if cursor_x >= frame_width {
                break; // Text hit the right boundary edge
            }
        }

        Ok(())
    }

    fn blit_glyph(
        frame: &mut [u8],
        width: usize,
        height: usize,
        glyph: &CachedGlyph,
        x: usize,
        y: usize,
        color: [u8; 3],
    ) {
        let g_width = glyph.metrics.width;
        let g_height = glyph.metrics.height;

        for row in 0..g_height {
            for col in 0..g_width {
                let bitmap_idx = row * g_width + col;
                let opacity_u8 = glyph.bitmap[bitmap_idx];

                if opacity_u8 == 0 {
                    continue; 
                }

                // FIX: Standard Typographical Alignment Math
                let target_x = (x as i32 + glyph.metrics.xmin + col as i32) as isize;
                let target_y = (y as i32 - glyph.metrics.ymin - row as i32) as isize;

                // Canvas boundaries validation
                if target_x < 0
                    || target_x >= width as isize
                    || target_y < 0
                    || target_y >= height as isize
                {
                    continue;
                }

                let pixel_index = (target_y as usize * width + target_x as usize) * 4;

                // Ensure safety within our shared memory slice
                if pixel_index + 3 >= frame.len() {
                    continue;
                }

                let alpha = opacity_u8 as f32 / 255.0;
                
                // Alpha blend raw components (Assuming a standard BGRA/RGBA layout target)
                for i in 0..3 {
                    let dst = frame[pixel_index + i] as f32;
                    let src = color[i] as f32;
                    let blended = (src * alpha) + (dst * (1.0 - alpha));
                    frame[pixel_index + i] = blended as u8;
                }
            }
        }
    }

    pub fn clear_font_cache(&mut self) {
        self.font_manager.clear_cache();
    }
}
