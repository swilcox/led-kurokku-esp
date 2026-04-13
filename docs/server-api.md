# Server API

The device polls a server for display instructions. This document describes the API contract between the device and the server.

You'll need a running instance of the [led-kurokku-go](https://github.com/swilcox/led-kurokku-go) server (or any server that implements this API).

## Endpoint

```
GET /api/v1/devices/{device_id}/instruction?display_type=max7219
```

The device sends its `device_id` (from config) and `display_type` (from the active cargo feature) as part of the request. The server uses these to decide what to display.

## Response Format

```json
{
  "instruction": { ... },
  "brightness": 8,
  "poll_interval_ms": 5000
}
```

All top-level fields are optional:

| Field | Type | Description |
|-------|------|-------------|
| `instruction` | object | What to display (see below) |
| `brightness` | integer (0-15) | Override display brightness |
| `poll_interval_ms` | integer | How often the device should poll, in ms |

If no `instruction` is provided, the device keeps its current display. If the device has never received an instruction, it shows a clock.

## Instruction Types

### clock

Display the current time with a blinking colon separator.

```json
{
  "type": "clock",
  "format_24h": true
}
```

| Field | Default | Description |
|-------|---------|-------------|
| `format_24h` | `true` | `true` for 24-hour format, `false` for 12-hour with AM/PM blink pattern |

### message

Display text on the screen. Short text is centered; longer text scrolls automatically.

```json
{
  "type": "message",
  "text": "Hello!",
  "scroll_speed_ms": 50,
  "repeats": 3
}
```

| Field | Default | Description |
|-------|---------|-------------|
| `text` | (required) | The text to display |
| `scroll_speed_ms` | `50` | Milliseconds between scroll steps (lower = faster) |
| `repeats` | `1` | Number of times to scroll the message. Use `-1` for infinite |

After all repeats finish, the device reverts to showing a clock.

### animation

Play a visual animation on the pixel display.

```json
{
  "type": "animation",
  "animation": "pong",
  "duration_secs": 30
}
```

| Field | Default | Description |
|-------|---------|-------------|
| `animation` | (required) | One of: `static`, `pong`, `matrix` (or `matrix_rain`) |
| `duration_secs` | `30` | How long to run in seconds. Use `0` for infinite |

Available animations:

- **`static`** — TV noise / random pixels
- **`pong`** — bouncing ball with AI-controlled paddles
- **`matrix`** / **`matrix_rain`** — falling column effect

After the duration expires, the device reverts to showing a clock. Unknown animation names default to `static`.

### raw_pixel

Send raw pixel data directly to the display. Useful for custom graphics or external rendering.

```json
{
  "type": "raw_pixel",
  "data": [0, 0, 255, 128, 64, 32, 16, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
}
```

| Field | Description |
|-------|-------------|
| `data` | Array of 32 bytes, one per column. Each byte's bits control the 8 rows in that column (bit 0 = top row) |

The display stays on this content indefinitely until the next instruction arrives.

### raw_segment

Send raw segment data to a 7-segment display (TM1637).

```json
{
  "type": "raw_segment",
  "segments": [119, 63, 6, 91],
  "colon": true
}
```

| Field | Default | Description |
|-------|---------|-------------|
| `segments` | (required) | Array of u16 bitmasks, one per digit |
| `colon` | `false` | Whether to light the colon separator |

### ota

Trigger an over-the-air firmware update. The device will download the firmware binary, flash it to the inactive OTA partition, and reboot.

```json
{
  "type": "ota",
  "url": "https://example.com/firmware.bin"
}
```

| Field | Description |
|-------|-------------|
| `url` | URL to the firmware binary (.bin file) |

During the update, the display shows "OTA...". On failure, it shows "OTA FAIL" and reverts to the clock.

## Example: Sending Instructions with curl

If you're building your own server or just testing, you can see how the device interprets responses by returning the right JSON from your endpoint.

Here's what the device expects back from a simple server:

```bash
# The device will GET this endpoint:
# GET /api/v1/devices/esp32-001/instruction?display_type=max7219

# A minimal response showing a message:
{
  "instruction": {
    "type": "message",
    "text": "Hello World!"
  }
}

# Just change brightness without changing what's displayed:
{
  "brightness": 12
}

# Show clock at low brightness with slower polling:
{
  "instruction": {
    "type": "clock",
    "format_24h": false
  },
  "brightness": 2,
  "poll_interval_ms": 10000
}
```
