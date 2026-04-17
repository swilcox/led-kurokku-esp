# Setup Guide

This guide walks you through everything you need to build and flash the LED Kurokku firmware onto an ESP32-C3, from installing tools to seeing pixels light up.

## 1. Software Prerequisites

### Rust Toolchain

Install Rust via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

The project's `rust-toolchain.toml` will automatically install the correct nightly toolchain and the `rust-src` component when you first build. No manual nightly setup needed.

### ESP-IDF Build Dependencies

The firmware uses `esp-idf-svc`, which downloads and builds the ESP-IDF SDK automatically during compilation. However, it needs some system packages installed first.

**macOS:**

```bash
brew install cmake ninja python3
```

**Linux (Debian/Ubuntu):**

```bash
sudo apt install git curl gcc ninja-build cmake python3 python3-venv libssl-dev \
  libffi-dev pkg-config libudev-dev
```

**Linux (Fedora):**

```bash
sudo dnf install git curl gcc ninja-build cmake python3 python3-pip openssl-devel \
  libffi-devel pkgconfig systemd-devel
```

> The first build will take a while (10+ minutes) as it downloads and compiles the ESP-IDF SDK. Subsequent builds are much faster.

### Flashing and Linking Tools

```bash
cargo install ldproxy espflash
```

- **ldproxy** — linker proxy required for ESP-IDF builds
- **espflash** — flashes firmware to the ESP32 and opens a serial monitor

### just (Command Runner)

[just](https://github.com/casey/just) is a command runner (like make) used for build recipes. Optional but recommended.

**macOS:**

```bash
brew install just
```

**Linux:**

```bash
cargo install just
```

Or see [other install methods](https://github.com/casey/just#installation).

### uv (Python Runner)

The device provisioning tool (`tools/provision.py`) is a single Python script that declares its own dependencies via [PEP 723 inline metadata](https://peps.python.org/pep-0723/). [uv](https://docs.astral.sh/uv/) resolves and caches those dependencies automatically — there is no `pip install` step.

**macOS:**

```bash
brew install uv
```

**Linux:**

```bash
curl -LsSf https://astral.sh/uv/install.sh | sh
```

## 2. Hardware

### Shopping List

| Component | Quantity | Notes |
|-----------|----------|-------|
| ESP32-C3 development board | 1 | Any board with USB-C and exposed GPIO pins. Common options: ESP32-C3-DevKitM-1, Seeed XIAO ESP32C3, WeAct ESP32-C3 |
| Display module | 1 | **Either** a MAX7219 4-in-1 matrix (32x8 pixels) **or** a TM1637 4-digit 7-segment clock module. Choose based on whether you want graphics + scrolling text or just a clock/numeric readout. |
| Jumper wires (female-to-female) | 4–5 | 4 for TM1637, 5 for MAX7219 |
| USB-C cable | 1 | For power and flashing |

You can find all of these on Amazon, AliExpress, or your preferred electronics supplier. The total cost is typically under $15.

### Wiring — MAX7219 (32x8 matrix)

Connect the ESP32-C3 to the MAX7219 module with 5 wires:

```
ESP32-C3          MAX7219 Module
─────────         ──────────────
GPIO6    ───────  CLK   (SPI clock)
GPIO7    ───────  DIN   (SPI data in)
GPIO10   ───────  CS    (chip select)
5V       ───────  VCC   (power)
GND      ───────  GND   (ground)
```

**Important notes:**

- The MAX7219 needs **5V** power. Most ESP32-C3 dev boards have a 5V pin that passes through USB power directly — use that, not the 3.3V pin.
- GPIO6, GPIO7, and GPIO10 are the default SPI pins for the ESP32-C3. If your board labels them differently, check the pinout diagram for your specific board.
- The MAX7219 module's DIN/CLK/CS labels are usually printed on the PCB. Make sure you connect to the **input** side (not the output side used for daisy-chaining additional modules).

### Wiring — TM1637 (4-digit 7-segment)

Connect the ESP32-C3 to the TM1637 module with 4 wires:

```
ESP32-C3          TM1637 Module
─────────         ─────────────
GPIO4    ───────  CLK   (bit-bang clock)
GPIO5    ───────  DIO   (bit-bang data)
3V3      ───────  VCC   (power; most modules also accept 5V)
GND      ───────  GND   (ground)
```

**Important notes:**

- The TM1637 is **not** I²C — it's a custom 2-wire protocol. The firmware bit-bangs both lines.
- Most TM1637 modules include on-board pull-up resistors on CLK and DIO, so no external pull-ups are needed.
- Both 3.3V and 5V typically work; start with 3.3V for safety.

## 3. Configuration

Firmware binaries are **generic** — no WiFi credentials, server URL, or device ID are baked in. All per-device configuration lives in NVS (Non-Volatile Storage) and is provisioned separately via a small Python script. This means a single signed OTA image can be distributed safely across every device in your fleet.

On an un-provisioned device, the firmware boots, shows `KUROKKU`, then displays `NO CFG` because no WiFi credentials are present.

### Create a Device Config File

Per-device values live in `tools/devices/<device-id>.yaml`. These files are gitignored; only `tools/devices/example.yaml` is checked in as a template.

```bash
cp tools/devices/example.yaml tools/devices/my-device.yaml
```

Edit `my-device.yaml`:

```yaml
# Required
device_id: my-device
wifi_ssid: MyNetwork
wifi_pass: correcthorsebatterystaple
server_url: http://192.168.1.100:8080
tz: CST6CDT,M3.2.0,M11.1.0

# Optional (omit to take firmware defaults)
# format_24h: true          # default true
# brightness: 4             # 0-15, default 4
# poll_ms: 5000             # default 5000
# syslog_host: 192.168.1.50:5514   # omit to disable UDP syslog
```

### Provision the Device

Connect the device via USB-C, then:

```bash
just provision my-device                  # auto-detect serial port
just provision my-device /dev/ttyUSB0     # or specify a port
```

The provisioning step writes to the NVS partition on the device. It's a one-time step per device — NVS is preserved across reboots and OTA updates. You only re-provision if credentials change.

> **Note:** The provisioner replaces the entire NVS partition, which also clears ESP-IDF's own WiFi RF calibration data. The device re-calibrates on next boot; this is harmless but mentioned for awareness. A future captive-portal mode will update individual keys without wiping.

### Timezone Format

The `tz` value in your device YAML uses the POSIX TZ format. Some common examples:

| Timezone | Value |
|----------|-------|
| US Eastern | `EST5EDT,M3.2.0,M11.1.0` |
| US Central | `CST6CDT,M3.2.0,M11.1.0` |
| US Mountain | `MST7MDT,M3.2.0,M11.1.0` |
| US Pacific | `PST8PDT,M3.2.0,M11.1.0` |
| UTC | `UTC0` |
| Central Europe | `CET-1CEST,M3.5.0,M10.5.0/3` |
| Japan | `JST-9` |
| Australia Eastern | `AEST-10AEDT,M10.1.0,M4.1.0/3` |

For other timezones, see [this POSIX TZ reference](https://www.gnu.org/software/libc/manual/html_node/TZ-Variable.html).

### NVS Keys Reference

All keys live in the `kurokku` NVS namespace. The provisioner populates them from your YAML; the firmware reads them on boot.

| NVS Key | Type | Default | Description |
|---------|------|---------|-------------|
| `wifi_ssid` | string | (empty) | WiFi network name — required |
| `wifi_pass` | string | (empty) | WiFi password — required |
| `server_url` | string | (empty) | Instruction server base URL — required |
| `device_id` | string | `unprovisioned` | Device identifier for server API |
| `tz` | string | `UTC0` | POSIX timezone string |
| `syslog_host` | string | (unset) | UDP syslog target as `host:port` |
| `format_24h` | u8 (0/1) | `1` | 24-hour clock (1) or 12-hour (0) |
| `brightness` | u8 | `4` | Default display brightness, 0-15 |
| `poll_ms` | i32 | `5000` | Server poll interval in milliseconds |

## 4. Building and Flashing

### Using just (Recommended)

```bash
# List all available commands
just

# --- MAX7219 (default) ---
just build              # debug
just build-release      # release
just deploy             # build release + flash

# --- TM1637 ---
just build-tm1637           # debug
just build-tm1637-release   # release
just deploy-tm1637          # build release + flash

# --- Tests (host, no ESP toolchain needed) ---
just test
```

### Manual Build

If you prefer not to use `just`:

```bash
# Build (MAX7219 is the default feature)
cargo build --release

# For TM1637 instead, disable the default feature:
# cargo build --release --no-default-features --features tm1637

# Flash and monitor
espflash flash target/riscv32imc-esp-espidf/release/led-kurokku-esp --monitor

# Provision the device (one-time)
uv run tools/provision.py my-device
```

### First Build

The first build downloads and compiles the ESP-IDF SDK, which takes a while. You'll see output from the ESP-IDF build system — this is normal. Subsequent builds only recompile your Rust code and are much faster.

### Flashing

1. Connect the ESP32-C3 to your computer via USB-C
2. Run `just deploy` (or the manual flash command above)
3. `espflash` will auto-detect the serial port. If you have multiple serial devices, it will ask you to choose
4. After flashing, the serial monitor opens automatically — you'll see log output showing the startup sequence

### What to Expect on First Boot

After flashing and provisioning, the display will show:

1. **"KUROKKU"** — firmware is starting
2. **Connecting to WiFi** — may take a few seconds
3. **IP address** — scrolls across the display once connected
4. **Clock** — default display when no server instructions are active

If the device hasn't been provisioned yet (no `just provision ...` run), it will show **"NO CFG"** after the banner and stop there.

If the server is reachable, it will start following whatever instructions the server provides.

## 5. Troubleshooting

### Build fails with linker errors

Make sure `ldproxy` is installed: `cargo install ldproxy`

### `espflash` can't find the device

- Check the USB cable — some cables are charge-only and don't carry data
- On Linux, you may need to add your user to the `dialout` group: `sudo usermod -aG dialout $USER` (log out and back in)
- On macOS, you may need to install a USB serial driver for some ESP32-C3 boards

### Display shows nothing

- Double-check the wiring, especially that VCC is connected to the 5V pin (not 3.3V)
- Make sure the jumper wires are firmly connected
- Check that you're connecting to the **input** side of the MAX7219 module

### WiFi won't connect

- Verify SSID and password in `tools/devices/<your-device>.yaml` are correct, then re-run `just provision <your-device>` to write the corrected values to NVS
- The ESP32-C3 only supports **2.4 GHz** WiFi — it cannot connect to 5 GHz networks
- Check the serial monitor output for error messages

### Display shows "NO CFG"

The device has no WiFi credentials in NVS. Run `just provision <your-device>` to populate it from the matching YAML file in `tools/devices/`.

### Device reboots in a loop

Check the serial monitor for panic messages. Common causes:
- Stack overflow — if you've added complex code, the 8KB main task stack may not be enough
- Wrong SPI pins — if your board uses non-default pins
