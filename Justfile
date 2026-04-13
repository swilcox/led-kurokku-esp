set dotenv-load

default:
    just --list

# Build debug firmware
build:
    cargo build

# Build release firmware
build-release:
    cargo build --release

# Build for TM1637 7-segment display
build-tm1637:
    cargo build --features tm1637 --no-default-features

# Flash release firmware and open serial monitor
flash:
    espflash flash target/riscv32imc-esp-espidf/release/led-kurokku-esp --monitor

# Flash debug firmware and open serial monitor
flash-debug:
    espflash flash target/riscv32imc-esp-espidf/debug/led-kurokku-esp --monitor

# Build release and flash in one step
deploy: build-release flash
