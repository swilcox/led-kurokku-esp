use anyhow::Result;
use std::time::Duration;

use crate::display::AnyDisplay;
use super::{Widget, CancelToken, sleep_or_cancel};

/// Raw pixel renderer — server sends 32 bytes, we display them directly.
pub struct RawPixel {
    data: Vec<u8>,
}

impl RawPixel {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl Widget for RawPixel {
    fn name(&self) -> &str {
        "raw_pixel"
    }

    fn run(&mut self, display: &mut AnyDisplay, cancel: &CancelToken) -> Result<()> {
        let disp = match display {
            AnyDisplay::Pixel(ref mut d) => d.as_mut(),
            _ => anyhow::bail!("raw_pixel requires a pixel display"),
        };

        let mut frame = [0u8; 32];
        let len = self.data.len().min(32);
        frame[..len].copy_from_slice(&self.data[..len]);
        disp.write_framebuffer(&frame);

        // Hold the frame until cancelled — this is a one-shot display
        loop {
            if cancel.is_cancelled() {
                anyhow::bail!("cancelled");
            }
            sleep_or_cancel(cancel, Duration::from_millis(100))?;
        }
    }
}

/// Raw segment renderer — server sends segment bitmasks, we display them directly.
pub struct RawSegment {
    segments: Vec<u16>,
    colon: bool,
}

impl RawSegment {
    pub fn new(segments: Vec<u16>, colon: bool) -> Self {
        Self { segments, colon }
    }
}

impl Widget for RawSegment {
    fn name(&self) -> &str {
        "raw_segment"
    }

    fn run(&mut self, display: &mut AnyDisplay, cancel: &CancelToken) -> Result<()> {
        let disp = match display {
            AnyDisplay::Segment(ref mut d) => d.as_mut(),
            _ => anyhow::bail!("raw_segment requires a segment display"),
        };

        disp.write_segments(&self.segments, self.colon);

        // Hold until cancelled
        loop {
            if cancel.is_cancelled() {
                anyhow::bail!("cancelled");
            }
            sleep_or_cancel(cancel, Duration::from_millis(100))?;
        }
    }
}
