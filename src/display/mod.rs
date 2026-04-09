use anyhow::Result;

/// Base display interface. All backends implement this.
pub trait Display: Send {
    fn init(&mut self) -> Result<()>;
    fn close(&mut self) -> Result<()>;
    fn clear(&mut self);
    /// Brightness 0-15; backends map to their native range.
    fn set_brightness(&mut self, level: u8);
}

/// Pixel matrix display (MAX7219 8x32).
pub trait PixelDisplay: Display {
    /// Write a 32-column framebuffer. Each byte is one column, bit 0 = top row.
    fn write_framebuffer(&mut self, buf: &[u8; 32]);
    fn width(&self) -> usize;
    fn height(&self) -> usize;
}

/// Segment display (TM1637 7-segment, future HT16K33 14-segment).
pub trait SegmentDisplay: Display {
    /// Write segment data. Each u16 is a segment bitmask. colon controls the colon indicator.
    fn write_segments(&mut self, segments: &[u16], colon: bool);
    fn display_length(&self) -> usize;
}

/// Runtime display type wrapper.
pub enum AnyDisplay {
    Pixel(Box<dyn PixelDisplay>),
    Segment(Box<dyn SegmentDisplay>),
}

#[cfg(feature = "max7219")]
pub mod max7219;

#[cfg(feature = "tm1637")]
pub mod tm1637;
