use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::display::AnyDisplay;

pub mod clock;
pub mod message;
pub mod raw_render;
pub mod status;

/// Widget trait. Widgets run until cancelled.
pub trait Widget: Send {
    fn name(&self) -> &str;
    /// Run the widget. Returns Ok(()) when cancelled, Err on failure.
    fn run(&mut self, display: &mut AnyDisplay, cancel: &CancelToken) -> Result<()>;
}

/// Lightweight cancellation token backed by an atomic bool.
#[derive(Clone)]
pub struct CancelToken {
    cancelled: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create a token that auto-cancels after the given duration.
    pub fn with_timeout(duration: Duration) -> Self {
        let token = Self::new();
        let cancelled = token.cancelled.clone();
        std::thread::spawn(move || {
            std::thread::sleep(duration);
            cancelled.store(true, Ordering::Relaxed);
        });
        token
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// Sleep for the given duration, returning early if cancelled.
/// Returns Err if cancelled.
pub fn sleep_or_cancel(cancel: &CancelToken, duration: Duration) -> Result<()> {
    // Sleep in small increments to check cancellation
    let step = Duration::from_millis(50);
    let mut remaining = duration;
    while remaining > Duration::ZERO {
        if cancel.is_cancelled() {
            anyhow::bail!("cancelled");
        }
        let sleep_time = remaining.min(step);
        std::thread::sleep(sleep_time);
        remaining = remaining.saturating_sub(sleep_time);
    }
    if cancel.is_cancelled() {
        anyhow::bail!("cancelled");
    }
    Ok(())
}
