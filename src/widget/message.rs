use anyhow::Result;
use std::time::Duration;

use crate::display::{AnyDisplay, PixelDisplay, SegmentDisplay};
use crate::font;
use crate::font_7seg;
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
        match display {
            AnyDisplay::Pixel(d) => {
                let disp = d.as_mut();
                let cols = font::render_text(&self.text);
                if cols.len() <= DISPLAY_WIDTH {
                    self.run_static_pixel(disp, cancel)
                } else {
                    self.run_scroll_pixel(disp, &cols, cancel)
                }
            }
            AnyDisplay::Segment(d) => {
                let disp = d.as_mut();
                let width = disp.display_length();
                let encoded = font_7seg::encode_text(&self.text);
                if encoded.len() <= width {
                    self.run_static_segment(disp, &encoded, cancel)
                } else {
                    self.run_scroll_segment(disp, &encoded, cancel)
                }
            }
        }
    }
}

impl Message {
    fn run_static_pixel(
        &self,
        disp: &mut dyn PixelDisplay,
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

    fn run_scroll_pixel(
        &self,
        disp: &mut dyn PixelDisplay,
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

    fn run_static_segment(
        &self,
        disp: &mut dyn SegmentDisplay,
        encoded: &[u8],
        cancel: &CancelToken,
    ) -> Result<()> {
        let width = disp.display_length();
        let mut segs = vec![0u16; width];
        for (i, &b) in encoded.iter().take(width).enumerate() {
            segs[i] = b as u16;
        }
        disp.write_segments(&segs, false);

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

    fn run_scroll_segment(
        &self,
        disp: &mut dyn SegmentDisplay,
        encoded: &[u8],
        cancel: &CancelToken,
    ) -> Result<()> {
        // Pad with `width` blanks on each side so the message slides fully
        // in and fully out, matching the Python sister project.
        let width = disp.display_length();
        let mut padded: Vec<u8> = Vec::with_capacity(encoded.len() + 2 * width);
        padded.extend(std::iter::repeat(0).take(width));
        padded.extend_from_slice(encoded);
        padded.extend(std::iter::repeat(0).take(width));

        let total_starts = padded.len() - width; // inclusive upper bound
        let mut reps_remaining = self.repeats;

        loop {
            if reps_remaining == 0 {
                return Ok(());
            }

            for start in 0..=total_starts {
                if cancel.is_cancelled() {
                    anyhow::bail!("cancelled");
                }

                let mut segs = vec![0u16; width];
                for i in 0..width {
                    segs[i] = padded[start + i] as u16;
                }
                disp.write_segments(&segs, false);
                sleep_or_cancel(cancel, self.scroll_speed)?;
            }

            if reps_remaining > 0 {
                reps_remaining -= 1;
            }
        }
    }
}
