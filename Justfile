set dotenv-load

default:
    just --list

# Build debug firmware
build:
    cargo build

# Build release firmware
build-release:
    cargo build --release

# Build debug for TM1637 7-segment display
build-tm1637:
    cargo build --features tm1637 --no-default-features

# Build release for TM1637 7-segment display
build-tm1637-release:
    cargo build --release --features tm1637 --no-default-features

# Flash release firmware and open serial monitor
flash:
    espflash flash target/riscv32imc-esp-espidf/release/led-kurokku-esp --monitor

# Flash debug firmware and open serial monitor
flash-debug:
    espflash flash target/riscv32imc-esp-espidf/debug/led-kurokku-esp --monitor

# Build release and flash in one step
deploy: build-release flash

# Build TM1637 release and flash in one step
deploy-tm1637: build-tm1637-release flash

# Run unit tests on the host (pure modules only: font, font_7seg, framebuf,
# tm1637_proto). Overrides the default ESP target from .cargo/config.toml.
test:
    cargo test --target {{ `rustc -vV | awk '/^host/ {print $2}'` }} --no-default-features
