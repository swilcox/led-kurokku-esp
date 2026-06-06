use std::sync::{Arc, Mutex};
use std::time::Duration;

use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use log;

use crate::config::AppConfig;
use crate::display::AnyDisplay;
use crate::network::{ConfigUpdate, InstructionSource, WidgetInstruction};
use crate::widget::clock::Clock;
use crate::widget::status::Status;
use crate::widget::{CancelToken, Widget};

pub type SharedWifi = Arc<Mutex<BlockingWifi<EspWifi<'static>>>>;

/// Shared state between network and display threads.
struct SharedState {
    /// Latest instruction from the server; None = no change.
    pending_instruction: Option<WidgetInstruction>,
    /// Server-requested brightness override.
    pending_brightness: Option<u8>,
    /// Set to true when the network thread detects an OTA request.
    ota_url: Option<String>,
    /// Remote config update from the server (syslog target, timezone, …).
    /// Applied by the display thread as a side effect without disturbing the
    /// active widget. Deduplicated by the network thread.
    pending_config: Option<ConfigUpdate>,
    /// Cancel token for the currently running widget. The network thread
    /// signals this when it installs a new instruction so the widget wakes
    /// up and the display loop can pick the instruction up. The display
    /// thread replaces it with a fresh token each time it starts a widget.
    current_cancel: CancelToken,
}

/// Engine manages the two-thread display loop.
pub struct Engine {
    config: AppConfig,
    /// NVS partition handle so remote config updates can be persisted.
    nvs_partition: EspDefaultNvsPartition,
}

impl Engine {
    pub fn new(config: AppConfig, nvs_partition: EspDefaultNvsPartition) -> Self {
        Self { config, nvs_partition }
    }

    /// Run the engine. This blocks forever.
    ///
    /// - `display`: the initialized display
    /// - `source`: the instruction source (HTTP poller, WebSocket, etc.)
    /// - `wifi`: optional shared WiFi handle. When present, the network
    ///   thread will attempt to reconnect after a poll failure if the STA
    ///   is no longer associated. None disables reconnect (e.g., offline
    ///   boot with no credentials).
    pub fn run(
        &self,
        mut display: AnyDisplay,
        mut source: Box<dyn InstructionSource>,
        wifi: Option<SharedWifi>,
    ) -> ! {
        let shared = Arc::new(Mutex::new(SharedState {
            pending_instruction: None,
            pending_brightness: None,
            ota_url: None,
            pending_config: None,
            current_cancel: CancelToken::new(),
        }));

        // Network thread: polls/receives instructions, updates shared state
        let shared_net = shared.clone();
        let wifi_net = wifi.clone();
        let _net_thread = std::thread::Builder::new()
            .name("net".into())
            .stack_size(6144)
            .spawn(move || {
                network_loop(&mut *source, &shared_net, wifi_net);
            })
            .expect("Failed to spawn network thread");

        // Display thread (this thread): runs widgets, checks for new instructions
        self.display_loop(&mut display, &shared);
    }

    fn display_loop(&self, display: &mut AnyDisplay, shared: &Arc<Mutex<SharedState>>) -> ! {
        // Track the most recently requested clock format so fallbacks (widget
        // finished, error, failed OTA) stay consistent with what the server
        // last asked for — not the compile-time config default.
        let mut current_format_24h = self.config.format_24h;
        let mut current_widget: Box<dyn Widget> = Box::new(Clock::new(current_format_24h));
        let mut cancel = shared.lock().unwrap().current_cancel.clone();

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

                if let Some(update) = state.pending_config.take() {
                    // Apply persisted-config changes without disturbing the
                    // active widget. Release the lock first — NVS writes and
                    // logging can be slow.
                    drop(state);
                    self.apply_config_update(&update);
                    continue;
                }

                if let Some(url) = state.ota_url.take() {
                    log::info!("OTA requested: {}", url);
                    // Release the lock before OTA (it may take a while)
                    drop(state);

                    // Show OTA status on display
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
                    current_widget = Box::new(Clock::new(current_format_24h));
                    let new_cancel = CancelToken::new();
                    shared.lock().unwrap().current_cancel = new_cancel.clone();
                    cancel = new_cancel;
                    continue;
                }

                if let Some(instruction) = state.pending_instruction.take() {
                    log::info!("New instruction: {:?}", instruction);

                    current_widget = match instruction {
                        WidgetInstruction::Clock { format_24h } => {
                            current_format_24h = format_24h;
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
                        WidgetInstruction::Animation { animation, duration_secs } => {
                            use crate::widget::animation::{Animation, AnimationType};
                            let anim_type = match animation.as_str() {
                                "pong" => AnimationType::Pong,
                                "matrix" | "matrix_rain" => AnimationType::MatrixRain,
                                "snake" => AnimationType::Snake,
                                "curtain" => AnimationType::Curtain,
                                "sine" | "sine_wave" | "sinewave" => AnimationType::SineWave,
                                "random" => AnimationType::Random,
                                _ => AnimationType::Static,
                            };
                            let dur = if duration_secs == 0 {
                                Duration::ZERO
                            } else {
                                Duration::from_secs(duration_secs)
                            };
                            Box::new(Animation::new(anim_type, dur))
                        }
                        WidgetInstruction::Ota { url } => {
                            // Put it back for OTA handling above on next loop
                            state.ota_url = Some(url);
                            Box::new(Status::new("OTA...", Duration::from_secs(5)))
                        }
                    };

                    // Install a fresh cancel token so the next server update
                    // targets THIS widget, not the stale one.
                    let new_cancel = CancelToken::new();
                    state.current_cancel = new_cancel.clone();
                    cancel = new_cancel;
                }
            }

            // Run current widget for one iteration
            match current_widget.run(display, &cancel) {
                Ok(()) => {
                    // Widget finished (e.g. message done scrolling) — revert to clock
                    log::info!("Widget finished, reverting to clock");
                    current_widget = Box::new(Clock::new(current_format_24h));
                    let new_cancel = CancelToken::new();
                    shared.lock().unwrap().current_cancel = new_cancel.clone();
                    cancel = new_cancel;
                }
                Err(e) => {
                    let msg = format!("{}", e);
                    if msg == "cancelled" {
                        // Normal cancellation from new instruction — loop will pick it up
                        continue;
                    }
                    log::error!("Widget error: {}", e);
                    current_widget = Box::new(Clock::new(current_format_24h));
                    let new_cancel = CancelToken::new();
                    shared.lock().unwrap().current_cancel = new_cancel.clone();
                    cancel = new_cancel;
                }
            }
        }
    }

    /// Apply a remote config update: persist the changed keys to NVS and apply
    /// them live where possible (syslog target, timezone) so no reboot is
    /// needed. Called from the display thread.
    fn apply_config_update(&self, update: &ConfigUpdate) {
        let mut nvs = match crate::config::open_nvs(self.nvs_partition.clone()) {
            Ok(n) => n,
            Err(e) => {
                log::error!("Config update: NVS open failed: {}", e);
                return;
            }
        };

        if let Some(host) = update.syslog_host.as_deref() {
            if host.is_empty() {
                if let Err(e) = crate::config::persist_str_opt(&mut nvs, "syslog_host", None) {
                    log::error!("Config update: persist syslog_host failed: {}", e);
                }
                crate::udp_log::disable_udp();
            } else {
                if let Err(e) =
                    crate::config::persist_str_opt(&mut nvs, "syslog_host", Some(host))
                {
                    log::error!("Config update: persist syslog_host failed: {}", e);
                }
                crate::udp_log::enable_udp(host, &self.config.device_id);
            }
        }

        if let Some(tz) = update.tz.as_deref() {
            if !tz.is_empty() {
                if let Err(e) = crate::config::persist_str_opt(&mut nvs, "tz", Some(tz)) {
                    log::error!("Config update: persist tz failed: {}", e);
                }
                if let Err(e) = crate::wifi::set_timezone(tz) {
                    log::warn!("Config update: apply tz failed: {}", e);
                }
            }
        }
    }
}

const MAX_CONSECUTIVE_FAILURES: u32 = 60;

/// Network thread loop. Polls the instruction source and updates shared state.
fn network_loop(
    source: &mut dyn InstructionSource,
    shared: &Arc<Mutex<SharedState>>,
    wifi: Option<SharedWifi>,
) {
    // Track the last instruction we pushed so repeated identical polls don't
    // restart the active widget every interval. The server is expected to
    // return the *current* desired state on each poll, not a diff.
    let mut last_served: Option<WidgetInstruction> = None;
    let mut last_config: Option<ConfigUpdate> = None;
    let mut consecutive_failures: u32 = 0;

    loop {
        match source.next_instruction() {
            Ok(Some(resp)) => {
                consecutive_failures = 0;
                let mut state = shared.lock().unwrap();

                if let Some(instruction) = resp.instruction {
                    let changed = last_served.as_ref() != Some(&instruction);
                    if changed {
                        log::info!("Instruction changed: {:?}", instruction);
                        state.pending_instruction = Some(instruction.clone());
                        // Wake the active widget so the display loop can
                        // pick up the new instruction.
                        state.current_cancel.cancel();
                        last_served = Some(instruction);
                    }
                }

                if let Some(brightness) = resp.brightness {
                    state.pending_brightness = Some(brightness);
                }

                if let Some(config) = resp.config {
                    // Dedup so we don't rewrite NVS every poll — the server is
                    // expected to keep returning the current desired config.
                    if last_config.as_ref() != Some(&config) {
                        log::info!("Config update received: {:?}", config);
                        state.pending_config = Some(config.clone());
                        last_config = Some(config);
                    }
                }
            }
            Ok(None) => {
                consecutive_failures = 0;
            }
            Err(e) => {
                consecutive_failures += 1;
                log::warn!(
                    "Instruction fetch failed ({}/{}): {}",
                    consecutive_failures, MAX_CONSECUTIVE_FAILURES, e
                );

                // If WiFi dropped (router rebooted, AP out of range), reconnect
                // here so we recover in one poll interval instead of waiting
                // for the MAX_CONSECUTIVE_FAILURES reboot escape hatch. If the
                // reconnect itself fails we just keep counting toward reboot.
                if let Some(ref wifi) = wifi {
                    let mut guard = wifi.lock().unwrap();
                    match crate::wifi::reconnect_if_down(&mut guard) {
                        Ok(true) => {
                            log::info!("WiFi restored — resetting failure counter");
                            consecutive_failures = 0;
                        }
                        Ok(false) => {} // still connected, poll failure was something else
                        Err(e) => log::warn!("Reconnect attempt failed: {}", e),
                    }
                }

                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    log::error!("Too many consecutive failures, rebooting...");
                    unsafe { esp_idf_svc::sys::esp_restart(); }
                }
            }
        }

        let interval = source.recommended_interval();
        std::thread::sleep(interval);
    }
}
