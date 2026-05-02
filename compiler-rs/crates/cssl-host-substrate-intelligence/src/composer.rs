//! § composer — the procedural-composition engine.
//!
//! Walks the substrate axes + entropy stream to emit UTF-8 morphologically-
//! coherent text into a caller-provided buffer. Stateless across calls,
//! deterministic given identical inputs, bounded by output buffer size.

use crate::axes::SubstrateAxes;
use crate::morpheme::{
    ARTICLES, CODAS, CONNECTIVES, NUCLEI, ONSETS, PREFIXES, PUNCT_ENDINGS, SUFFIXES,
};
use crate::role_envelope::{pick, ClauseShape};
use crate::{ComposeKind, Role};

/// Streaming morpheme composer.
pub struct Composer<'a> {
    role: Role,
    kind: ComposeKind,
    seed: u64,
    axes: &'a SubstrateAxes,
    /// Counter that walks the entropy reserve as morphemes consume it.
    cursor: u64,
}

impl<'a> Composer<'a> {
    pub fn new(
        role: Role,
        kind: ComposeKind,
        seed: u64,
        axes: &'a SubstrateAxes,
        params: &[u8],
    ) -> Self {
        // Mix params into the cursor so identical (role, kind, seed)
        // with different params produces different output.
        let mut cursor = seed;
        for (i, b) in params.iter().enumerate() {
            cursor = cursor.wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add((*b as u64) << ((i & 7) * 8));
        }
        Self { role, kind, seed, axes, cursor }
    }

    /// Pull a fresh u32 from the deterministic stream.
    fn next_u32(&mut self) -> u32 {
        // SplitMix64 step for high-quality avalanche from a counter.
        self.cursor = self.cursor.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.cursor;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        // Mix in entropy from the BLAKE3 reserve so distinct axes yield
        // distinct streams even at the same cursor value.
        let blend = self.axes.entropy_u32_at((self.cursor as usize) % 21);
        ((z ^ (z >> 31)) as u32) ^ blend
    }

    /// Sample an index into a slice of length `n`.
    fn pick<T>(&mut self, slice: &[T]) -> usize {
        (self.next_u32() as usize) % slice.len().max(1)
    }

    /// Axis-weighted index. `axis_weight` is `0..=255`. Higher weight
    /// biases towards the high-end of the slice, lower weight towards
    /// the low-end. Useful for "antiquity high → favor old-feeling
    /// codas".
    fn pick_weighted<T>(&mut self, slice: &[T], axis_weight: u8) -> usize {
        let r = (self.next_u32() & 0xFFFF) as u32; // 0..65535
        let bias = (axis_weight as u32) * 256; // 0..65280
        // Blend: 50% axis-bias, 50% true entropy.
        let blended = (r / 2) + (bias / 2);
        ((blended as usize) % slice.len().max(1)).min(slice.len() - 1)
    }

    /// Emit one stem (consonant-vowel-consonant) with optional prefix +
    /// suffix into the output buffer. Returns bytes written.
    fn push_stem(&mut self, out: &mut [u8], cursor: usize) -> usize {
        let mut written = 0;
        // Optional prefix (axis-weighted by antiquity for archaic feel).
        if self.next_u32() & 0x7 == 0 {
            let p = self.pick_weighted(PREFIXES, self.axes.antiquity);
            written += copy_into(out, cursor + written, PREFIXES[p]);
        }
        // Onset, nucleus, coda — concatenated into the same buffer.
        let o = self.pick_weighted(ONSETS, self.axes.resonance);
        let n = self.pick_weighted(NUCLEI, self.axes.intimacy);
        let c = self.pick_weighted(CODAS, self.axes.antiquity);
        written += copy_into(out, cursor + written, ONSETS[o]);
        written += copy_into(out, cursor + written, NUCLEI[n]);
        written += copy_into(out, cursor + written, CODAS[c]);
        // Optional suffix (axis-weighted by verbosity).
        let suffix_threshold = (self.axes.verbosity as u32) * 16;
        if (self.next_u32() & 0xFFFF) < suffix_threshold {
            let s = self.pick_weighted(SUFFIXES, self.axes.verbosity);
            written += copy_into(out, cursor + written, SUFFIXES[s]);
        }
        written
    }

    /// Emit one clause (article + N stems + connective).
    fn push_clause(&mut self, shape: ClauseShape, out: &mut [u8], cursor: usize) -> usize {
        let mut written = 0;
        // Leading article on roughly half of clauses (axis-tunable).
        if self.next_u32() & 0x1 == 0 {
            let a = self.pick_weighted(ARTICLES, self.axes.concreteness);
            written += copy_into(out, cursor + written, ARTICLES[a]);
        }
        let stems = shape.stem_count();
        for i in 0..stems {
            written += self.push_stem(out, cursor + written);
            // Inter-stem space (replaced by punctuation at clause-end).
            if i + 1 < stems {
                written += copy_into(out, cursor + written, " ");
            }
        }
        written
    }

    /// Emit the entire composition into `out`. Returns bytes written.
    pub fn write_into(&mut self, out: &mut [u8]) -> usize {
        let envelope = pick(self.role, self.kind);
        let mut cursor = 0;
        // Capitalize first letter post-emission (handled in finalize).
        let start = cursor;
        for clause_idx in 0..envelope.clause_count {
            let shape = envelope.clauses[clause_idx as usize];
            let n = self.push_clause(shape, out, cursor);
            cursor += n;
            // Connective between clauses (skip after last).
            if clause_idx + 1 < envelope.clause_count {
                let conn = self.pick_weighted(CONNECTIVES, self.axes.dynamism);
                cursor += copy_into(out, cursor, CONNECTIVES[conn]);
            }
        }
        // Closing punctuation.
        let punct = if envelope.force_declarative {
            "."
        } else {
            let p = self.pick_weighted(PUNCT_ENDINGS, self.axes.solemnity);
            PUNCT_ENDINGS[p]
        };
        cursor += copy_into(out, cursor, punct);
        // Capitalize the first ASCII letter.
        if cursor > start && start < out.len() {
            let first = out[start];
            if first.is_ascii_lowercase() {
                out[start] = first.to_ascii_uppercase();
            }
        }
        cursor
    }
}

/// Copy `src` into `out` starting at `cursor`. Truncates if it would
/// overflow. Returns bytes copied.
fn copy_into(out: &mut [u8], cursor: usize, src: &str) -> usize {
    let bytes = src.as_bytes();
    let space = out.len().saturating_sub(cursor);
    let n = bytes.len().min(space);
    if n > 0 {
        out[cursor..cursor + n].copy_from_slice(&bytes[..n]);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::axes::SubstrateAxes;

    #[test]
    fn write_into_emits_bytes() {
        let axes = SubstrateAxes::derive(Role::Gm, ComposeKind::DialogueLine, 1, b"");
        let mut c = Composer::new(Role::Gm, ComposeKind::DialogueLine, 1, &axes, b"");
        let mut out = [0u8; 256];
        let n = c.write_into(&mut out);
        assert!(n > 0);
        assert!(n <= out.len());
    }

    #[test]
    fn write_into_respects_buffer_size() {
        let axes = SubstrateAxes::derive(Role::Gm, ComposeKind::DialogueLine, 1, b"");
        let mut c = Composer::new(Role::Gm, ComposeKind::DialogueLine, 1, &axes, b"");
        let mut out = [0u8; 8];
        let n = c.write_into(&mut out);
        assert!(n <= 8);
    }

    #[test]
    fn copy_into_truncates_safely() {
        let mut out = [0u8; 4];
        let n = copy_into(&mut out, 0, "hello");
        assert_eq!(n, 4);
        assert_eq!(&out[..4], b"hell");
    }
}
