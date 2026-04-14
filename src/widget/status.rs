use anyhow::Result;
use std::time::Duration;

use crate::display::{AnyDisplay, PixelDisplay, SegmentDisplay};
use crate::font_7seg;
use crate::framebuf;
use super::{Widget, CancelToken, sleep_or_cancel};

/// Status widget — displays a status message (IP address, error, etc.).
/// Scrolls when the text is wider than the display.
pub struct Status {
    pub text: String,
    pub duration: Duration,
}

impl Status {
    /// Show a message for a given duration.
    pub fn new(text: &str, duration: Duration) -> Self {
        Self {
            text: text.to_string(),
            duration,
        }
    }
}

impl Widget for Status {
    fn name(&self) -> &str {
        "status"
    }

    fn run(&mut self, display: &mut AnyDisplay, cancel: &CancelToken) -> Result<()> {
        match display {
            AnyDisplay::Pixel(d) => self.run_pixel(d.as_mut(), cancel),
            AnyDisplay::Segment(d) => self.run_segment(d.as_mut(), cancel),
        }
    }
}

impl Status {
    fn run_pixel(&self, disp: &mut dyn PixelDisplay, cancel: &CancelToken) -> Result<()> {
        let cols = crate::font::render_text(&self.text);
        let text_width = cols.len();

        if text_width <= 32 {
            let mut frame = [0u8; 32];
            framebuf::blit_text_centered(&mut frame, &self.text);
            disp.write_framebuffer(&frame);
            sleep_or_cancel(cancel, self.duration)?;
        } else {
            let scroll_speed = Duration::from_millis(30);
            let total_scroll = text_width + 32;
            let start = std::time::Instant::now();

            for offset in 0..=total_scroll {
                if cancel.is_cancelled() || start.elapsed() >= self.duration {
                    return Ok(());
                }
                let mut frame = [0u8; 32];
                for (i, &col) in cols.iter().enumerate() {
                    let x = i as isize - offset as isize + 32;
                    if x >= 0 && x < 32 {
                        frame[x as usize] = col;
                    }
                }
                disp.write_framebuffer(&frame);
                sleep_or_cancel(cancel, scroll_speed)?;
            }
        }

        Ok(())
    }

    fn run_segment(&self, disp: &mut dyn SegmentDisplay, cancel: &CancelToken) -> Result<()> {
        let width = disp.display_length();
        let encoded = font_7seg::encode_text(&self.text);

        if encoded.len() <= width {
            // Left-align static text (short status messages fit naturally).
            let mut segs = vec![0u16; width];
            for (i, &b) in encoded.iter().enumerate() {
                segs[i] = b as u16;
            }
            disp.write_segments(&segs, false);
            sleep_or_cancel(cancel, self.duration)?;
            return Ok(());
        }

        // Scroll: pad `width` blanks each side, slide window.
        let scroll_speed = Duration::from_millis(250);
        let mut padded: Vec<u8> = Vec::with_capacity(encoded.len() + 2 * width);
        padded.extend(std::iter::repeat(0).take(width));
        padded.extend_from_slice(&encoded);
        padded.extend(std::iter::repeat(0).take(width));

        let total_starts = padded.len() - width;
        let start = std::time::Instant::now();

        for s in 0..=total_starts {
            if cancel.is_cancelled() || start.elapsed() >= self.duration {
                return Ok(());
            }
            let mut segs = vec![0u16; width];
            for i in 0..width {
                segs[i] = padded[s + i] as u16;
            }
            disp.write_segments(&segs, false);
            sleep_or_cancel(cancel, scroll_speed)?;
        }

        Ok(())
    }
}
