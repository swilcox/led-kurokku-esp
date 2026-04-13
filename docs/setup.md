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

## 2. Hardware

### Shopping List

| Component | Quantity | Notes |
|-----------|----------|-------|
| ESP32-C3 development board | 1 | Any board with USB-C and exposed GPIO pins. Common options: ESP32-C3-DevKitM-1, Seeed XIAO ESP32C3, WeAct ESP32-C3 |
| MAX7219 LED matrix module (4-in-1) | 1 | Look for "MAX7219 dot matrix module 4 in 1" — these come as a single board with four 8x8 LED matrices daisy-chained together, giving you a 32x8 pixel display |
| Jumper wires (female-to-female) | 5 | For connecting the ESP32-C3 to the LED matrix |
| USB-C cable | 1 | For power and flashing |

You can find all of these on Amazon, AliExpress, or your preferred electronics supplier. The total cost is typically under $15.

### Wiring

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

## 3. Configuration

### Environment Variables

Copy the example environment file and edit it with your values:

```bash
cp .env.example .env
```

Edit `.env`:

```bash
# Your WiFi network name
KUROKKU_WIFI_SSID=MyNetwork

# Your WiFi password
KUROKKU_WIFI_PASSWORD=secret

# URL of your kurokku server (see led-kurokku-go)
KUROKKU_SERVER_URL=http://192.168.1.100:8080

# Unique ID for this device (used in server API calls)
KUROKKU_DEVICE_ID=esp32-001

# Timezone in POSIX format (this example is US Central with DST)
KUROKKU_TZ=CST6CDT,M3.2.0,M11.1.0
```

These values are baked into the firmware at compile time. If you change them, you need to rebuild and reflash.

### Timezone Format

The `KUROKKU_TZ` value uses the POSIX TZ format. Some common examples:

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

### Runtime Configuration (NVS)

Some settings can also be changed at runtime via NVS (Non-Volatile Storage) without reflashing. NVS values take priority over compile-time values.

| NVS Key | Default | Description |
|---------|---------|-------------|
| `wifi_ssid` | from `.env` | WiFi network name |
| `wifi_pass` | from `.env` | WiFi password |
| `server_url` | from `.env` | Server base URL |
| `device_id` | from `.env` | Device identifier |
| `tz` | from `.env` | POSIX timezone string |
| `format_24h` | `1` | 24-hour clock (1) or 12-hour (0) |
| `brightness` | `4` | Display brightness, 0-15 |
| `poll_ms` | `5000` | Server poll interval in milliseconds |

NVS values persist across reboots and firmware updates.

## 4. Building and Flashing

### Using just (Recommended)

```bash
# List all available commands
just

# Build debug firmware
just build

# Build release firmware (smaller, optimized)
just build-release

# Build release and flash in one step
just deploy
```

### Manual Build

If you prefer not to use `just`:

```bash
# Set environment variables (or export them)
export KUROKKU_WIFI_SSID="MyNetwork"
export KUROKKU_WIFI_PASSWORD="secret"
export KUROKKU_SERVER_URL="http://192.168.1.100:8080"
export KUROKKU_DEVICE_ID="esp32-001"
export KUROKKU_TZ="CST6CDT,M3.2.0,M11.1.0"

# Build
cargo build --release

# Flash and monitor
espflash flash target/riscv32imc-esp-espidf/release/led-kurokku-esp --monitor
```

### First Build

The first build downloads and compiles the ESP-IDF SDK, which takes a while. You'll see output from the ESP-IDF build system — this is normal. Subsequent builds only recompile your Rust code and are much faster.

### Flashing

1. Connect the ESP32-C3 to your computer via USB-C
2. Run `just deploy` (or the manual flash command above)
3. `espflash` will auto-detect the serial port. If you have multiple serial devices, it will ask you to choose
4. After flashing, the serial monitor opens automatically — you'll see log output showing the startup sequence

### What to Expect on First Boot

The display will show:

1. **"KUROKKU"** — firmware is starting
2. **Connecting to WiFi** — may take a few seconds
3. **IP address** — scrolls across the display once connected
4. **Clock** — default display when no server instructions are active

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

- Verify SSID and password in `.env` are correct
- The ESP32-C3 only supports **2.4 GHz** WiFi — it cannot connect to 5 GHz networks
- Check the serial monitor output for error messages

### Device reboots in a loop

Check the serial monitor for panic messages. Common causes:
- Stack overflow — if you've added complex code, the 8KB main task stack may not be enough
- Wrong SPI pins — if your board uses non-default pins
