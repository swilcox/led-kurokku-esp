use anyhow::Result;
use std::mem::MaybeUninit;
use std::time::Duration;

use crate::display::{AnyDisplay, PixelDisplay};
use crate::framebuf;
use super::{Widget, CancelToken, sleep_or_cancel};

/// Read the current local-time hour (0-23) and minute via newlib's
/// `localtime_r`, which honors the `TZ` environment variable set at startup.
fn local_hour_minute() -> (u8, u8) {
    let mut t: esp_idf_svc::sys::time_t = 0;
    let mut tm = MaybeUninit::<esp_idf_svc::sys::tm>::zeroed();
    let tm = unsafe {
        esp_idf_svc::sys::time(&mut t);
        esp_idf_svc::sys::localtime_r(&t, tm.as_mut_ptr());
        tm.assume_init()
    };
    (tm.tm_hour as u8, tm.tm_min as u8)
}

/// Clock widget — displays current time with blinking colon.
///
/// AM/24h mode: 500ms colon on, 500ms colon off.
/// PM mode (12h): double-blink — 150ms on, 200ms off, 150ms on, 500ms off.
pub struct Clock {
    pub format_24h: bool,
}

impl Clock {
    pub fn new(format_24h: bool) -> Self {
        Self { format_24h }
    }

    fn show_text(
        &self,
        disp: &mut dyn PixelDisplay,
        text: &str,
        duration: Duration,
        cancel: &CancelToken,
    ) -> Result<()> {
        let mut frame = [0u8; 32];
        framebuf::blit_text_centered(&mut frame, text);
        disp.write_framebuffer(&frame);
        sleep_or_cancel(cancel, duration)
    }
}

impl Widget for Clock {
    fn name(&self) -> &str {
        "clock"
    }

    fn run(&mut self, display: &mut AnyDisplay, cancel: &CancelToken) -> Result<()> {
        let disp = match display {
            AnyDisplay::Pixel(ref mut d) => d.as_mut(),
            AnyDisplay::Segment(_) => {
                // TODO: segment clock variant
                anyhow::bail!("clock widget does not yet support segment displays");
            }
        };

        loop {
            if cancel.is_cancelled() {
                return Ok(());
            }

            let (hour_raw, minute) = local_hour_minute();
            let is_pm = hour_raw >= 12;

            let hour = if self.format_24h {
                hour_raw
            } else {
                let h = hour_raw % 12;
                if h == 0 { 12 } else { h }
            };

            let colon_on = if self.format_24h || hour >= 10 {
                format!("{:02}:{:02}", hour, minute)
            } else {
                format!("{}:{:02}", hour, minute)
            };

            let colon_off = if self.format_24h || hour >= 10 {
                format!("{:02} {:02}", hour, minute)
            } else {
                format!("{} {:02}", hour, minute)
            };

            if !self.format_24h && is_pm {
                // PM double blink: 150ms on, 200ms off, 150ms on, 500ms off
                self.show_text(disp, &colon_on, Duration::from_millis(150), cancel)?;
                self.show_text(disp, &colon_off, Duration::from_millis(200), cancel)?;
                self.show_text(disp, &colon_on, Duration::from_millis(150), cancel)?;
                self.show_text(disp, &colon_off, Duration::from_millis(500), cancel)?;
            } else {
                // 24h or AM: 500ms on, 500ms off
                self.show_text(disp, &colon_on, Duration::from_millis(500), cancel)?;
                self.show_text(disp, &colon_off, Duration::from_millis(500), cancel)?;
            }
        }
    }
}
