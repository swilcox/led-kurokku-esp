use anyhow::Result;
use std::time::Duration;

use crate::display::AnyDisplay;
use crate::framebuf;
use super::{Widget, CancelToken, sleep_or_cancel};

/// Status widget — displays a status message (IP address, error, etc.)
/// on the pixel display. Scrolls if text is wider than 32 pixels.
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
        let disp = match display {
            AnyDisplay::Pixel(ref mut d) => d.as_mut(),
            AnyDisplay::Segment(_) => {
                // TODO: segment status variant
                anyhow::bail!("status widget does not yet support segment displays");
            }
        };

        let cols = crate::font::render_text(&self.text);
        let text_width = cols.len();

        if text_width <= 32 {
            // Static display — fits on screen, center it
            let mut frame = [0u8; 32];
            framebuf::blit_text_centered(&mut frame, &self.text);
            disp.write_framebuffer(&frame);
            sleep_or_cancel(cancel, self.duration)?;
        } else {
            // Scroll the text across the display
            let scroll_speed = Duration::from_millis(30);
            let total_scroll = text_width + 32; // scroll fully off-screen
            let start = std::time::Instant::now();

            for offset in 0..=total_scroll {
                if cancel.is_cancelled() || start.elapsed() >= self.duration {
                    return Ok(());
                }
                let mut frame = [0u8; 32];
                // Blit with negative offset to scroll left
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
}
