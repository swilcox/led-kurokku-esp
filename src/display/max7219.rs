use anyhow::Result;
use esp_idf_svc::hal::gpio::OutputPin;
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::hal::spi::{self, SpiDeviceDriver, SpiDriver, SpiDriverConfig, config};
use esp_idf_svc::hal::units::Hertz;

use super::{Display, PixelDisplay};

// MAX7219 registers
const REG_NOOP: u8 = 0x00;
const REG_DECODE_MODE: u8 = 0x09;
const REG_INTENSITY: u8 = 0x0A;
const REG_SCAN_LIMIT: u8 = 0x0B;
const REG_SHUTDOWN: u8 = 0x0C;
const REG_DISPLAY_TEST: u8 = 0x0F;

const NUM_DEVICES: usize = 4;

pub struct Max7219<'d> {
    spi: SpiDeviceDriver<'d, SpiDriver<'d>>,
}

impl<'d> Max7219<'d> {
    /// Create a new MAX7219 driver.
    ///
    /// - `spi_bus`: SPI peripheral (e.g. SPI2)
    /// - `sclk`: SPI clock pin
    /// - `mosi`: SPI MOSI (data) pin
    /// - `cs`: Chip select pin
    pub fn new(
        spi_bus: impl Peripheral<P = impl spi::SpiAnyPins> + 'd,
        sclk: impl Peripheral<P = impl OutputPin> + 'd,
        mosi: impl Peripheral<P = impl OutputPin> + 'd,
        cs: impl Peripheral<P = impl OutputPin> + 'd,
    ) -> Result<Self> {
        let driver = SpiDriver::new(spi_bus, sclk, mosi, None::<esp_idf_svc::hal::gpio::AnyIOPin>, &SpiDriverConfig::default())
            .map_err(|e| anyhow::anyhow!("SPI driver init failed: {}", e))?;

        let spi_config = config::Config::new()
            .baudrate(Hertz(10_000_000))
            .data_mode(config::MODE_0);

        let spi = SpiDeviceDriver::new(driver, Some(cs), &spi_config)
            .map_err(|e| anyhow::anyhow!("SPI device init failed: {}", e))?;

        Ok(Self { spi })
    }

    /// Send the same register/value to all daisy-chained devices.
    fn write_all(&mut self, reg: u8, value: u8) {
        let mut packet = [0u8; NUM_DEVICES * 2];
        for i in 0..NUM_DEVICES {
            packet[i * 2] = reg;
            packet[i * 2 + 1] = value;
        }
        if let Err(e) = self.spi.write(&packet) {
            log::error!("max7219 write_all failed: reg=0x{:02X}, err={}", reg, e);
        }
    }
}

impl<'d> Display for Max7219<'d> {
    fn init(&mut self) -> Result<()> {
        self.write_all(REG_DISPLAY_TEST, 0x00); // normal operation
        self.write_all(REG_DECODE_MODE, 0x00);  // raw segments, no BCD
        self.write_all(REG_SCAN_LIMIT, 0x07);   // scan all 8 rows
        self.write_all(REG_SHUTDOWN, 0x01);      // power on
        self.write_all(REG_INTENSITY, 0x04);     // moderate brightness
        self.clear();
        Ok(())
    }

    fn close(&mut self) -> Result<()> {
        self.write_all(REG_SHUTDOWN, 0x00);
        Ok(())
    }

    fn clear(&mut self) {
        for row in 1..=8u8 {
            self.write_all(row, 0x00);
        }
    }

    fn set_brightness(&mut self, level: u8) {
        let level = level.min(15);
        self.write_all(REG_INTENSITY, level);
    }
}

impl<'d> PixelDisplay for Max7219<'d> {
    fn write_framebuffer(&mut self, buf: &[u8; 32]) {
        for row in 0u8..8 {
            // Build packet: 2 bytes per device, in daisy-chain order.
            // Device 0 (columns 0-7) is first in packet = farthest in chain (rightmost).
            let mut packet = [0u8; NUM_DEVICES * 2];
            for dev in 0..NUM_DEVICES {
                let idx = dev * 2;
                packet[idx] = row + 1; // MAX7219 row registers are 1-indexed

                // Pack 8 columns into one byte for this device's row
                let base_col = dev * 8;
                let mut row_byte: u8 = 0;
                for col in 0..8 {
                    if buf[base_col + col] & (1 << row) != 0 {
                        row_byte |= 1 << (7 - col);
                    }
                }
                packet[idx + 1] = row_byte;
            }
            if let Err(e) = self.spi.write(&packet) {
                log::error!("max7219 write_framebuffer failed: row={}, err={}", row, e);
                return;
            }
        }
    }

    fn width(&self) -> usize {
        32
    }

    fn height(&self) -> usize {
        8
    }
}
