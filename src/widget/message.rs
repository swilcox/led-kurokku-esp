use anyhow::Result;
use std::time::Duration;

use crate::display::AnyDisplay;
use crate::font;
use super::{Widget, CancelToken, sleep_or_cancel};

/// Message widget — scrolls text across the pixel display.
pub struct Message {
    text: String,
    scroll_speed: Duration,
    repeats: i32,
}

impl Message {
    pub fn new(text: &str, scroll_speed: Duration, repeats: i32) -> Self {
        Self {
            text: text.to_string(),
            scroll_speed,
            repeats,
        }
    }
}

impl Widget for Message {
    fn name(&self) -> &str {
        "message"
    }

    fn run(&mut self, display: &mut AnyDisplay, cancel: &CancelToken) -> Result<()> {
        let disp = match display {
            AnyDisplay::Pixel(ref mut d) => d.as_mut(),
            AnyDisplay::Segment(_) => {
                // TODO: segment message variant (static 4-char display)
                anyhow::bail!("message widget does not yet support segment displays");
            }
        };

        let cols = font::render_text(&self.text);
        let text_width = cols.len();
        let total_scroll = text_width + 32;

        // repeats < 0 means infinite
        let mut reps_remaining = self.repeats;

        loop {
            if reps_remaining == 0 {
                return Ok(());
            }

            // Scroll text from right to left
            for offset in 0..total_scroll {
                if cancel.is_cancelled() {
                    anyhow::bail!("cancelled");
                }

                let mut frame = [0u8; 32];
                for (i, &col) in cols.iter().enumerate() {
                    let x = i as isize - offset as isize + 32;
                    if x >= 0 && x < 32 {
                        frame[x as usize] = col;
                    }
                }
                disp.write_framebuffer(&frame);
                sleep_or_cancel(cancel, self.scroll_speed)?;
            }

            if reps_remaining > 0 {
                reps_remaining -= 1;
            }
        }
    }
}
