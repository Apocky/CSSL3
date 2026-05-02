//! § ring — Temporal-coherence ring-buffer (depth = 3).
//!
//! Three recent substrate-projection pixel-fields. Display is a per-pixel
//! axis-weighted blend across them. Replaces conventional TAA (temporal
//! anti-aliasing) with a substrate-aware blend whose mode depends on the
//! scene's mood-axes (passed to `blended` via `BlendKind`).

use cssl_host_alien_materialization::PixelField;

/// 5 blend modes per digital_intelligence_render.csl TCB_BLEND_*.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendKind {
    /// Equal-weight average across 3 frames.
    Linear = 0,
    /// Most recent weighted highest, decreasing.
    EaseOut = 1,
    /// Center-weighted (smooths jitter, preserves mid-frame detail).
    EaseInOut = 2,
    /// Spring-style overshoot (high-Solemnity scenes).
    Spring = 3,
    /// No blending — pure most-recent (snappy combat).
    Instant = 4,
}

/// Ring-of-3 pixel fields. The ring is FIFO ; `push` evicts oldest.
#[derive(Debug)]
pub struct TemporalCoherenceRing {
    pub width: u32,
    pub height: u32,
    /// 3 buffers ; index 0 = most-recent, 2 = oldest.
    buffers: Vec<PixelField>,
}

impl TemporalCoherenceRing {
    pub fn new(width: u32, height: u32) -> Self {
        let mut buffers = Vec::with_capacity(3);
        for _ in 0..3 {
            buffers.push(PixelField::new(width, height));
        }
        Self { width, height, buffers }
    }

    /// Push a fresh frame. Evicts the oldest. Returns the evicted frame so
    /// the caller can reuse its allocation if desired (zero-alloc steady-state).
    pub fn push(&mut self, frame: PixelField) -> PixelField {
        // We rotate: insert at index 0, pop from index 2.
        let evicted = self.buffers.pop().expect("ring always has 3 buffers");
        self.buffers.insert(0, frame);
        evicted
    }

    /// Return the per-pixel blended display frame.
    pub fn blended(&self, kind: BlendKind) -> PixelField {
        let mut out = PixelField::new(self.width, self.height);
        let n = self.buffers.len();
        if n == 0 {
            return out;
        }

        let weights: [u32; 3] = match kind {
            BlendKind::Linear => [85, 85, 85],
            BlendKind::EaseOut => [180, 50, 25],
            BlendKind::EaseInOut => [80, 130, 45],
            BlendKind::Spring => [220, 20, 15],
            BlendKind::Instant => [255, 0, 0],
        };
        let total: u32 = weights.iter().sum();
        let total = total.max(1);

        let pix_count = (self.width as usize) * (self.height as usize);
        for i in 0..pix_count {
            let mut r: u32 = 0;
            let mut g: u32 = 0;
            let mut b: u32 = 0;
            let mut a: u32 = 0;
            for fi in 0..n.min(3) {
                let p = self.buffers[fi].pixels[i];
                let w = weights[fi];
                r += (p[0] as u32) * w;
                g += (p[1] as u32) * w;
                b += (p[2] as u32) * w;
                a += (p[3] as u32) * w;
            }
            out.pixels[i] = [
                (r / total).min(255) as u8,
                (g / total).min(255) as u8,
                (b / total).min(255) as u8,
                (a / total).min(255) as u8,
            ];
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_ring_has_three_zeroed_frames() {
        let r = TemporalCoherenceRing::new(4, 4);
        assert_eq!(r.buffers.len(), 3);
        for buf in &r.buffers {
            assert!(buf.pixels.iter().all(|p| *p == [0, 0, 0, 0]));
        }
    }

    #[test]
    fn push_rotates_fifo() {
        let mut r = TemporalCoherenceRing::new(4, 4);
        let mut f1 = PixelField::new(4, 4);
        f1.pixels[0] = [10, 20, 30, 255];
        r.push(f1);
        assert_eq!(r.buffers[0].pixels[0], [10, 20, 30, 255]);
    }

    #[test]
    fn blended_instant_returns_most_recent() {
        let mut r = TemporalCoherenceRing::new(2, 2);
        let mut latest = PixelField::new(2, 2);
        latest.pixels[0] = [100, 100, 100, 255];
        r.push(latest);
        let b = r.blended(BlendKind::Instant);
        assert_eq!(b.pixels[0], [100, 100, 100, 255]);
    }

    #[test]
    fn blended_linear_averages_frames() {
        let mut r = TemporalCoherenceRing::new(2, 2);
        let mut f0 = PixelField::new(2, 2);
        f0.pixels[0] = [120, 0, 0, 255];
        let mut f1 = PixelField::new(2, 2);
        f1.pixels[0] = [0, 120, 0, 255];
        let mut f2 = PixelField::new(2, 2);
        f2.pixels[0] = [0, 0, 120, 255];
        r.push(f2);
        r.push(f1);
        r.push(f0);
        let b = r.blended(BlendKind::Linear);
        // Each weight = 85, total = 255, so each component contributes 120 * 85 / 255 = 40.
        assert!(b.pixels[0][0] >= 35 && b.pixels[0][0] <= 45);
        assert!(b.pixels[0][1] >= 35 && b.pixels[0][1] <= 45);
        assert!(b.pixels[0][2] >= 35 && b.pixels[0][2] <= 45);
    }
}
