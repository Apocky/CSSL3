// § render_stub : trait-shape for cssl-render-v2 + cssl-spectral-render +
// § cssl-fractal-amp wiring at G1-tier-3. This crate provides MOCK only.

/// § TourRenderPipeline — minimal contract a pipeline must satisfy to drive
/// a deterministic frame-by-frame tour. The real implementations (in
/// cssl-render-v2 / cssl-spectral-render / cssl-fractal-amp) hook this trait
/// at G1-tier-3 integration.
///
/// Contract :
///   `step(frame, seed)` returns a 32-byte BLAKE3 digest of the rendered
///   frame-payload. MUST be deterministic in `(frame, seed)` and self-state.
pub trait TourRenderPipeline {
    fn step(&mut self, frame: u64, seed: [u8; 32]) -> [u8; 32];
    fn name(&self) -> &'static str;
}

/// § AudioStub — placeholder audio handle (G1-tier-3 wires real engine).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AudioStub {
    pub volume_q15: u16,
    pub muted: bool,
}

/// § MockRenderPipeline — deterministic mock used for tests + dev tours.
/// Output digest = BLAKE3(name · frame · seed · self.salt).
#[derive(Debug, Clone)]
pub struct MockRenderPipeline {
    pub label: &'static str,
    pub salt:  [u8; 32],
}

impl MockRenderPipeline {
    pub fn new(label: &'static str, salt: [u8; 32]) -> Self {
        Self { label, salt }
    }
}

impl TourRenderPipeline for MockRenderPipeline {
    fn step(&mut self, frame: u64, seed: [u8; 32]) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(self.label.as_bytes());
        h.update(&frame.to_le_bytes());
        h.update(&seed);
        h.update(&self.salt);
        *h.finalize().as_bytes()
    }
    fn name(&self) -> &'static str { self.label }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_pipeline_step_deterministic() {
        let mut a = MockRenderPipeline::new("mock", [1u8; 32]);
        let mut b = MockRenderPipeline::new("mock", [1u8; 32]);
        let r1 = a.step(7, [2u8; 32]);
        let r2 = b.step(7, [2u8; 32]);
        assert_eq!(r1, r2);
    }

    #[test]
    fn mock_pipeline_changes_with_frame() {
        let mut a = MockRenderPipeline::new("mock", [1u8; 32]);
        let r1 = a.step(0, [2u8; 32]);
        let r2 = a.step(1, [2u8; 32]);
        assert_ne!(r1, r2);
    }

    #[test]
    fn pipeline_satisfies_trait_via_dyn() {
        let mut p: Box<dyn TourRenderPipeline> = Box::new(MockRenderPipeline::new("dyn", [3u8; 32]));
        let r = p.step(42, [4u8; 32]);
        assert_eq!(r.len(), 32);
        assert_eq!(p.name(), "dyn");
    }

    #[test]
    fn audio_stub_default_silent() {
        let a = AudioStub::default();
        assert_eq!(a.volume_q15, 0);
        assert!(!a.muted);
    }
}
