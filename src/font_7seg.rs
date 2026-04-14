//! 7-segment font: character → segment bitmask.
//!
//! Bit layout (matches TM1637 and standard 7-seg convention):
//!
//! ```text
//!    _a_
//!   |   |
//!   f   b
//!   |_g_|
//!   |   |
//!   e   c
//!   |_d_|
//! ```
//!
//! bit 0 = a, 1 = b, 2 = c, 3 = d, 4 = e, 5 = f, 6 = g.
//! bit 7 (DP/colon) is driver-managed and not set here.
//!
//! Mapping is ported from the Python `led_kurokku.tm1637.SEGMENTS`
//! and Go `segfont.Seg7` sister projects. Uppercase and lowercase
//! share a bitmask where the rendering is the same (`A|a`, `E|e`,
//! etc.); C/c, H/h, O/o, U/u have distinct renderings.

/// Encode a single character. `None` for unsupported characters.
pub fn encode(c: char) -> Option<u8> {
    Some(match c {
        '0' => 0x3F,
        '1' => 0x06,
        '2' => 0x5B,
        '3' => 0x4F,
        '4' => 0x66,
        '5' => 0x6D,
        '6' => 0x7D,
        '7' => 0x07,
        '8' => 0x7F,
        '9' => 0x6F,

        'A' | 'a' => 0x77,
        'B' | 'b' => 0x7C,
        'C' => 0x39,
        'c' => 0x58,
        'D' | 'd' => 0x5E,
        'E' | 'e' => 0x79,
        'F' | 'f' => 0x71,
        'G' | 'g' => 0x3D,
        'H' => 0x76,
        'h' => 0x74,
        'I' | 'i' => 0x30,
        'J' | 'j' => 0x1E,
        'K' | 'k' => 0x76,
        'L' | 'l' => 0x38,
        'M' | 'm' => 0x55,
        'N' | 'n' => 0x54,
        'O' => 0x3F,
        'o' => 0x5C,
        'P' | 'p' => 0x73,
        'Q' | 'q' => 0x67,
        'R' | 'r' => 0x50,
        'S' | 's' => 0x6D,
        'T' | 't' => 0x78,
        'U' => 0x3E,
        'u' | 'V' | 'v' => 0x1C,
        'W' | 'w' => 0x2A,
        'X' | 'x' => 0x76,
        'Y' | 'y' => 0x6E,
        'Z' | 'z' => 0x5B,

        '-' => 0x40,
        '_' => 0x08,
        '*' | '°' => 0x63,
        ' ' => 0x00,

        _ => return None,
    })
}

/// Encode a single digit 0-9. Panics on out-of-range input;
/// callers that compute `hour / 10`, `minute % 10`, etc. can rely on this.
pub fn digit(d: u8) -> u8 {
    const DIGITS: [u8; 10] = [0x3F, 0x06, 0x5B, 0x4F, 0x66, 0x6D, 0x7D, 0x07, 0x7F, 0x6F];
    DIGITS[d as usize]
}

/// Encode a string. Unsupported characters render as blank (`0x00`).
///
/// `.` is folded into the previous digit's DP bit (0x80) rather than
/// consuming its own glyph slot — so `"1.2.3.4"` produces 4 bytes, not 7.
/// A leading `.` (or a second `.` after a digit that already has DP set)
/// is emitted as a standalone `0x80` byte. On TM1637 modules whose digit
/// DPs aren't wired, standalone DPs render blank; if a DP happens to land
/// on digit 1 (the colon position), it visually merges with the colon.
pub fn encode_text(s: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    for c in s.chars() {
        if c == '.' {
            if let Some(last) = out.last_mut() {
                if *last & 0x80 == 0 {
                    *last |= 0x80;
                    continue;
                }
            }
            out.push(0x80);
            continue;
        }
        out.push(encode(c).unwrap_or(0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digit_table_matches_encode_for_0_through_9() {
        for d in 0..=9u8 {
            let c = char::from(b'0' + d);
            assert_eq!(Some(digit(d)), encode(c));
        }
    }

    #[test]
    fn digit_bitmasks_are_canonical() {
        // 0 = a+b+c+d+e+f = 0x3F; 1 = b+c = 0x06; 8 = all segments = 0x7F
        assert_eq!(digit(0), 0x3F);
        assert_eq!(digit(1), 0x06);
        assert_eq!(digit(8), 0x7F);
    }

    #[test]
    fn case_insensitive_where_rendering_matches() {
        // Letters that share a rendering across cases.
        assert_eq!(encode('A'), encode('a'));
        assert_eq!(encode('E'), encode('e'));
        assert_eq!(encode('F'), encode('f'));
        assert_eq!(encode('P'), encode('p'));
    }

    #[test]
    fn case_distinct_letters_differ() {
        // C, H, O, U have visually different upper/lower renderings.
        assert_ne!(encode('C'), encode('c'));
        assert_ne!(encode('H'), encode('h'));
        assert_ne!(encode('O'), encode('o'));
        assert_ne!(encode('U'), encode('u'));
    }

    #[test]
    fn uppercase_o_equals_zero() {
        // Expected: uppercase O and digit 0 share the 6-segment oval shape.
        assert_eq!(encode('O'), Some(0x3F));
        assert_eq!(encode('O'), Some(digit(0)));
    }

    #[test]
    fn unsupported_character_returns_none() {
        assert_eq!(encode('?'), None);
        assert_eq!(encode('@'), None);
        assert_eq!(encode('\t'), None);
    }

    #[test]
    fn space_renders_blank() {
        assert_eq!(encode(' '), Some(0x00));
    }

    #[test]
    fn dp_bit_is_never_set_in_base_encoding() {
        // bit 7 is reserved for driver-managed DP/colon — no base glyph
        // should have it set.
        for c in ['0', '9', 'A', 'z', '-', '_', '*', ' '] {
            if let Some(b) = encode(c) {
                assert_eq!(b & 0x80, 0, "char {:?} set DP bit", c);
            }
        }
    }

    #[test]
    fn encode_text_clock_style() {
        // "12:34" — colon is driver-managed, so ':' here falls through to 0.
        let out = encode_text("12 34");
        assert_eq!(out.len(), 5);
        assert_eq!(out[0], digit(1));
        assert_eq!(out[1], digit(2));
        assert_eq!(out[2], 0x00); // space → blank
        assert_eq!(out[3], digit(3));
        assert_eq!(out[4], digit(4));
    }

    #[test]
    fn encode_text_folds_dot_into_previous_byte() {
        // "1.2.3.4" — 4 bytes, each with DP bit set.
        let out = encode_text("1.2.3.4");
        assert_eq!(out.len(), 4);
        assert_eq!(out[0], digit(1) | 0x80);
        assert_eq!(out[1], digit(2) | 0x80);
        assert_eq!(out[2], digit(3) | 0x80);
        assert_eq!(out[3], digit(4));
    }

    #[test]
    fn encode_text_ip_address_is_compact() {
        // "10.11.12.13" — 8 digits with 3 folded DPs = 8 bytes.
        let out = encode_text("10.11.12.13");
        assert_eq!(out.len(), 8);
        // DP on positions 1, 3, 5 (the digit just before each '.').
        assert_eq!(out[1] & 0x80, 0x80);
        assert_eq!(out[3] & 0x80, 0x80);
        assert_eq!(out[5] & 0x80, 0x80);
        // Last digit has no trailing dot.
        assert_eq!(out[7] & 0x80, 0);
    }

    #[test]
    fn encode_text_leading_dot_becomes_standalone() {
        // Nothing to fold into — emits a bare 0x80.
        let out = encode_text(".1");
        assert_eq!(out, vec![0x80, digit(1)]);
    }

    #[test]
    fn encode_text_second_dot_after_dp_is_standalone() {
        // "1.." — first '.' folds into '1', second '.' can't fold (DP already
        // set) and emits a bare 0x80.
        let out = encode_text("1..");
        assert_eq!(out, vec![digit(1) | 0x80, 0x80]);
    }

    #[test]
    fn encode_text_unsupported_chars_render_blank() {
        // '?' has no glyph → renders as 0x00 rather than being dropped.
        let out = encode_text("A?B");
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], encode('A').unwrap());
        assert_eq!(out[1], 0x00);
        assert_eq!(out[2], encode('B').unwrap());
    }

    #[test]
    fn encode_text_empty_input_is_empty() {
        assert_eq!(encode_text(""), Vec::<u8>::new());
    }
}
