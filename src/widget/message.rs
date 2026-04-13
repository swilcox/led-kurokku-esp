use anyhow::Result;
use std::time::Duration;

use crate::display::AnyDisplay;
use crate::font;
use crate::framebuf;
use super::{Widget, CancelToken, sleep_or_cancel};

const DISPLAY_WIDTH: usize = 32;
const STATIC_HOLD_DURATION: Duration = Duration::from_secs(5);

/// Message widget — shows text on the pixel display.
/// Short text that fits the display is shown centered and static.
/// Long text scrolls across the display.
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
                anyhow::bail!("message widget does not yet support segment displays");
            }
        };

        let cols = font::render_text(&self.text);
        let text_width = cols.len();

        if text_width <= DISPLAY_WIDTH {
            return self.run_static(disp, cancel);
        }

        self.run_scroll(disp, &cols, cancel)
    }
}

impl Message {
    fn run_static(
        &self,
        disp: &mut dyn crate::display::PixelDisplay,
        cancel: &CancelToken,
    ) -> Result<()> {
        let mut frame = [0u8; 32];
        framebuf::blit_text_centered(&mut frame, &self.text);
        disp.write_framebuffer(&frame);

        let mut reps_remaining = self.repeats;
        loop {
            if reps_remaining == 0 {
                return Ok(());
            }
            sleep_or_cancel(cancel, STATIC_HOLD_DURATION)?;
            if reps_remaining > 0 {
                reps_remaining -= 1;
            }
        }
    }

    fn run_scroll(
        &self,
        disp: &mut dyn crate::display::PixelDisplay,
        cols: &[u8],
        cancel: &CancelToken,
    ) -> Result<()> {
        let text_width = cols.len();
        let total_scroll = text_width + DISPLAY_WIDTH;
        let mut reps_remaining = self.repeats;

        loop {
            if reps_remaining == 0 {
                return Ok(());
            }

            for offset in 0..=total_scroll {
                if cancel.is_cancelled() {
                    anyhow::bail!("cancelled");
                }

                let mut frame = [0u8; 32];
                for (i, &col) in cols.iter().enumerate() {
                    let x = i as isize - offset as isize + DISPLAY_WIDTH as isize;
                    if x >= 0 && (x as usize) < DISPLAY_WIDTH {
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
