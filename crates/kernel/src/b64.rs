//! Standard base64 (RFC 4648) for binary data inside JSON documents (frozen plugin bodies).

const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let b = [c.first().copied().unwrap_or(0), c.get(1).copied().unwrap_or(0), c.get(2).copied().unwrap_or(0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for (i, shift) in [18, 12, 6, 0].iter().enumerate() {
            if i <= c.len() {
                out.push(T[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn value(c: u8) -> Option<u32> {
    T.iter().position(|t| *t == c).map(|p| p as u32)
}

/// Decode; whitespace is ignored, anything else invalid is an error.
pub fn decode(text: &str) -> Result<Vec<u8>, String> {
    let clean: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !clean.len().is_multiple_of(4) {
        return Err("base64: length is not a multiple of 4".into());
    }
    let mut out = Vec::with_capacity(clean.len() / 4 * 3);
    for q in clean.chunks(4) {
        let pad = q.iter().rev().take_while(|b| **b == b'=').count();
        if pad > 2 {
            return Err("base64: bad padding".into());
        }
        let mut n = 0u32;
        for (i, b) in q.iter().enumerate() {
            let v = if i >= 4 - pad { 0 } else { value(*b).ok_or("base64: invalid character")? };
            n = n << 6 | v;
        }
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(bytes.get(..3 - pad).unwrap_or_default());
    }
    Ok(out)
}
