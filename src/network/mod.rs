pub mod polling;

use anyhow::Result;
use serde::Deserialize;
use std::time::Duration;

/// Instruction from the server describing what to display.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum WidgetInstruction {
    #[serde(rename = "clock")]
    Clock {
        #[serde(default = "default_true")]
        format_24h: bool,
    },

    #[serde(rename = "message")]
    Message {
        text: String,
        #[serde(default = "default_scroll_speed")]
        scroll_speed_ms: u64,
        #[serde(default = "default_repeats")]
        repeats: i32,
    },

    #[serde(rename = "raw_pixel")]
    RawPixel {
        /// Base64-encoded 32 bytes, or array of 32 ints
        data: Vec<u8>,
    },

    #[serde(rename = "raw_segment")]
    RawSegment {
        segments: Vec<u16>,
        #[serde(default)]
        colon: bool,
    },

    #[serde(rename = "animation")]
    Animation {
        animation: String,
        #[serde(default = "default_animation_duration")]
        duration_secs: u64,
    },

    #[serde(rename = "ota")]
    Ota { url: String },
}

fn default_true() -> bool { true }
fn default_scroll_speed() -> u64 { 50 }
fn default_repeats() -> i32 { 1 }
fn default_animation_duration() -> u64 { 30 }

/// Remote configuration update delivered alongside an instruction.
///
/// Lets the server change persisted NVS settings without re-provisioning a
/// device over serial. Fields omitted from the JSON are left unchanged; an
/// empty `syslog_host` string disables syslog. Updates are deduplicated by
/// the device, so the server can keep returning the same `config` on every
/// poll without churning NVS flash.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct ConfigUpdate {
    /// UDP syslog target as "host:port". Empty string disables syslog.
    #[serde(default)]
    pub syslog_host: Option<String>,
    /// POSIX TZ string, e.g. "CST6CDT,M3.2.0,M11.1.0".
    #[serde(default)]
    pub tz: Option<String>,
}

/// Full server response.
#[derive(Deserialize, Debug, Clone)]
pub struct ServerResponse {
    pub instruction: Option<WidgetInstruction>,
    pub brightness: Option<u8>,
    pub poll_interval_ms: Option<u64>,
    pub config: Option<ConfigUpdate>,
}

/// Trait for fetching instructions from a server.
/// Implementations can be HTTP polling, WebSocket, MQTT, etc.
pub trait InstructionSource: Send {
    /// Fetch the next instruction. Blocks until one is available or timeout.
    /// Returns None if no new instruction (e.g. server returned empty, or poll unchanged).
    fn next_instruction(&mut self) -> Result<Option<ServerResponse>>;

    /// How long the engine should wait before calling next_instruction again.
    /// Polling sources return their poll interval; push sources can return Duration::MAX
    /// since next_instruction will block until a message arrives.
    fn recommended_interval(&self) -> Duration;
}
