//! § cssl-hdc::sdm — Sparse Distributed Memory (Kanerva 1988)
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Kanerva's Sparse Distributed Memory model : a content-addressable
//!   memory of N hard-cells, each carrying a fixed D-dim "address"
//!   hypervector and a bipolar accumulator vector. To **write** an
//!   `(address, value)` pair, every hard-cell within Hamming radius `r`
//!   of `address` increments / decrements its accumulator according to
//!   the `value` bits. To **read** an `address`, the same selection
//!   step picks the active cells, and their accumulators are summed
//!   bit-wise then thresholded at zero to produce a binary output.
//!
//!   The mathematical insight : in a D-dim Hamming space, a Hamming
//!   ball of radius `r ≈ D/2 - sqrt(D)/2` contains roughly `1/2`
//!   of the volume. Picking cells at random gives access to roughly
//!   `N/2` cells on every access — enough redundancy for graceful
//!   degradation but few enough that distinct addresses don't entirely
//!   overlap their cell-sets.
//!
//! § PARAMETERIZATION
//!   - `D` : dimension (10000 for genome compatibility).
//!   - `N` : number of hard-cells. Larger → more capacity but more
//!     storage. Practical band : 1k..100k. We make `N` a const-generic
//!     so the cell array can be a `Box<[HdcCell]>` of known length.
//!   - `radius` : Hamming radius for cell selection. Default
//!     `D/2 - sqrt(D)/2 ≈ 4500` for D = 10000 (selects ≈ N/2 cells per
//!     access on random addresses). Configurable so callers can trade
//!     write-spread for read-precision.
//!
//! § COUNTERS
//!   Each cell's accumulator is a `Vec<i16>` of length D. Writing a
//!   bipolar `+1` increments the counter by 1 ; writing `-1` decrements.
//!   Reading takes the sign of the per-bit summed counter. We use `i16`
//!   to allow at least 32k writes per cell before overflow risk —
//!   adequate for episodic memory at expected write-rates.
//!
//! § PERF NOTE
//!   Selection is O(N) Hamming-distance computations per access,
//!   each costing ≈ 157 popcount-XOR ops. For N = 1024 that's
//!   ≈ 160k ops per access — sub-millisecond. Capacity is roughly
//!   `0.1 * N * D / log(N)` bits (Kanerva's empirical bound).
//!
//! § INTEGRATION
//!   The substrate-level `EpisodicMemory` slice (forward-referenced) is
//!   expected to wrap a [`SparseDistributedMemory`] with
//!   `D = HDC_DIM = 10000` and `N = 4096` cells, supplying the
//!   "have I seen something like this" predicate that creature-AI uses
//!   for novelty detection. Active inference (Friston-style) treats
//!   high-novelty inputs as drivers for exploration ; SDM is the
//!   constant-time substrate that makes this practical at thousands of
//!   creatures × thousands of frames.

use crate::hypervector::Hypervector;
use crate::prng::SplitMix64;
use crate::similarity::hamming_distance;

/// § One hard-cell in a Sparse Distributed Memory.
#[derive(Debug, Clone)]
pub struct HdcCell<const D: usize> {
    /// § The cell's fixed address in hyperspace. Set at construction
    ///   from a seeded random distribution.
    address: Hypervector<D>,
    /// § Per-bit signed accumulator. `counters[i]` is the running tally
    ///   of writes to bit `i` : `+1` for each bipolar `+1` written,
    ///   `-1` for each bipolar `-1` written. Read takes the sign.
    counters: Vec<i16>,
}

impl<const D: usize> HdcCell<D> {
    /// § Construct a cell with random address from the given seed and
    ///   zero-initialized counters.
    fn from_seed(seed: u64) -> Self {
        Self {
            address: Hypervector::random_from_seed(seed),
            counters: vec![0i16; D],
        }
    }

    /// § Borrow the cell's address.
    #[must_use]
    pub fn address(&self) -> &Hypervector<D> {
        &self.address
    }

    /// § Borrow the counter array.
    #[must_use]
    pub fn counters(&self) -> &[i16] {
        &self.counters
    }
}

/// § Sparse Distributed Memory of N cells over D-dim hypervectors.
#[derive(Debug, Clone)]
pub struct SparseDistributedMemory<const D: usize, const N: usize> {
    /// § The hard-cell array. Length is exactly N.
    cells: Box<[HdcCell<D>]>,
    /// § Hamming radius for cell-selection. Cells whose address is
    ///   within this distance of the access address are activated.
    radius: u32,
}

impl<const D: usize, const N: usize> SparseDistributedMemory<D, N> {
    /// § Construct an SDM with random cell addresses derived from
    ///   `master_seed` and the default selection radius. Default
    ///   radius is `D / 2 - integer_sqrt(D)` which selects roughly
    ///   half the cells per access on random inputs.
    #[must_use]
    pub fn new(master_seed: u64) -> Self {
        let radius = Self::default_radius();
        Self::with_radius(master_seed, radius)
    }

    /// § Construct with a caller-specified Hamming-distance radius.
    ///   `radius == 0` means only exact-address matches activate
    ///   (degenerate to a hash-table). `radius == D` means every cell
    ///   activates on every access (degenerate to a single bundled
    ///   hypervector).
    #[must_use]
    pub fn with_radius(master_seed: u64, radius: u32) -> Self {
        let mut rng = SplitMix64::new(master_seed);
        let cells: Vec<HdcCell<D>> = (0..N)
            .map(|_| {
                let cell_seed = rng.next();
                HdcCell::from_seed(cell_seed)
            })
            .collect();
        Self {
            cells: cells.into_boxed_slice(),
            radius,
        }
    }

    /// § Default cell-selection radius. Approximation of
    ///   `D/2 - sqrt(D)/2` rounded to an integer. For D = 10000 this
    ///   gives 4950, selecting ≈ half the cells on random accesses.
    #[must_use]
    pub fn default_radius() -> u32 {
        if D == 0 {
            return 0;
        }
        let half = D as u32 / 2;
        let sqrt_d_half = (Self::integer_sqrt(D as u32)) / 2;
        half.saturating_sub(sqrt_d_half)
    }

    /// § Integer square root via Newton's method ; const-friendly. We
    ///   could use `(x as f64).sqrt() as u32` but the float path
    ///   would not be deterministic across host floating-point modes.
    fn integer_sqrt(x: u32) -> u32 {
        if x < 2 {
            return x;
        }
        let mut guess = x / 2;
        for _ in 0..20 {
            let next = (guess + x / guess) / 2;
            if next >= guess {
                return guess;
            }
            guess = next;
        }
        guess
    }

    /// § Number of cells (= N).
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// § Current selection radius.
    #[must_use]
    pub fn radius(&self) -> u32 {
        self.radius
    }

    /// § Set a new selection radius. Does not invalidate stored data ;
    ///   the radius affects which cells participate on the next access.
    pub fn set_radius(&mut self, radius: u32) {
        self.radius = radius;
    }

    /// § Borrow the cell array (for diagnostics / testing).
    #[must_use]
    pub fn cells(&self) -> &[HdcCell<D>] {
        &self.cells
    }

    /// § Identify the indices of cells activated by an access address.
    ///   A cell is activated iff `hamming(cell.address, access) ≤ radius`.
    #[must_use]
    pub fn activated_cells(&self, access: &Hypervector<D>) -> Vec<usize> {
        self.cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| hamming_distance(cell.address(), access) <= self.radius)
            .map(|(i, _)| i)
            .collect()
    }

    /// § Write an `(address, value)` pair to the SDM. Every activated
    ///   cell increments / decrements its counters according to the
    ///   bipolar interpretation of the value bits : `+1` for set,
    ///   `-1` for clear. Counters saturate at `i16::MAX / MIN` rather
    ///   than overflow.
    pub fn write(&mut self, address: &Hypervector<D>, value: &Hypervector<D>) {
        let active = self.activated_cells(address);
        for cell_idx in active {
            let cell = &mut self.cells[cell_idx];
            for bit in 0..D {
                let delta: i16 = if value.bit(bit) { 1 } else { -1 };
                cell.counters[bit] = cell.counters[bit].saturating_add(delta);
            }
        }
    }

    /// § Read at `address` : sum the counters of every activated cell
    ///   bit-wise, then threshold at zero. The output bit `i` is set
    ///   iff the bit-`i` sum across activated cells is strictly
    ///   positive. Ties (zero) round to `0` (clear bit) so an empty
    ///   memory reads as the all-zero hypervector.
    #[must_use]
    pub fn read(&self, address: &Hypervector<D>) -> SdmReadout<D> {
        let active = self.activated_cells(address);
        let mut sums = vec![0i32; D];
        for cell_idx in &active {
            let cell = &self.cells[*cell_idx];
            for (sum, &counter) in sums.iter_mut().zip(cell.counters.iter()) {
                *sum += counter as i32;
            }
        }
        let mut output: Hypervector<D> = Hypervector::zero();
        for (bit, &sum) in sums.iter().enumerate() {
            if sum > 0 {
                output.set_bit(bit, true);
            }
        }
        SdmReadout {
            value: output,
            n_activated: active.len(),
            confidence: Self::confidence_score(&sums, active.len()),
        }
    }

    /// § Compute a confidence score for a readout : the average
    ///   absolute counter-sum across all D bits, normalized by the
    ///   number of activated cells. High confidence (close to 1.0)
    ///   means the per-bit signals are far from zero ; low confidence
    ///   (close to 0.0) means the readout is near-noise.
    fn confidence_score(sums: &[i32], n_activated: usize) -> f32 {
        if n_activated == 0 {
            return 0.0;
        }
        let total: i32 = sums.iter().map(|s| s.unsigned_abs() as i32).sum();
        let avg_abs = (total as f32) / (sums.len() as f32);
        let normalized = avg_abs / (n_activated as f32);
        normalized.clamp(0.0, 1.0)
    }

    /// § Auto-associative recall : SDMs are often used in the
    ///   self-paired mode where `address == value` on write, then
    ///   noisy/partial query on read recovers the cleaned value. This
    ///   helper writes a self-paired entry — equivalent to
    ///   `self.write(&pattern, &pattern)` but expresses intent.
    pub fn write_auto(&mut self, pattern: &Hypervector<D>) {
        self.write(pattern, pattern);
    }
}

/// § Result of an SDM read. Carries the recovered hypervector plus
///   diagnostic metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct SdmReadout<const D: usize> {
    /// § The thresholded output hypervector.
    pub value: Hypervector<D>,
    /// § Number of cells that participated in this readout.
    pub n_activated: usize,
    /// § Confidence score in `[0, 1]` derived from per-bit counter
    ///   magnitudes. Higher = stronger signal.
    pub confidence: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::similarity::hamming_distance_normalized;

    /// § New SDM has zero counters everywhere.
    #[test]
    fn new_sdm_is_zero() {
        let sdm: SparseDistributedMemory<1024, 32> = SparseDistributedMemory::new(0xDEAD);
        for cell in sdm.cells() {
            assert!(cell.counters().iter().all(|&c| c == 0));
        }
    }

    /// § SDM has exactly N cells.
    #[test]
    fn sdm_has_n_cells() {
        let sdm: SparseDistributedMemory<256, 100> = SparseDistributedMemory::new(0);
        assert_eq!(sdm.cell_count(), 100);
    }

    /// § Default radius for D = 10000 is approximately 4950.
    #[test]
    fn default_radius_for_genome_dim() {
        let r = SparseDistributedMemory::<10000, 1024>::default_radius();
        // § sqrt(10000) = 100, so D/2 - 50 = 4950.
        assert_eq!(r, 4950);
    }

    /// § Radius 0 means only exact-address activation.
    #[test]
    fn radius_zero_exact_match() {
        let sdm: SparseDistributedMemory<128, 16> =
            SparseDistributedMemory::with_radius(0, 0);
        let probe: Hypervector<128> = Hypervector::random_from_seed(99);
        let active = sdm.activated_cells(&probe);
        // § Random probe vs random cells : Hamming distance ≈ 64 ;
        //   exact match probability is ≈ 0. Should activate 0 cells.
        assert_eq!(active.len(), 0);

        // § If we use one of the cell's own addresses, that cell is
        //   activated.
        let cell_addr = sdm.cells()[3].address().clone();
        let active = sdm.activated_cells(&cell_addr);
        assert!(active.contains(&3));
    }

    /// § Auto-associative recall : write a pattern self-paired, read
    ///   it back, expect high similarity.
    #[test]
    fn auto_associative_recall() {
        let mut sdm: SparseDistributedMemory<10000, 256> = SparseDistributedMemory::new(42);
        let pattern: Hypervector<10000> = Hypervector::random_from_seed(7);
        sdm.write_auto(&pattern);

        let readout = sdm.read(&pattern);
        let d = hamming_distance_normalized(&readout.value, &pattern);
        // § Auto-associative recall should give very low Hamming
        //   distance — well below random baseline 0.5.
        assert!(d < 0.4, "auto-recall failed : d = {d}");
    }

    /// § Hetero-associative : write (key, value), read at key,
    ///   recover value above noise.
    #[test]
    fn hetero_associative_recall() {
        let mut sdm: SparseDistributedMemory<10000, 512> = SparseDistributedMemory::new(99);
        let key: Hypervector<10000> = Hypervector::random_from_seed(1);
        let value: Hypervector<10000> = Hypervector::random_from_seed(2);
        sdm.write(&key, &value);

        let readout = sdm.read(&key);
        let d = hamming_distance_normalized(&readout.value, &value);
        assert!(d < 0.4, "hetero-recall failed : d = {d}");
    }

    /// § Read at unrelated address with a tight radius gives noise. We
    ///   need radius small enough that the activation-set intersection
    ///   between the write-key and the unrelated-key is empty ; with
    ///   default radius (≈ D/2 - √D/2 = 4950) the cell-sets overlap
    ///   substantially and readout still matches value because the
    ///   single write put the same bits in every activated cell.
    ///
    ///   Setting radius = D/4 = 2500 puts the activation-set roughly
    ///   at the cells whose addresses are within Hamming D/4 of the
    ///   probe — for random probes vs random cells (which average D/2
    ///   apart), almost no cells are in this radius, so writes and
    ///   reads at independent addresses don't overlap.
    #[test]
    fn unrelated_address_returns_noise() {
        let mut sdm: SparseDistributedMemory<10000, 512> =
            SparseDistributedMemory::with_radius(101, 2500);
        let key: Hypervector<10000> = Hypervector::random_from_seed(1);
        let value: Hypervector<10000> = Hypervector::random_from_seed(2);
        sdm.write(&key, &value);

        let unrelated_key: Hypervector<10000> = Hypervector::random_from_seed(99999);
        let readout = sdm.read(&unrelated_key);
        // § With small radius, no cells activate on the unrelated read
        //   ⇒ readout is all-zero. Verify : either readout is all-zero
        //   (no activation) or the recovery is dissimilar to the
        //   original value.
        if readout.n_activated == 0 {
            assert_eq!(readout.value.popcount(), 0);
        } else {
            let d = hamming_distance_normalized(&readout.value, &value);
            assert!(d > 0.3, "unrelated recall too clean : d = {d}");
        }
    }

    /// § Multiple writes : each pattern recoverable after writing
    ///   several unrelated patterns.
    #[test]
    fn multiple_writes_recoverable() {
        let mut sdm: SparseDistributedMemory<10000, 1024> = SparseDistributedMemory::new(33);
        let patterns: Vec<Hypervector<10000>> = (0..5)
            .map(|i| Hypervector::random_from_seed(2000 + i as u64))
            .collect();
        for p in &patterns {
            sdm.write_auto(p);
        }

        for (i, p) in patterns.iter().enumerate() {
            let readout = sdm.read(p);
            let d = hamming_distance_normalized(&readout.value, p);
            assert!(d < 0.45, "pattern {i} recall too noisy : d = {d}");
        }
    }

    /// § Confidence score is non-negative and bounded.
    #[test]
    fn confidence_in_range() {
        let mut sdm: SparseDistributedMemory<256, 64> = SparseDistributedMemory::new(0);
        let p: Hypervector<256> = Hypervector::random_from_seed(0);
        sdm.write_auto(&p);
        let readout = sdm.read(&p);
        assert!(readout.confidence >= 0.0);
        assert!(readout.confidence <= 1.0);
    }

    /// § Empty SDM (no writes) reads as all-zero with zero confidence.
    #[test]
    fn empty_sdm_zero_readout() {
        let sdm: SparseDistributedMemory<256, 64> = SparseDistributedMemory::new(0);
        let p: Hypervector<256> = Hypervector::random_from_seed(0);
        let readout = sdm.read(&p);
        assert_eq!(readout.value, Hypervector::zero());
    }

    /// § set_radius takes effect on subsequent reads.
    #[test]
    fn set_radius_affects_activation() {
        let mut sdm: SparseDistributedMemory<256, 32> =
            SparseDistributedMemory::with_radius(0, 0);
        let p: Hypervector<256> = Hypervector::random_from_seed(0);
        let active_zero_radius = sdm.activated_cells(&p);
        sdm.set_radius(256); // activates everything
        let active_full_radius = sdm.activated_cells(&p);
        assert!(active_full_radius.len() > active_zero_radius.len());
        assert_eq!(active_full_radius.len(), 32);
    }
}
