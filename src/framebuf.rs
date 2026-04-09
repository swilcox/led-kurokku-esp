use crate::font;

/// 32x8 LED matrix framebuffer.
/// Each byte is one vertical column (bit 0 = top row, bit 7 = bottom row).
pub type Frame = [u8; 32];

/// Clear all pixels.
pub fn clear(frame: &mut Frame) {
    *frame = [0u8; 32];
}

/// Set or clear the pixel at (x, y). x: 0-31, y: 0-7.
pub fn set_pixel(frame: &mut Frame, x: usize, y: usize, on: bool) {
    if x >= 32 || y >= 8 {
        return;
    }
    if on {
        frame[x] |= 1 << y;
    } else {
        frame[x] &= !(1 << y);
    }
}

/// Get the pixel value at (x, y).
pub fn get_pixel(frame: &Frame, x: usize, y: usize) -> bool {
    if x >= 32 || y >= 8 {
        return false;
    }
    frame[x] & (1 << y) != 0
}

/// Render text into the frame at the given horizontal offset.
/// Returns the total pixel width of the rendered text.
pub fn blit_text(frame: &mut Frame, text: &str, offset_x: usize) -> usize {
    let cols = font::render_text(text);
    for (i, &col) in cols.iter().enumerate() {
        let x = offset_x + i;
        if x < 32 {
            frame[x] = col;
        }
    }
    cols.len()
}

/// Render text centered horizontally in the frame.
/// Returns the total pixel width of the rendered text.
pub fn blit_text_centered(frame: &mut Frame, text: &str) -> usize {
    let cols = font::render_text(text);
    let w = cols.len();
    let offset = if w < 32 { (32 - w) / 2 } else { 0 };
    for (i, &col) in cols.iter().enumerate() {
        let x = offset + i;
        if x < 32 {
            frame[x] = col;
        }
    }
    w
}
