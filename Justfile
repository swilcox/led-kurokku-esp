default:
    just --list

# Build debug firmware
build:
    cargo build

# Build release firmware.
# On the first build after adding a custom partition table, cmake creates the
# out/ dir during its configure step but then fails to find partitions.csv
# during the build step. The retry copies it there and succeeds.
build-release:
    #!/usr/bin/env bash
    shopt -s nullglob
    _sync() {
        for dir in target/riscv32imc-esp-espidf/release/build/esp-idf-sys-*/out; do
            cp -f partitions.csv "$dir/"
        done
    }
    _sync
    cargo build --release || { _sync && cargo build --release; }

# Build debug for TM1637 7-segment display
build-tm1637:
    cargo build --features tm1637 --no-default-features

# Build release for TM1637 7-segment display
build-tm1637-release:
    cargo build --release --features tm1637 --no-default-features

# Flash release firmware and open serial monitor
flash:
    espflash flash \
        --bootloader target/riscv32imc-esp-espidf/release/bootloader.bin \
        --partition-table partitions.csv \
        --erase-data-parts ota \
        target/riscv32imc-esp-espidf/release/led-kurokku-esp \
        --monitor

# Flash debug firmware and open serial monitor
flash-debug:
    espflash flash \
        --bootloader target/riscv32imc-esp-espidf/debug/bootloader.bin \
        --partition-table partitions.csv \
        --erase-data-parts ota \
        target/riscv32imc-esp-espidf/debug/led-kurokku-esp \
        --monitor

# Build release and flash in one step
deploy: build-release flash

# Build TM1637 release and flash in one step
deploy-tm1637: build-tm1637-release flash

# Run unit tests on the host (pure modules only: font, font_7seg, framebuf,
# tm1637_proto). Overrides the default ESP target from .cargo/config.toml.
test:
    cargo test --target {{ `rustc -vV | awk '/^host/ {print $2}'` }} --no-default-features

# Provision a device's NVS from tools/devices/<name>.yaml.
# Usage: just provision kitchen-01 [/dev/ttyUSB0]
provision device port="":
    uv run tools/provision.py {{device}} {{ if port != "" { "--port " + port } else { "" } }}
