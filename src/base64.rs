//! Tiny standard-alphabet Base64 decoder. Used to decode `data:` URI
//! payloads inside `@font-face` `src: url("data:...;base64,...")` values.
//! Padding is optional; ASCII whitespace is ignored.

#[derive(Debug, PartialEq, Eq)]
pub enum Base64Error {
    InvalidChar(u8),
    InvalidLength,
}

const INVALID: u8 = 0xff;

const fn decode_table() -> [u8; 256] {
    let mut t = [INVALID; 256];
    let mut i = 0u8;
    while i < 26 {
        t[(b'A' + i) as usize] = i;
        t[(b'a' + i) as usize] = i + 26;
        i += 1;
    }
    let mut i = 0u8;
    while i < 10 {
        t[(b'0' + i) as usize] = i + 52;
        i += 1;
    }
    t[b'+' as usize] = 62;
    t[b'/' as usize] = 63;
    t
}

const TABLE: [u8; 256] = decode_table();

pub fn decode(input: &str) -> Result<Vec<u8>, Base64Error> {
    let mut out: Vec<u8> = Vec::with_capacity(input.len() / 4 * 3);
    let mut quad = [0u8; 4];
    let mut filled = 0usize;
    let mut pad = 0usize;

    for &b in input.as_bytes() {
        if b.is_ascii_whitespace() {
            continue;
        }
        if b == b'=' {
            pad += 1;
            quad[filled] = 0;
            filled += 1;
        } else {
            if pad > 0 {
                return Err(Base64Error::InvalidChar(b));
            }
            let v = TABLE[b as usize];
            if v == INVALID {
                return Err(Base64Error::InvalidChar(b));
            }
            quad[filled] = v;
            filled += 1;
        }
        if filled == 4 {
            let n = (u32::from(quad[0]) << 18)
                | (u32::from(quad[1]) << 12)
                | (u32::from(quad[2]) << 6)
                | u32::from(quad[3]);
            out.push((n >> 16) as u8);
            if pad < 2 {
                out.push((n >> 8) as u8);
            }
            if pad < 1 {
                out.push(n as u8);
            }
            filled = 0;
        }
    }

    match filled {
        0 => Ok(out),
        2 => {
            let n = (u32::from(quad[0]) << 18) | (u32::from(quad[1]) << 12);
            out.push((n >> 16) as u8);
            Ok(out)
        }
        3 => {
            let n =
                (u32::from(quad[0]) << 18) | (u32::from(quad[1]) << 12) | (u32::from(quad[2]) << 6);
            out.push((n >> 16) as u8);
            out.push((n >> 8) as u8);
            Ok(out)
        }
        _ => Err(Base64Error::InvalidLength),
    }
}

#[cfg(test)]
mod tests {
    use super::{Base64Error, decode};

    #[test]
    fn decode_empty() {
        assert_eq!(decode("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn decode_single_quad_no_padding() {
        assert_eq!(decode("QUJD").unwrap(), b"ABC");
    }

    #[test]
    fn decode_with_one_pad() {
        // "AB" → QUI=
        assert_eq!(decode("QUI=").unwrap(), b"AB");
    }

    #[test]
    fn decode_with_two_pads() {
        // "A" → QQ==
        assert_eq!(decode("QQ==").unwrap(), b"A");
    }

    #[test]
    fn decode_unpadded_tail() {
        // "AB" without padding
        assert_eq!(decode("QUI").unwrap(), b"AB");
        // "A" without padding
        assert_eq!(decode("QQ").unwrap(), b"A");
    }

    #[test]
    fn decode_rejects_invalid_char() {
        assert_eq!(decode("QU!D"), Err(Base64Error::InvalidChar(b'!')));
    }

    #[test]
    fn decode_ignores_whitespace_and_newlines() {
        assert_eq!(decode("QUJD\n  QUJD\t").unwrap(), b"ABCABC");
    }

    #[test]
    fn decode_round_trip_arbitrary_bytes() {
        // QUJDREVGRw== → "ABCDEFG"
        assert_eq!(decode("QUJDREVGRw==").unwrap(), b"ABCDEFG");
    }
}
