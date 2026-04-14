use anyhow::Result;
use esp_idf_svc::hal::delay::Ets;
use esp_idf_svc::hal::gpio::{Output, OutputPin, PinDriver};
use esp_idf_svc::hal::peripheral::Peripheral;

use super::tm1637_proto::{
    data_mode_frame, digit_frame, display_control_frame, map_brightness, NUM_DIGITS,
};
use super::{Display, SegmentDisplay};

// Bit-period for the bit-banged 2-wire protocol. TM1637 spec allows up to
// 500 kHz; we target ~100 kHz (≈5 µs half-period) for margin. FreeRTOS
// tick is 1 ms, so std::thread::sleep is too coarse — use ETS.
const BIT_DELAY_US: u32 = 5;

pub struct Tm1637<'d, CLK, DIO>
where
    CLK: OutputPin,
    DIO: OutputPin,
{
    clk: PinDriver<'d, CLK, Output>,
    dio: PinDriver<'d, DIO, Output>,
    bright: u8, // native TM1637 level 0..=7
}

impl<'d, CLK, DIO> Tm1637<'d, CLK, DIO>
where
    CLK: OutputPin,
    DIO: OutputPin,
{
    /// Create a new TM1637 driver.
    ///
    /// TM1637 modules typically have on-board pull-ups on CLK and DIO. We
    /// drive both lines push-pull for simplicity (matches the Go sister
    /// project); briefly contended during the ACK slot, which is tolerated
    /// by every module we've tested.
    pub fn new(
        clk: impl Peripheral<P = CLK> + 'd,
        dio: impl Peripheral<P = DIO> + 'd,
    ) -> Result<Self> {
        let clk = PinDriver::output(clk)
            .map_err(|e| anyhow::anyhow!("TM1637 CLK pin init failed: {}", e))?;
        let dio = PinDriver::output(dio)
            .map_err(|e| anyhow::anyhow!("TM1637 DIO pin init failed: {}", e))?;
        Ok(Self { clk, dio, bright: 7 })
    }

    fn delay() {
        Ets::delay_us(BIT_DELAY_US);
    }

    fn start(&mut self) {
        let _ = self.dio.set_high();
        let _ = self.clk.set_high();
        Self::delay();
        let _ = self.dio.set_low();
        Self::delay();
        let _ = self.clk.set_low();
        Self::delay();
    }

    fn stop(&mut self) {
        let _ = self.clk.set_low();
        Self::delay();
        let _ = self.dio.set_low();
        Self::delay();
        let _ = self.clk.set_high();
        Self::delay();
        let _ = self.dio.set_high();
        Self::delay();
    }

    /// Send one byte LSB-first. We don't actually sample the ACK; we just
    /// release DIO high during the ACK slot (the TM1637 briefly pulls it
    /// low on its own).
    fn write_byte(&mut self, b: u8) {
        for i in 0..8 {
            let _ = self.clk.set_low();
            Self::delay();
            if b & (1 << i) != 0 {
                let _ = self.dio.set_high();
            } else {
                let _ = self.dio.set_low();
            }
            Self::delay();
            let _ = self.clk.set_high();
            Self::delay();
        }
        // ACK slot — release DIO, pulse CLK.
        let _ = self.clk.set_low();
        Self::delay();
        let _ = self.dio.set_high();
        Self::delay();
        let _ = self.clk.set_high();
        Self::delay();
        let _ = self.clk.set_low();
        Self::delay();
    }

    /// Send a single bit-banged frame: START, every byte in `bytes`, STOP.
    fn send_frame(&mut self, bytes: &[u8]) {
        self.start();
        for &b in bytes {
            self.write_byte(b);
        }
        self.stop();
    }

    fn send_display_control(&mut self) {
        self.send_frame(&display_control_frame(self.bright));
    }
}

impl<'d, CLK, DIO> Display for Tm1637<'d, CLK, DIO>
where
    CLK: OutputPin,
    DIO: OutputPin,
{
    fn init(&mut self) -> Result<()> {
        let _ = self.clk.set_high();
        let _ = self.dio.set_high();
        self.clear();
        self.send_display_control();
        Ok(())
    }

    fn close(&mut self) -> Result<()> {
        let _ = self.clk.set_low();
        let _ = self.dio.set_low();
        Ok(())
    }

    fn clear(&mut self) {
        self.write_segments(&[0, 0, 0, 0], false);
    }

    /// Accepts 0-15 (common interface); maps to TM1637's native 0-7.
    fn set_brightness(&mut self, level: u8) {
        self.bright = map_brightness(level);
        self.send_display_control();
    }
}

impl<'d, CLK, DIO> SegmentDisplay for Tm1637<'d, CLK, DIO>
where
    CLK: OutputPin,
    DIO: OutputPin,
{
    /// Write the 4 digit segments. `colon` toggles the colon between
    /// digits 1 and 2 (stored as bit 7 of digit 1 on most TM1637 modules).
    /// Missing entries in `segments` are treated as blank.
    fn write_segments(&mut self, segments: &[u16], colon: bool) {
        self.send_frame(&data_mode_frame());
        self.send_frame(&digit_frame(segments, colon));
        // Re-send display-on + brightness; also ensures the module stays on
        // after a power-up or glitch.
        self.send_display_control();
    }

    fn display_length(&self) -> usize {
        NUM_DIGITS
    }
}
