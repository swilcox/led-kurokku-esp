# LED Kurokku ESP

## Overview

Rust (esp-idf-hal std) firmware for ESP32-C3 driving LED displays. Fetches display instructions from a server via HTTP polling. Sister project to `led-kurokku-go` (Raspberry Pi version).

Supports:
- **MAX7219** — 32x8 pixel matrix (SPI, 4 daisy-chained chips)
- **TM1637** — 4-digit 7-segment (GPIO bit-bang) — driver not yet implemented

Single display per device, selected via cargo feature.

## Build

Requires nightly Rust toolchain with `rust-src` component (configured in `rust-toolchain.toml`). Uses `ldproxy` linker and `build-std` for `riscv32imc-esp-espidf` target (configured in `.cargo/config.toml`).

```bash
# 1. Copy .env.example to .env and fill in your values
cp .env.example .env

# 2. Use just recipes (loads .env automatically)
just build            # debug build
just build-release    # release build
just flash            # flash release + monitor
just deploy           # build-release then flash

# Or manually with env vars
KUROKKU_WIFI_SSID="MyNetwork" KUROKKU_WIFI_PASSWORD="secret" cargo build

# Feature flags
cargo build --features max7219   # default
cargo build --features tm1637    # 7-segment display
```

### Compile-Time Environment Variables

All are optional if the corresponding NVS key is set at runtime.

| Env Var | NVS Key | Default | Description |
|---------|---------|---------|-------------|
| `KUROKKU_WIFI_SSID` | `wifi_ssid` | (empty) | WiFi network name |
| `KUROKKU_WIFI_PASSWORD` | `wifi_pass` | (empty) | WiFi password |
| `KUROKKU_SERVER_URL` | `server_url` | `http://192.168.1.100:8080` | Instruction server base URL |
| `KUROKKU_DEVICE_ID` | `device_id` | `esp32-001` | Device identifier for server API |
| `KUROKKU_TZ` | `tz` | `CST6CDT,M3.2.0,M11.1.0` | POSIX TZ string for local time |
| — | `format_24h` | `1` (true) | 24-hour clock format (NVS-only, u8: 0 or 1) |
| — | `brightness` | `4` | Default display brightness 0-15 (NVS-only) |
| — | `poll_ms` | `5000` | Server poll interval in ms (NVS-only, stored as i32) |

NVS values take priority over compile-time env vars. NVS namespace: `kurokku`.

### Build Tooling Files

- **`Justfile`** — build recipes with `dotenv-load`; run `just` to list available commands
- **`.env`** — compile-time env vars (gitignored); copy from `.env.example`
- **`rust-toolchain.toml`** — pins nightly channel, includes `rust-src` component
- **`.cargo/config.toml`** — sets target to `riscv32imc-esp-espidf`, configures `ldproxy` linker, `build-std`, and `ESP_IDF_SDKCONFIG_DEFAULTS`
- **`sdkconfig.defaults`** — ESP-IDF Kconfig overrides: 8KB main task stack, 1000Hz FreeRTOS tick (1ms sleep granularity), 4MB flash, two-OTA partition table, full TLS certificate bundle for HTTPS OTA

### Hardware Wiring (MAX7219)

ESP32-C3 to MAX7219 (4 daisy-chained 8x8 modules):

| ESP32-C3 Pin | MAX7219 Pin | Function |
|-------------|-------------|----------|
| GPIO6 | CLK | SPI clock (SCLK) |
| GPIO7 | DIN | SPI data (MOSI) |
| GPIO10 | CS | Chip select (active low) |
| 5V | VCC | Power |
| GND | GND | Ground |

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
- **`engine`** — Two-thread display loop. Network thread feeds `SharedState`, display thread runs widgets. Cancel token shared between threads: network thread cancels current widget on instruction change, display thread installs fresh token when starting each widget. Network loop deduplicates repeated identical polls via `last_served` tracking. Falls back to clock on error/widget completion.
- **`font`** — 5x7 ASCII bitmap font (columns, bit 0 = top row). `render_text()` joins glyphs with 1-column gaps.
- **`framebuf`** — `Frame = [u8; 32]`, column-major. `blit_text`, `blit_text_centered`, `set_pixel`.
- **`network/`** — `InstructionSource` trait (polling now, WebSocket later). `polling.rs` implements HTTP GET polling. `WidgetInstruction` enum for server commands.
- **`ota`** — `perform_ota(url)` downloads firmware, flashes inactive OTA partition, reboots. Uses ESP-IDF's built-in two-OTA partition table.
- **`widget/`** — `Widget` trait (`name`, `run`). `CancelToken` for cooperative cancellation. `sleep_or_cancel` polls at 50ms granularity.
  - `clock` — 24h/12h with AM/PM double-blink pattern (ported from Go)
  - `message` — text display: static centered if ≤32px wide, scrolling otherwise
  - `animation` — visual animations: `static` (TV noise), `pong` (bouncing ball), `matrix_rain` (falling columns)
  - `raw_render` — dumb renderer: server sends pixel/segment data directly
  - `status` — shows IP address, errors, startup messages (scrolls if > 32px)
- **`wifi`** — `connect()` blocks until WiFi + IP. `sync_ntp()` returns SNTP handle for periodic re-sync.

### Server API Contract

```
GET /api/v1/devices/{device_id}/instruction?display_type=max7219
```

Response envelope:

```json
{
  "instruction": { ... },
  "brightness": 8,
  "poll_interval_ms": 5000
}
```

All top-level fields are optional. `brightness` (0-15) overrides display brightness. `poll_interval_ms` adjusts the polling interval for subsequent requests.

#### Instruction Types

**clock** — display current time with blinking colon:
```json
{ "type": "clock", "format_24h": true }
```
`format_24h` defaults to `true`. In 12h mode, PM hours use a double-blink colon pattern.

**message** — display text (static if ≤32px wide, scrolling otherwise):
```json
{ "type": "message", "text": "Hello!", "scroll_speed_ms": 50, "repeats": 3 }
```
`scroll_speed_ms` defaults to `50`. `repeats` defaults to `1`; use `-1` for infinite. After all repeats, reverts to clock.

**animation** — visual animation on pixel display:
```json
{ "type": "animation", "animation": "pong", "duration_secs": 30 }
```
`animation` values: `static` (TV noise), `pong` (bouncing ball with AI paddles), `matrix` or `matrix_rain` (falling columns). Unknown values default to `static`. `duration_secs` defaults to `30`; use `0` for infinite (runs until next instruction). After duration, reverts to clock.

**raw_pixel** — direct framebuffer control (32 bytes, one per column):
```json
{ "type": "raw_pixel", "data": [0, 0, 255, 128, ...] }
```
`data` is 32 bytes (column-major, bit 0 = top row). Displays indefinitely until next instruction.

**raw_segment** — direct 7-segment control:
```json
{ "type": "raw_segment", "segments": [119, 63, 6, 91], "colon": true }
```
`segments` is an array of u16 bitmasks. `colon` defaults to `false`.

**ota** — trigger over-the-air firmware update:
```json
{ "type": "ota", "url": "https://example.com/firmware.bin" }
```
Device downloads firmware, flashes inactive OTA partition, and reboots. On failure, shows "OTA FAIL" and reverts to clock.

### InstructionSource Trait

```rust
pub trait InstructionSource: Send {
    fn next_instruction(&mut self) -> Result<Option<ServerResponse>>;
    fn recommended_interval(&self) -> Duration;
}
```

`HttpPoller` implements this now. Future `WebSocketSource` would block on receive in `next_instruction()` and return `Duration::ZERO` for interval.

## Key Patterns

- Graceful degradation: clock widget runs if server unreachable or WiFi down. Network thread reboots after 60 consecutive poll failures (~5 min).
- NTP periodic re-sync: SNTP handle kept alive for automatic hourly re-sync via ESP-IDF
- NVS-first config: runtime values override compile-time defaults
- OTA triggered by server instruction — engine shows "OTA..." then downloads/reboots
- `EspError` doesn't impl `std::error::Error` — use `.map_err(|e| anyhow::anyhow!(...))` for `?` operator
- ESP32-C3 SPI pins default: GPIO6 (SCLK), GPIO7 (MOSI), GPIO10 (CS) — adjust to match wiring
- Startup sequence: display "KUROKKU" → WiFi → show IP → NTP → engine

## Dependencies

- `esp-idf-svc` — WiFi, HTTP, NVS, OTA, SNTP, SPI, GPIO (wraps ESP-IDF v5.2). **Must include `binstart` feature** when using `default-features = false`, or Rust `main()` is never bridged to ESP-IDF's `app_main` and nothing runs.
- `serde` + `serde_json` — JSON parsing for server responses
- `anyhow` — error handling
- `log` — logging (backed by `EspLogger`)

Clock widget reads local time via newlib `localtime_r`, which honors the `TZ` env var set at startup from `AppConfig::tz`. The TZ string uses POSIX format (e.g. `CST6CDT,M3.2.0,M11.1.0` for US Central with DST). Applied via `setenv("TZ", ...)`/`tzset()` after NTP sync.
