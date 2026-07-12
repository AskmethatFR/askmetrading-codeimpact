// RFC 4648 §4 base64 encoder (US7 T2 slice S4) — hand-rolled to keep the
// hexagon/secondaries dependency budget at zero (spec §6: no `base64`
// crate, no build script). Padding is where hand-rolled encoders die, so
// the test vectors below are the canonical RFC 4648 ones, padding cases
// included.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut chunks = data.chunks_exact(3);
    for chunk in &mut chunks {
        let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32);
        push_sextet(&mut out, n >> 18);
        push_sextet(&mut out, n >> 12);
        push_sextet(&mut out, n >> 6);
        push_sextet(&mut out, n);
    }

    match chunks.remainder() {
        [] => {}
        [b0] => {
            let n = (*b0 as u32) << 16;
            push_sextet(&mut out, n >> 18);
            push_sextet(&mut out, n >> 12);
            out.push('=');
            out.push('=');
        }
        [b0, b1] => {
            let n = ((*b0 as u32) << 16) | ((*b1 as u32) << 8);
            push_sextet(&mut out, n >> 18);
            push_sextet(&mut out, n >> 12);
            push_sextet(&mut out, n >> 6);
            out.push('=');
        }
        _ => unreachable!("chunks_exact(3)'s remainder is always 0, 1, or 2 bytes"),
    }

    out
}

fn push_sextet(out: &mut String, shifted: u32) {
    out.push(ALPHABET[(shifted & 0x3F) as usize] as char);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test List (RFC 4648 §10 test vectors — the canonical padding cases):
    // 1. "" -> ""
    // 2. "f" -> "Zg==" (2-byte padding)
    // 3. "fo" -> "Zm8=" (1-byte padding)
    // 4. "foo" -> "Zm9v" (no padding, exactly 3 bytes)
    // 5. "foob" -> "Zm9vYg==" (3 bytes + 1, 2-byte padding again)
    // 6. "fooba" -> "Zm9vYmE=" (3 bytes + 2, 1-byte padding)
    // 7. "foobar" -> "Zm9vYmFy" (exactly 6 bytes, no padding)

    #[test]
    fn encodes_empty_input_to_empty_string() {
        assert_eq!(encode(b""), "");
    }

    #[test]
    fn encodes_one_byte_with_two_padding_chars() {
        assert_eq!(encode(b"f"), "Zg==");
    }

    #[test]
    fn encodes_two_bytes_with_one_padding_char() {
        assert_eq!(encode(b"fo"), "Zm8=");
    }

    #[test]
    fn encodes_three_bytes_with_no_padding() {
        assert_eq!(encode(b"foo"), "Zm9v");
    }

    #[test]
    fn encodes_four_bytes_with_two_padding_chars() {
        assert_eq!(encode(b"foob"), "Zm9vYg==");
    }

    #[test]
    fn encodes_five_bytes_with_one_padding_char() {
        assert_eq!(encode(b"fooba"), "Zm9vYmE=");
    }

    #[test]
    fn encodes_six_bytes_with_no_padding() {
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    }
}
