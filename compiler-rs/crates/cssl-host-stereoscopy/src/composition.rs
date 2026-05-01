//! § composition — Compose two RGBA byte slices (left + right) into a single
//! image in side-by-side, top-bottom, or red-cyan-anaglyph layout.
//!
//! § INPUT FORMAT
//!   - Each input slice : `w * h * 4` bytes (RGBA8 row-major top-to-bottom).
//!   - `w >= 1` and `h >= 1` ; both slices must have identical length.
//!
//! § OUTPUT FORMAT
//!   - SBS : `(2*w) * h * 4` bytes ; left occupies x ∈ [0, w), right ∈ [w, 2w).
//!   - TB  : `w * (2*h) * 4` bytes ; left occupies y ∈ [0, h), right ∈ [h, 2h).
//!   - Anaglyph red-cyan : `w * h * 4` bytes ; per-pixel
//!     `out.r = left.r ; out.g = right.g ; out.b = right.b ; out.a = max(left.a, right.a)`.

/// Composition errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeErr {
    /// Left + right slice lengths don't match, or don't match `w*h*4`.
    LengthMismatch,
    /// Either slice is empty.
    EmptyInput,
    /// `w == 0` or `h == 0`.
    ZeroDimension,
}

impl core::fmt::Display for ComposeErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::LengthMismatch => write!(f, "compose : input slice length != w*h*4 or left.len() != right.len()"),
            Self::EmptyInput => write!(f, "compose : input slice is empty"),
            Self::ZeroDimension => write!(f, "compose : w == 0 or h == 0"),
        }
    }
}

impl std::error::Error for ComposeErr {}

#[inline]
fn validate_inputs(left: &[u8], right: &[u8], w: u32, h: u32) -> Result<usize, ComposeErr> {
    if w == 0 || h == 0 {
        return Err(ComposeErr::ZeroDimension);
    }
    if left.is_empty() || right.is_empty() {
        return Err(ComposeErr::EmptyInput);
    }
    let expected = (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or(ComposeErr::LengthMismatch)?;
    if left.len() != expected || right.len() != expected {
        return Err(ComposeErr::LengthMismatch);
    }
    Ok(expected)
}

/// Side-by-side : left | right → output 2W × H RGBA.
pub fn compose_side_by_side(left: &[u8], right: &[u8], w: u32, h: u32) -> Result<Vec<u8>, ComposeErr> {
    let _ = validate_inputs(left, right, w, h)?;
    let w_us = w as usize;
    let h_us = h as usize;
    let row_bytes = w_us * 4;
    let out_row_bytes = row_bytes * 2;
    let mut out = vec![0u8; out_row_bytes * h_us];

    for y in 0..h_us {
        let in_off = y * row_bytes;
        let out_off = y * out_row_bytes;
        out[out_off..out_off + row_bytes].copy_from_slice(&left[in_off..in_off + row_bytes]);
        out[out_off + row_bytes..out_off + out_row_bytes].copy_from_slice(&right[in_off..in_off + row_bytes]);
    }
    Ok(out)
}

/// Top-bottom : left over right → output W × 2H RGBA.
pub fn compose_top_bottom(left: &[u8], right: &[u8], w: u32, h: u32) -> Result<Vec<u8>, ComposeErr> {
    let len = validate_inputs(left, right, w, h)?;
    let mut out = Vec::with_capacity(len * 2);
    out.extend_from_slice(left);
    out.extend_from_slice(right);
    Ok(out)
}

/// Red-cyan anaglyph : left.R + right.GB → output W × H RGBA.
/// Alpha = max(left.A, right.A) so transparent pixels stay transparent.
pub fn compose_anaglyph_red_cyan(left: &[u8], right: &[u8], w: u32, h: u32) -> Result<Vec<u8>, ComposeErr> {
    let len = validate_inputs(left, right, w, h)?;
    let mut out = Vec::with_capacity(len);
    for i in (0..len).step_by(4) {
        out.push(left[i]); // R from left
        out.push(right[i + 1]); // G from right
        out.push(right[i + 2]); // B from right
        out.push(left[i + 3].max(right[i + 3])); // A = max
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            v.extend_from_slice(&rgba);
        }
        v
    }

    #[test]
    fn sbs_doubles_width() {
        let w = 4;
        let h = 3;
        let l = solid(w, h, [255, 0, 0, 255]);
        let r = solid(w, h, [0, 0, 255, 255]);
        let out = compose_side_by_side(&l, &r, w, h).unwrap();
        assert_eq!(out.len(), (2 * w * h * 4) as usize);
        // First pixel is red ; pixel at x=w should be blue.
        assert_eq!(&out[0..4], &[255, 0, 0, 255]);
        let blue_idx = (w as usize) * 4;
        assert_eq!(&out[blue_idx..blue_idx + 4], &[0, 0, 255, 255]);
    }

    #[test]
    fn tb_doubles_height() {
        let w = 2;
        let h = 5;
        let l = solid(w, h, [10, 20, 30, 255]);
        let r = solid(w, h, [40, 50, 60, 255]);
        let out = compose_top_bottom(&l, &r, w, h).unwrap();
        assert_eq!(out.len(), (w * 2 * h * 4) as usize);
        // Last pixel of top half = left's last pixel.
        let top_last = ((w * h) as usize - 1) * 4;
        assert_eq!(&out[top_last..top_last + 4], &[10, 20, 30, 255]);
        // First pixel of bottom half = right's first pixel.
        let bottom_first = (w * h * 4) as usize;
        assert_eq!(&out[bottom_first..bottom_first + 4], &[40, 50, 60, 255]);
    }

    #[test]
    fn anaglyph_preserves_wh() {
        let w = 3;
        let h = 3;
        let l = solid(w, h, [200, 100, 50, 255]);
        let r = solid(w, h, [10, 20, 30, 128]);
        let out = compose_anaglyph_red_cyan(&l, &r, w, h).unwrap();
        assert_eq!(out.len(), (w * h * 4) as usize);
        // First pixel : R=left.R=200, G=right.G=20, B=right.B=30, A=max(255,128)=255.
        assert_eq!(&out[0..4], &[200, 20, 30, 255]);
    }

    #[test]
    fn length_mismatch_rejected() {
        let l = vec![0u8; 16]; // 2x2 RGBA
        let r = vec![0u8; 12]; // wrong size
        let res = compose_side_by_side(&l, &r, 2, 2);
        assert_eq!(res, Err(ComposeErr::LengthMismatch));
        // Also when both match each other but not w*h*4.
        let l = vec![0u8; 12];
        let r = vec![0u8; 12];
        let res = compose_anaglyph_red_cyan(&l, &r, 2, 2);
        assert_eq!(res, Err(ComposeErr::LengthMismatch));
    }

    #[test]
    fn zero_dim_rejected() {
        let l = vec![0u8; 4];
        let r = vec![0u8; 4];
        assert_eq!(compose_side_by_side(&l, &r, 0, 1), Err(ComposeErr::ZeroDimension));
        assert_eq!(compose_top_bottom(&l, &r, 1, 0), Err(ComposeErr::ZeroDimension));
        assert_eq!(compose_anaglyph_red_cyan(&l, &r, 0, 0), Err(ComposeErr::ZeroDimension));
    }

    #[test]
    fn empty_input_rejected() {
        let l: Vec<u8> = vec![];
        let r = vec![0u8; 4];
        assert_eq!(compose_side_by_side(&l, &r, 1, 1), Err(ComposeErr::EmptyInput));
        let l = vec![0u8; 4];
        let r: Vec<u8> = vec![];
        assert_eq!(compose_top_bottom(&l, &r, 1, 1), Err(ComposeErr::EmptyInput));
    }
}
