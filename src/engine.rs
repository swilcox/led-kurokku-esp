use std::sync::{Arc, Mutex};
use std::time::Duration;

use log;

use crate::config::AppConfig;
use crate::display::AnyDisplay;
use crate::network::{InstructionSource, WidgetInstruction};
use crate::widget::clock::Clock;
use crate::widget::status::Status;
use crate::widget::{CancelToken, Widget};

/// Shared state between network and display threads.
struct SharedState {
    /// Latest instruction from the server; None = no change.
    pending_instruction: Option<WidgetInstruction>,
    /// Server-requested brightness override.
    pending_brightness: Option<u8>,
    /// Set to true when the network thread detects an OTA request.
    ota_url: Option<String>,
}

/// Engine manages the two-thread display loop.
pub struct Engine {
    config: AppConfig,
}

impl Engine {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    /// Run the engine. This blocks forever.
    ///
    /// - `display`: the initialized display
    /// - `source`: the instruction source (HTTP poller, WebSocket, etc.)
    pub fn run(&self, mut display: AnyDisplay, mut source: Box<dyn InstructionSource>) -> ! {
        let shared = Arc::new(Mutex::new(SharedState {
            pending_instruction: None,
            pending_brightness: None,
            ota_url: None,
        }));

        // Network thread: polls/receives instructions, updates shared state
        let shared_net = shared.clone();
        let _net_thread = std::thread::Builder::new()
            .name("net".into())
            .stack_size(6144)
            .spawn(move || {
                network_loop(&mut *source, &shared_net);
            })
            .expect("Failed to spawn network thread");

        // Display thread (this thread): runs widgets, checks for new instructions
        self.display_loop(&mut display, &shared);
    }

    fn display_loop(&self, display: &mut AnyDisplay, shared: &Arc<Mutex<SharedState>>) -> ! {
        let mut current_widget: Box<dyn Widget> = Box::new(Clock::new(self.config.format_24h));
        let mut cancel = CancelToken::new();

        loop {
            // Check for pending instructions
            {
                let mut state = shared.lock().unwrap();

                if let Some(brightness) = state.pending_brightness.take() {
                    match display {
                        AnyDisplay::Pixel(ref mut d) => d.set_brightness(brightness),
                        AnyDisplay::Segment(ref mut d) => d.set_brightness(brightness),
                    }
                }

                if let Some(url) = state.ota_url.take() {
                    log::info!("OTA requested: {}", url);
                    // Release the lock before OTA (it may take a while)
                    drop(state);

                    // Show OTA status on display
                    cancel.cancel();
                    cancel = CancelToken::new();
                    let mut ota_status = Status::new("OTA...", Duration::from_secs(60));
                    let _ = ota_status.run(display, &CancelToken::with_timeout(Duration::from_secs(2)));

                    // Perform OTA — this reboots on success
                    match crate::ota::perform_ota(&url) {
                        Ok(()) => unreachable!("OTA reboots the device"),
                        Err(e) => {
                            log::error!("OTA failed: {}", e);
                            let mut fail_status = Status::new("OTA FAIL", Duration::from_secs(3));
                            let _ = fail_status.run(display, &CancelToken::with_timeout(Duration::from_secs(3)));
                        }
                    }

                    // Revert to clock after failed OTA
                    current_widget = Box::new(Clock::new(self.config.format_24h));
                    cancel = CancelToken::new();
                    continue;
                }

                if let Some(instruction) = state.pending_instruction.take() {
                    log::info!("New instruction: {:?}", instruction);
                    cancel.cancel(); // stop current widget

                    current_widget = match instruction {
                        WidgetInstruction::Clock { format_24h } => {
                            Box::new(Clock::new(format_24h))
                        }
                        WidgetInstruction::Message { text, scroll_speed_ms, repeats } => {
                            Box::new(crate::widget::message::Message::new(
                                &text,
                                Duration::from_millis(scroll_speed_ms),
                                repeats,
                            ))
                        }
                        WidgetInstruction::RawPixel { data } => {
                            Box::new(crate::widget::raw_render::RawPixel::new(data))
                        }
                        WidgetInstruction::RawSegment { segments, colon } => {
                            Box::new(crate::widget::raw_render::RawSegment::new(segments, colon))
                        }
                        WidgetInstruction::Ota { url } => {
                            // Put it back for OTA handling above on next loop
                            state.ota_url = Some(url);
                            Box::new(Status::new("OTA...", Duration::from_secs(5)))
                        }
                    };

                    cancel = CancelToken::new();
                }
            }

            // Run current widget for one iteration
            match current_widget.run(display, &cancel) {
                Ok(()) => {
                    // Widget finished (e.g. message done scrolling) — revert to clock
                    log::info!("Widget finished, reverting to clock");
                    current_widget = Box::new(Clock::new(self.config.format_24h));
                    cancel = CancelToken::new();
                }
                Err(e) => {
                    let msg = format!("{}", e);
                    if msg == "cancelled" {
                        // Normal cancellation from new instruction — loop will pick it up
                        continue;
                    }
                    log::error!("Widget error: {}", e);
                    current_widget = Box::new(Clock::new(self.config.format_24h));
                    cancel = CancelToken::new();
                }
            }
        }
    }
}

/// Network thread loop. Polls the instruction source and updates shared state.
fn network_loop(source: &mut dyn InstructionSource, shared: &Arc<Mutex<SharedState>>) {
    loop {
        match source.next_instruction() {
            Ok(Some(resp)) => {
                let mut state = shared.lock().unwrap();
                if let Some(instruction) = resp.instruction {
                    state.pending_instruction = Some(instruction);
                }
                if let Some(brightness) = resp.brightness {
                    state.pending_brightness = Some(brightness);
                }
            }
            Ok(None) => {
                // No new instruction — server is reachable but nothing changed
            }
            Err(e) => {
                log::warn!("Instruction fetch failed: {}", e);
                // Don't update state — keep running current widget (graceful degradation)
            }
        }

        let interval = source.recommended_interval();
        std::thread::sleep(interval);
    }
}
