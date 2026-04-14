//! TM1637 protocol byte-encoding, decoupled from GPIO so it can be
//! unit-tested on any host without ESP-IDF.
//!
//! The driver (`tm1637.rs`) handles the bit-banged start/stop/ack framing;
//! this module produces the raw payload bytes that go between those frames.

pub const CMD_DATA_AUTO: u8 = 0x40; // write data, auto-increment address
pub const CMD_ADDR_START: u8 = 0xC0; // pointer to digit 0
pub const CMD_DISPLAY_ON: u8 = 0x88; // display-on command; low 3 bits = brightness
pub const NUM_DIGITS: usize = 4;

/// Payload for the "set data mode" frame. Sent between its own start/stop.
pub fn data_mode_frame() -> [u8; 1] {
    [CMD_DATA_AUTO]
}

/// Payload for the "write all digits" frame: address byte followed by
/// four segment bytes. `colon` ORs 0x80 into digit 1, matching the
/// standard 4-digit TM1637 clock module layout. Missing entries in
/// `segments` pad with 0. Segment bytes are truncated to u8 (low byte
/// only — the TM1637 drives 7 segments + DP).
pub fn digit_frame(segments: &[u16], colon: bool) -> [u8; 1 + NUM_DIGITS] {
    let mut out = [0u8; 1 + NUM_DIGITS];
    out[0] = CMD_ADDR_START;
    for i in 0..NUM_DIGITS {
        let mut b = segments.get(i).copied().unwrap_or(0) as u8;
        if i == 1 && colon {
            b |= 0x80;
        }
        out[1 + i] = b;
    }
    out
}

/// Payload for the "display on + brightness" frame. `native_bright` is
/// clamped to the TM1637's native 0-7 range.
pub fn display_control_frame(native_bright: u8) -> [u8; 1] {
    [CMD_DISPLAY_ON | (native_bright & 0x07)]
}

/// Map the crate-wide 0-15 brightness range to the TM1637's native 0-7.
pub fn map_brightness(level_0_15: u8) -> u8 {
    (level_0_15 >> 1).min(7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_mode_is_auto_increment_command() {
        assert_eq!(data_mode_frame(), [0x40]);
    }

    #[test]
    fn digit_frame_empty_slice_writes_zeros() {
        assert_eq!(digit_frame(&[], false), [0xC0, 0, 0, 0, 0]);
    }

    #[test]
    fn digit_frame_with_colon_sets_bit7_on_digit1_only() {
        // "12:34" → digit 1 gets 0x80 ORed
        let segs = [0x06_u16, 0x5B, 0x4F, 0x66];
        let out = digit_frame(&segs, true);
        assert_eq!(out[0], 0xC0);
        assert_eq!(out[1], 0x06); // digit 0 unchanged
        assert_eq!(out[2], 0x5B | 0x80); // digit 1 colon set
        assert_eq!(out[3], 0x4F); // digit 2 unchanged
        assert_eq!(out[4], 0x66); // digit 3 unchanged
    }

    #[test]
    fn digit_frame_without_colon_leaves_bit7_clear() {
        let segs = [0x06_u16, 0x5B, 0x4F, 0x66];
        let out = digit_frame(&segs, false);
        assert_eq!(out[2] & 0x80, 0);
    }

    #[test]
    fn digit_frame_short_slice_pads_with_zero() {
        assert_eq!(digit_frame(&[0xFF, 0xAB], false), [0xC0, 0xFF, 0xAB, 0, 0]);
    }

    #[test]
    fn digit_frame_excess_entries_ignored() {
        let out = digit_frame(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06], false);
        assert_eq!(out, [0xC0, 0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn digit_frame_u16_truncates_to_low_byte() {
        // High byte is discarded — segment bytes are 8-bit on TM1637
        assert_eq!(digit_frame(&[0xFF12], false), [0xC0, 0x12, 0, 0, 0]);
    }

    #[test]
    fn digit_frame_preserves_user_dp_bit() {
        // If the caller has already OR'd 0x80 into a byte (e.g. IP-address
        // DP folding), the colon-on-digit-1 logic is additive and harmless.
        let out = digit_frame(&[0x06 | 0x80, 0, 0, 0], false);
        assert_eq!(out[1], 0x86);
    }

    #[test]
    fn display_control_encodes_low_3_bits() {
        assert_eq!(display_control_frame(0), [0x88]);
        assert_eq!(display_control_frame(4), [0x8C]);
        assert_eq!(display_control_frame(7), [0x8F]);
    }

    #[test]
    fn display_control_masks_high_bits() {
        // Any bits above 0x07 must be ignored — caller already passes native 0-7
        // but defensive masking avoids silently scrambling the command bits.
        assert_eq!(display_control_frame(0xFF), [0x8F]);
        assert_eq!(display_control_frame(0x18), [0x88]);
    }

    #[test]
    fn map_brightness_covers_boundary_values() {
        assert_eq!(map_brightness(0), 0);
        assert_eq!(map_brightness(1), 0); // rounds down
        assert_eq!(map_brightness(2), 1);
        assert_eq!(map_brightness(14), 7);
        assert_eq!(map_brightness(15), 7);
    }

    #[test]
    fn map_brightness_clamps_out_of_range() {
        // 0-15 is the contract, but caller bugs shouldn't produce
        // values outside 0-7.
        assert_eq!(map_brightness(255), 7);
    }
}
