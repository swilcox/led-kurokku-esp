# LED Kurokku ESP

## Overview

Rust (esp-idf-hal std) firmware for ESP32-C3 driving LED displays. Fetches display instructions from a server via HTTP polling. Sister project to `led-kurokku-go` (Raspberry Pi version).

Supports:
- **MAX7219** — 32x8 pixel matrix (SPI, 4 daisy-chained chips)
- **TM1637** — 4-digit 7-segment (GPIO bit-bang, custom 2-wire protocol)

Single display per device, selected via cargo feature.

## Build

Requires nightly Rust toolchain with `rust-src` component (configured in `rust-toolchain.toml`). Uses `ldproxy` linker and `build-std` for `riscv32imc-esp-espidf` target (configured in `.cargo/config.toml`).

Firmware binaries are **generic**: no WiFi credentials, server URLs, or device IDs are baked into the `.bin`. All per-device config lives in NVS and is provisioned via `tools/provision.py` before first boot. This keeps a single signed OTA image safely distributable across every device on the fleet.

```bash
just build            # debug build
just build-release    # release build
just flash            # flash release + monitor
just deploy           # build-release then flash

# Feature flags
cargo build --features max7219   # default
cargo build --features tm1637    # 7-segment display
```

### NVS Configuration Keys

All config is loaded from the `kurokku` NVS namespace on boot. Unset keys fall back to placeholder defaults in `src/config.rs` — the device will boot but refuse to join WiFi (`has_wifi_config()` returns false) until provisioned.

| NVS Key | Type | Default | Description |
|---------|------|---------|-------------|
| `wifi_ssid` | string | (empty) | WiFi network name — required |
| `wifi_pass` | string | (empty) | WiFi password — required |
| `server_url` | string | (empty) | Instruction server base URL — required |
| `device_id` | string | `unprovisioned` | Device identifier for server API |
| `tz` | string | `UTC0` | POSIX TZ string for local time |
| `syslog_host` | string | (unset) | UDP syslog target as `host:port` |
| `format_24h` | u8 (0/1) | `1` | 24-hour clock format |
| `brightness` | u8 | `4` | Default display brightness 0-15 |
| `poll_ms` | i32 | `5000` | Server poll interval in ms |
| `log_level` | string | `info` | Max log level: `off`/`error`/`warn`/`info`/`debug`/`trace` |

### Provisioning a Device

Per-device values live in `tools/devices/<device-id>.yaml` (gitignored except the template). See `tools/devices/example.yaml`.

```bash
# Copy template, edit, then flash NVS
cp tools/devices/example.yaml tools/devices/kitchen-01.yaml
# ...edit kitchen-01.yaml...
just provision kitchen-01                  # auto-detect serial port
just provision kitchen-01 /dev/ttyUSB0     # explicit port
```

Python dependencies are declared via PEP 723 inline script metadata in `tools/provision.py` and resolved automatically by `uv run` into a cached env — no manual install step. Requires `uv` (https://docs.astral.sh/uv/) on the host.

The provisioning script reads the YAML, generates an NVS partition image with `nvs_partition_gen` (from the `esp-idf-nvs-partition-gen` package), and writes it to the NVS partition offset via `espflash write-bin`. The NVS partition is preserved across OTA updates, so you only provision once per device.

**Caveat**: The script overwrites the entire NVS partition, which also clears ESP-IDF's own WiFi RF calibration data. The device re-calibrates on next boot — harmless but mentioned for awareness. A future captive-portal mode will update individual keys without wiping.

### Build Tooling Files

- **`Justfile`** — build recipes; run `just` to list available commands
- **`rust-toolchain.toml`** — pins nightly channel, includes `rust-src` component
- **`.cargo/config.toml`** — sets target to `riscv32imc-esp-espidf`, configures `ldproxy` linker, `build-std`, and `ESP_IDF_SDKCONFIG_DEFAULTS`
- **`sdkconfig.defaults`** — ESP-IDF Kconfig overrides: 8KB main task stack, 1000Hz FreeRTOS tick (1ms sleep granularity), 4MB flash, custom partition table (`partitions.csv`), full TLS certificate bundle for HTTPS OTA
- **`build.rs`** — exports `KUROKKU_GIT_HASH` (short hash + `-dirty` suffix if tree is modified) so firmware can report its version to the server

### Hardware Wiring (MAX7219)

ESP32-C3 to MAX7219 (4 daisy-chained 8x8 modules):

| ESP32-C3 Pin | MAX7219 Pin | Function |
|-------------|-------------|----------|
| GPIO6 | CLK | SPI clock (SCLK) |
| GPIO7 | DIN | SPI data (MOSI) |
| GPIO10 | CS | Chip select (active low) |
| 5V | VCC | Power |
| GND | GND | Ground |

### Hardware Wiring (TM1637)

ESP32-C3 to TM1637 (4-digit 7-segment module):

| ESP32-C3 Pin | TM1637 Pin | Function |
|-------------|-----------|----------|
| GPIO4 | CLK | Bit-bang clock |
| GPIO5 | DIO | Bit-bang data |
| 3V3 or 5V | VCC | Power (most modules accept either) |
| GND | GND | Ground |

Modules typically include on-board pull-ups on CLK and DIO. Both lines are driven push-pull by the firmware; the ACK slot is released high without sampling (matches the Go/Python sister projects and works on all tested modules).

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

- **`config`** — `AppConfig` loaded exclusively from NVS; placeholder defaults for missing keys. `open_nvs()` opens the `kurokku` NVS namespace. Provisioned via `tools/provision.py`.
- **`display/`** — Trait definitions + drivers. `max7219.rs` (SPI), `tm1637.rs` (bit-banged 2-wire; 5µs bit delay via `Ets::delay_us`).
- **`engine`** — Two-thread display loop. Network thread feeds `SharedState`, display thread runs widgets. Cancel token shared between threads: network thread cancels current widget on instruction change, display thread installs fresh token when starting each widget. Network loop deduplicates repeated identical polls via `last_served` tracking. Falls back to clock on error/widget completion. A remote `config` field (deduped via `last_config`) is applied by the display thread through `apply_config_update` — it persists changed keys to NVS and applies syslog/timezone/log-level live, without swapping the active widget. The network thread also emits a periodic `telemetry:` log line (WiFi RSSI + CPU die temperature, every 60s) via `log_telemetry` — health signal that flows to syslog when enabled.
- **`font`** — 5x7 ASCII bitmap font (columns, bit 0 = top row). `render_text()` joins glyphs with 1-column gaps.
- **`font_7seg`** — character → 7-segment bitmask table (u8). `encode(char)`, `digit(u8)`, `encode_text(&str)`. Table ported from Go `segfont.Seg7` / Python `SEGMENTS`. `encode_text` folds `.` into the previous digit's DP bit (0x80).
- **`framebuf`** — `Frame = [u8; 32]`, column-major. `blit_text`, `blit_text_centered`, `set_pixel`.
- **`network/`** — `InstructionSource` trait (polling now, WebSocket later). `polling.rs` implements HTTP GET polling. `WidgetInstruction` enum for server commands.
- **`ota`** — `perform_ota(url)` downloads firmware, flashes inactive OTA partition, reboots. Uses ESP-IDF's built-in two-OTA partition table.
- **`temp_sensor`** — Thin wrapper over ESP-IDF's `temperature_sensor` driver (raw `sys` bindings; not wrapped by `esp-idf-svc`). `TempSensor::new()` installs/enables the on-die sensor; `read_celsius()` reads it. Used for telemetry.
- **`widget/`** — `Widget` trait (`name`, `run`). `CancelToken` for cooperative cancellation. `sleep_or_cancel` polls at 50ms granularity. Each widget dispatches on `AnyDisplay` to render on pixel or segment backends.
  - `clock` — 24h/12h with AM/PM double-blink pattern. Segment variant renders digits via `font_7seg::digit` with leading blank in 12h mode.
  - `message` — pixel: static centered if ≤32px wide, scrolling otherwise. Segment: static if fits in display length, otherwise window scroll (pad `width` blanks each side, slide 1 char per tick).
  - `animation` — pixel: `static` (TV noise), `pong` (bouncing ball), `matrix_rain` (falling columns). Segment: `static` (random segments), `pong` (vertical-bar "ball" bouncing L/R across each digit in turn: left verts → right verts → next digit), `matrix_rain` (per-side raindrops per digit, top-vert → bottom-vert → bottom-horizontal, multiple concurrent drops).
  - `raw_render` — dumb renderer: server sends pixel/segment data directly.
  - `status` — shows IP address, errors, startup messages. Pixel scrolls if > 32px; segment scrolls if > display length (250ms cadence).
- **`udp_log`** — Optional UDP syslog (RFC5424). Composite logger wraps `EspLogger` (serial) + UDP sink. `init()` replaces `EspLogger::initialize_default()`. `enable_udp(host, device_id)` called after WiFi connects if `syslog_host` is configured. The target lives in a `RwLock<Option<…>>` so it can be retargeted at runtime via a remote `config` update; `disable_udp()` clears it. Fire-and-forget, non-blocking.
- **`wifi`** — `connect()` blocks until WiFi + IP. `sync_ntp()` returns SNTP handle for periodic re-sync.

### Server API Contract

```
GET /api/v1/devices/{device_id}/instruction?display_type=max7219&firmware_version=0.1.0%2Babc1234
```

`display_type` is set by the firmware based on the active cargo feature: `max7219` or `tm1637`. `firmware_version` combines the Cargo package version with the short git hash from `build.rs` (suffixed `-dirty` when the working tree had uncommitted changes at build time); the `+` separator is percent-encoded as `%2B` on the wire so the server sees the decoded form `0.1.0+abc1234`. The server can use this to gate OTAs or spot stale devices.

Response envelope:

```json
{
  "instruction": { ... },
  "brightness": 8,
  "poll_interval_ms": 5000,
  "config": { "syslog_host": "192.168.1.50:5514", "tz": "CST6CDT,M3.2.0,M11.1.0" }
}
```

All top-level fields are optional. `brightness` (0-15) overrides display brightness. `poll_interval_ms` adjusts the polling interval for subsequent requests.

`config` remotely updates persisted NVS settings without re-provisioning over serial — a side channel applied alongside whatever `instruction` is showing (it does **not** swap the active widget). Supported keys:

- `syslog_host` — UDP syslog target as `host:port`. Empty string `""` disables syslog. Applied live via `udp_log::enable_udp`/`disable_udp`.
- `tz` — POSIX TZ string. Applied live via `setenv`/`tzset`.
- `log_level` — max log level (`off`/`error`/`warn`/`info`/`debug`/`trace`). Empty string is ignored; unrecognized values fall back to `info`. Applied live via `log::set_max_level`.

Omitted keys are left unchanged. All keys are persisted to NVS so they survive reboot. The device deduplicates against the last-applied `config`, so the server can keep returning the same block on every poll without churning NVS flash (a write only happens when the value actually changes, and ESP-IDF NVS itself skips identical writes). This is the only way to change logging/timezone/log-level on a deployed device short of serial re-provisioning, since OTA replaces only the firmware image and preserves the NVS partition.

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

**animation** — visual animation:
```json
{ "type": "animation", "animation": "pong", "duration_secs": 30 }
```
`animation` values: `static` (TV noise), `pong` (bouncing ball with AI paddles), `matrix` or `matrix_rain` (falling columns), `snake` (AI-driven snake chasing food on pixel; segment runs a 3-segment trail around each digit's outer perimeter — a,b,c,d,e,f,a moving right / a,f,e,d,c,b,a moving left — bouncing off the first/last digit, which drops the closing 'a'), `curtain` (pixel: fill one side → unfill same side; segment: fill digits left-to-right in 3 stages each — `e,f` → `a,d,e,f,g` → all seven — with completed digits staying lit, then unfill left-to-right peeling segments in the same order they were laid down — `a,b,c,d,g` → `b,c` → off — while digits to the right remain fully lit), `sine` / `sine_wave` / `sinewave` (pixel: scrolling sine curve; segment: one extra segment accrues across all digits per step in a,b,c,d,e,f,g order, then unrolls in the same order), `random` (pick one variant at widget start from the other six). Unknown values default to `static`. `duration_secs` defaults to `30`; use `0` for infinite (runs until next instruction). After duration, reverts to clock.

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
- Startup sequence: display "LED クロック" (pixel) / "KUROKKU" (7-segment) → WiFi → show IP → NTP → engine

## Dependencies

- `esp-idf-svc` — WiFi, HTTP, NVS, OTA, SNTP, SPI, GPIO (wraps ESP-IDF v5.2). **Must include `binstart` feature** when using `default-features = false`, or Rust `main()` is never bridged to ESP-IDF's `app_main` and nothing runs.
- `serde` + `serde_json` — JSON parsing for server responses
- `anyhow` — error handling
- `log` — logging (backed by `EspLogger`)

Clock widget reads local time via newlib `localtime_r`, which honors the `TZ` env var set at startup from `AppConfig::tz`. The TZ string uses POSIX format (e.g. `CST6CDT,M3.2.0,M11.1.0` for US Central with DST). Applied via `setenv("TZ", ...)`/`tzset()` after NTP sync.
