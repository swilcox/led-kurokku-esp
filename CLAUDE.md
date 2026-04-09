# LED Kurokku ESP

## Overview

Rust (esp-idf-hal std) firmware for ESP32-C3 driving LED displays. Fetches display instructions from a server via HTTP polling. Sister project to `led-kurokku-go` (Raspberry Pi version).

Supports:
- **MAX7219** — 32x8 pixel matrix (SPI, 4 daisy-chained chips)
- **TM1637** — 4-digit 7-segment (GPIO bit-bang) — driver not yet implemented

Single display per device, selected via cargo feature.

## Build

Requires nightly Rust toolchain with `rust-src` component (configured in `rust-toolchain.toml`).

```bash
# WiFi credentials are required at compile time (or stored in NVS)
KUROKKU_WIFI_SSID="MyNetwork" KUROKKU_WIFI_PASSWORD="secret" cargo build

# Optional env vars:
# KUROKKU_SERVER_URL (default: http://192.168.1.100:8080)
# KUROKKU_DEVICE_ID  (default: esp32-001)

# Release build
KUROKKU_WIFI_SSID="MyNetwork" KUROKKU_WIFI_PASSWORD="secret" cargo build --release

# Flash and monitor
espflash flash target/riscv32imc-esp-espidf/release/led-kurokku-esp --monitor

# Feature flags
cargo build --features max7219   # default
cargo build --features tm1637    # 7-segment display
```

## Architecture

### Two-Thread Engine Model

- **Network thread**: polls server for instructions via `InstructionSource` trait, updates shared state
- **Display thread**: runs widgets, checks for new instructions between frames

Communication: `Arc<Mutex<SharedState>>` between threads.

### Display Trait Hierarchy

Mirrors `led-kurokku-go`:

- **Display** — base: `init`, `close`, `clear`, `set_brightness` (always 0-15; backends map to native range)
- **PixelDisplay** — extends Display: `write_framebuffer(&[u8; 32])`, `width`, `height`
- **SegmentDisplay** — extends Display: `write_segments(&[u16], colon)`, `display_length`

`AnyDisplay` enum wraps either type for runtime dispatch.

### Modules

- **`config`** — `AppConfig` loaded from NVS with compile-time env var fallbacks. `open_nvs()` opens the `kurokku` NVS namespace.
- **`display/`** — Trait definitions + drivers. `max7219.rs` (SPI), `tm1637.rs` (planned).
- **`engine`** — Two-thread display loop. Network thread feeds `SharedState`, display thread runs widgets. Falls back to clock on error/disconnect.
- **`font`** — 5x7 ASCII bitmap font (columns, bit 0 = top row). `render_text()` joins glyphs with 1-column gaps.
- **`framebuf`** — `Frame = [u8; 32]`, column-major. `blit_text`, `blit_text_centered`, `set_pixel`.
- **`network/`** — `InstructionSource` trait (polling now, WebSocket later). `polling.rs` implements HTTP GET polling. `WidgetInstruction` enum for server commands.
- **`ota`** — `perform_ota(url)` downloads firmware, flashes inactive OTA partition, reboots. Uses ESP-IDF's built-in two-OTA partition table.
- **`widget/`** — `Widget` trait (`name`, `run`). `CancelToken` for cooperative cancellation. `sleep_or_cancel` polls at 50ms granularity.
  - `clock` — 24h/12h with AM/PM double-blink pattern (ported from Go)
  - `message` — scrolling text across pixel display
  - `raw_render` — dumb renderer: server sends pixel/segment data directly
  - `status` — shows IP address, errors, startup messages (scrolls if > 32px)
- **`wifi`** — `connect()` blocks until WiFi + IP. `sync_ntp()` syncs system clock.

### Server API Contract

```
GET /api/v1/devices/{device_id}/instruction?display_type=max7219
```
```json
{
  "instruction": { "type": "clock", "format_24h": true },
  "brightness": 8,
  "poll_interval_ms": 5000
}
```

Instruction types: `clock`, `message`, `raw_pixel`, `raw_segment`, `ota`.

### InstructionSource Trait

```rust
pub trait InstructionSource: Send {
    fn next_instruction(&mut self) -> Result<Option<ServerResponse>>;
    fn recommended_interval(&self) -> Duration;
}
```

`HttpPoller` implements this now. Future `WebSocketSource` would block on receive in `next_instruction()` and return `Duration::ZERO` for interval.

## Key Patterns

- Graceful degradation: clock widget runs if server unreachable or WiFi down
- NVS-first config: runtime values override compile-time defaults
- OTA triggered by server instruction — engine shows "OTA..." then downloads/reboots
- `EspError` doesn't impl `std::error::Error` — use `.map_err(|e| anyhow::anyhow!(...))` for `?` operator
- ESP32-C3 SPI pins default: GPIO6 (SCLK), GPIO7 (MOSI), GPIO10 (CS) — adjust to match wiring
- Startup sequence: display "KUROKKU" → WiFi → show IP → NTP → engine

## Dependencies

- `esp-idf-svc` — WiFi, HTTP, NVS, OTA, SNTP, SPI, GPIO (wraps ESP-IDF v5.2)
- `serde` + `serde_json` — JSON parsing for server responses
- `anyhow` — error handling
- `log` — logging (backed by `EspLogger`)
- `time` — date/time formatting for clock widget
