//! § reservoir.rs — reservoir-N=4096 + k-anon-floor + sovereign-revoke.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § Reservoir
//!   Holds the most-recent N=4096 quality-signals as bit-packed 16-byte records.
//!   Eviction is ring-buffer FIFO ; bursty single-player can NOT drown the
//!   reservoir because k-anon-floor enforcement gates bias-updates at READ-time.
//!
//! § QualitySignalRecord layout (16 bytes fixed)
//!
//! ```text
//!  offset | bytes | field            | semantic
//!  -------+-------+------------------+---------------------------------
//!    0    |   4   | template_id      | u32 little-endian
//!    4    |   1   | archetype_id     | u8 (0..ARCHETYPE_COUNT)
//!    5    |   1   | reserved         | u8 (zero)
//!    6    |   4   | signal_pack4     | QualitySignal::pack4()
//!   10    |   4   | player_handle    | u32 fingerprint (k-anon target)
//!   14    |   2   | frame_offset     | u16 differential frame from created_at
//! ```
//!
//! § PlayerHandle
//!   Opaque 32-bit fingerprint. The reservoir does NOT know the wall-clock
//!   identity behind a handle — sibling-W11 (hash-pseudonym) is responsible
//!   for mapping wallet-PK → opaque u32. This crate enforces uniqueness-count
//!   only.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::signal::QualitySignal;
use crate::bias_map::{ArchetypeId, TemplateId};

// ───────────────────────────────────────────────────────────────────────────
// § Constants
// ───────────────────────────────────────────────────────────────────────────

/// Reservoir hold-N most-recent signals. Power-of-two so wrap-mod is mask.
pub const RESERVOIR_CAPACITY: usize = 4096;

/// Default k-anon floor : bias-update may only fire for a (template · archetype)
/// cell after ≥ this-many DISTINCT player-fingerprints have contributed.
pub const K_ANON_FLOOR_DEFAULT: u32 = 10;

// ───────────────────────────────────────────────────────────────────────────
// § Index types
// ───────────────────────────────────────────────────────────────────────────

/// Opaque 32-bit player-fingerprint. Hash-pseudonym maintained by sibling-W11.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlayerHandle(pub u32);

// ───────────────────────────────────────────────────────────────────────────
// § QualitySignalRecord — bit-packed 16-byte fixed.
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualitySignalRecord {
    pub template_id: TemplateId,
    pub archetype_id: ArchetypeId,
    pub signal: QualitySignal,
    pub player: PlayerHandle,
    /// Frame-offset (differential) from `Reservoir::created_at_frame`. u16 wraps
    /// every ~65k frames @ 60Hz ≈ 18 minutes of wall-clock — sufficient for
    /// reservoir-drain-cadence ; longer horizons handled by Σ-Chain anchor.
    pub frame_offset: u16,
}

impl QualitySignalRecord {
    pub const fn new(
        template_id: TemplateId,
        archetype_id: ArchetypeId,
        signal: QualitySignal,
        player: PlayerHandle,
        frame_offset: u16,
    ) -> Self {
        Self {
            template_id,
            archetype_id,
            signal,
            player,
            frame_offset,
        }
    }

    /// Pack into 16 bytes per the layout-doc above.
    pub fn pack16(self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[0..4].copy_from_slice(&self.template_id.0.to_le_bytes());
        out[4] = self.archetype_id.0;
        out[5] = 0; // reserved
        out[6..10].copy_from_slice(&self.signal.pack4());
        out[10..14].copy_from_slice(&self.player.0.to_le_bytes());
        out[14..16].copy_from_slice(&self.frame_offset.to_le_bytes());
        out
    }

    /// Inverse of pack16. Returns None on bad signal-tag.
    pub fn unpack16(bytes: [u8; 16]) -> Option<Self> {
        let template_id = TemplateId(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        let archetype_id = ArchetypeId(bytes[4]);
        let signal = QualitySignal::unpack4([bytes[6], bytes[7], bytes[8], bytes[9]])?;
        let player = PlayerHandle(u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]));
        let frame_offset = u16::from_le_bytes([bytes[14], bytes[15]]);
        Some(Self {
            template_id,
            archetype_id,
            signal,
            player,
            frame_offset,
        })
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Errors
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ReservoirError {
    /// Signal was rejected because the source-player-fingerprint is on the
    /// revocation-set (sovereign-revoke previously fired).
    #[error("source player handle {0} is on revocation-set")]
    SourceRevoked(u32),
    /// Archetype-id was out of canonical range.
    #[error("archetype-id {0} out of canonical range")]
    ArchetypeOutOfRange(u8),
    /// Anti-poisoning : single player exceeded the per-cell rate-limit.
    /// Default = max 64 contributions to a single (template · archetype) cell
    /// from a single player ; further contributions are dropped silently
    /// (returned as Err so caller can audit-emit).
    #[error("anti-poisoning : player {player} exceeded per-cell-rate-limit on cell ({template}, {archetype})")]
    PerCellRateLimit {
        player: u32,
        template: u32,
        archetype: u8,
    },
}

// ───────────────────────────────────────────────────────────────────────────
// § Reservoir
// ───────────────────────────────────────────────────────────────────────────

/// Per-cell anti-poisoning : at most this many contributions from a single
/// (player · template · archetype) tuple are accepted into the reservoir.
/// Beyond this, [`Reservoir::ingest`] returns [`ReservoirError::PerCellRateLimit`].
pub const PER_CELL_RATE_LIMIT: u32 = 64;

/// Fixed-N ring of QualitySignalRecord with k-anon-floor enforcement.
#[derive(Debug)]
pub struct Reservoir {
    /// Pre-allocated ring of N records.
    records: Box<[Option<QualitySignalRecord>; RESERVOIR_CAPACITY]>,
    /// Head-pointer (next write index).
    head: usize,
    /// Total signals ever ingested (monotone).
    total_ingested: u64,
    /// Σ-mask audit-frame baseline ; record.frame_offset is u16 delta from this.
    /// Read by [`Self::created_at_frame`] for differential-time reconstruction.
    created_at_frame: u32,
    /// Set of revoked player-handles. Future contributions are dropped + past
    /// contributions are removed at revoke-time by [`Self::sovereign_revoke`].
    revoked_players: BTreeSet<PlayerHandle>,
    /// Per-(player · template · archetype) contribution counter — enforces
    /// [`PER_CELL_RATE_LIMIT`].
    per_cell_counts: BTreeMap<(PlayerHandle, TemplateId, ArchetypeId), u32>,
    /// k-anon floor (instance-tunable ; default = K_ANON_FLOOR_DEFAULT).
    k_anon_floor: u32,
}

impl Reservoir {
    pub fn new(created_at_frame: u32) -> Self {
        Self::with_k_anon_floor(created_at_frame, K_ANON_FLOOR_DEFAULT)
    }

    pub fn with_k_anon_floor(created_at_frame: u32, k_anon_floor: u32) -> Self {
        Self {
            records: Box::new([None; RESERVOIR_CAPACITY]),
            head: 0,
            total_ingested: 0,
            created_at_frame,
            revoked_players: BTreeSet::new(),
            per_cell_counts: BTreeMap::new(),
            k_anon_floor: k_anon_floor.max(2), // floor at 2 ; never <2 (would defeat k-anon)
        }
    }

    pub const fn capacity(&self) -> usize {
        RESERVOIR_CAPACITY
    }

    pub const fn k_anon_floor(&self) -> u32 {
        self.k_anon_floor
    }

    /// Σ-mask audit-frame baseline ; signal-record frame_offset is u16-delta from this.
    pub const fn created_at_frame(&self) -> u32 {
        self.created_at_frame
    }

    pub const fn total_ingested(&self) -> u64 {
        self.total_ingested
    }

    /// Number of currently-occupied slots.
    pub fn occupied(&self) -> usize {
        self.records.iter().filter(|r| r.is_some()).count()
    }

    pub fn is_player_revoked(&self, player: PlayerHandle) -> bool {
        self.revoked_players.contains(&player)
    }

    /// Ingest a signal into the reservoir. FIFO eviction.
    pub fn ingest(&mut self, rec: QualitySignalRecord) -> Result<(), ReservoirError> {
        if !rec.archetype_id.is_canonical() {
            return Err(ReservoirError::ArchetypeOutOfRange(rec.archetype_id.0));
        }
        if self.revoked_players.contains(&rec.player) {
            return Err(ReservoirError::SourceRevoked(rec.player.0));
        }
        let key = (rec.player, rec.template_id, rec.archetype_id);
        let count = self.per_cell_counts.entry(key).or_insert(0);
        if *count >= PER_CELL_RATE_LIMIT {
            return Err(ReservoirError::PerCellRateLimit {
                player: rec.player.0,
                template: rec.template_id.0,
                archetype: rec.archetype_id.0,
            });
        }
        *count = count.saturating_add(1);

        // FIFO eviction : if the slot has an old record, decrement its
        // per-cell-count so eviction releases the rate-limit budget.
        let old = self.records[self.head].take();
        if let Some(old_rec) = old {
            let old_key = (old_rec.player, old_rec.template_id, old_rec.archetype_id);
            if let Some(c) = self.per_cell_counts.get_mut(&old_key) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    self.per_cell_counts.remove(&old_key);
                }
            }
        }
        self.records[self.head] = Some(rec);
        self.head = (self.head + 1) % RESERVOIR_CAPACITY;
        self.total_ingested = self.total_ingested.saturating_add(1);
        Ok(())
    }

    /// Iterate all live records in slot-order (NOT chronological).
    pub fn iter_records(&self) -> impl Iterator<Item = &QualitySignalRecord> {
        self.records.iter().filter_map(|r| r.as_ref())
    }

    /// Count of distinct PlayerHandle values that have contributed signals
    /// to a specific (template · archetype) cell.
    pub fn distinct_players_for_cell(
        &self,
        template: TemplateId,
        archetype: ArchetypeId,
    ) -> u32 {
        let mut set: BTreeSet<PlayerHandle> = BTreeSet::new();
        for r in self.iter_records() {
            if r.template_id == template && r.archetype_id == archetype {
                set.insert(r.player);
            }
        }
        set.len() as u32
    }

    /// Test whether the (template · archetype) cell currently meets the
    /// k-anon-floor. Used by the loop-tick gate.
    pub fn cell_meets_k_anon(&self, template: TemplateId, archetype: ArchetypeId) -> bool {
        self.distinct_players_for_cell(template, archetype) >= self.k_anon_floor
    }

    /// Aggregate the Q14 bias-delta for a (template · archetype) cell from
    /// all currently-held signals (saturating-sum). Returns None if k-anon-floor
    /// is not met (the loop-tick must skip this cell silently).
    pub fn aggregate_cell_delta(
        &self,
        template: TemplateId,
        archetype: ArchetypeId,
    ) -> Option<i16> {
        if !self.cell_meets_k_anon(template, archetype) {
            return None;
        }
        let mut sum: i32 = 0;
        let mut count: u32 = 0;
        for r in self.iter_records() {
            if r.template_id == template && r.archetype_id == archetype {
                sum = sum.saturating_add(i32::from(r.signal.to_q14_delta()));
                count = count.saturating_add(1);
            }
        }
        if count == 0 {
            return Some(0);
        }
        // Average to keep magnitudes bounded as N grows.
        let avg = sum / count.min(i32::MAX as u32) as i32;
        Some(avg.clamp(
            i32::from(crate::bias_map::BIAS_Q14_MIN),
            i32::from(crate::bias_map::BIAS_Q14_MAX),
        ) as i16)
    }

    /// Iterate all (template · archetype) cells that currently meet k-anon
    /// floor. Stable ascending order for replay-determinism.
    pub fn k_anon_cells(&self) -> Vec<(TemplateId, ArchetypeId)> {
        let mut counts: BTreeMap<(TemplateId, ArchetypeId), BTreeSet<PlayerHandle>> = BTreeMap::new();
        for r in self.iter_records() {
            counts
                .entry((r.template_id, r.archetype_id))
                .or_default()
                .insert(r.player);
        }
        counts
            .into_iter()
            .filter_map(|(k, v)| if v.len() as u32 >= self.k_anon_floor { Some(k) } else { None })
            .collect()
    }

    /// Sovereign-revoke ALL contributions from a player. Past contributions
    /// are scrubbed from the reservoir AND the player-handle is added to the
    /// revocation-set for future ingests.
    ///
    /// § PRIME_DIRECTIVE § 5 revocability : revocation cascades through any
    /// downstream bias-state ; the loop-tick caller is responsible for invoking
    /// [`crate::bias_map::TemplateBiasMap::reset_cells`] + replay-from-reservoir
    /// to rebuild the bias-map from the post-revoke reservoir contents.
    ///
    /// § Returns the number of records scrubbed.
    pub fn sovereign_revoke(&mut self, player: PlayerHandle) -> u32 {
        let mut scrubbed = 0;
        for slot in self.records.iter_mut() {
            if let Some(r) = slot {
                if r.player == player {
                    let old_key = (r.player, r.template_id, r.archetype_id);
                    if let Some(c) = self.per_cell_counts.get_mut(&old_key) {
                        *c = c.saturating_sub(1);
                        if *c == 0 {
                            self.per_cell_counts.remove(&old_key);
                        }
                    }
                    *slot = None;
                    scrubbed += 1;
                }
            }
        }
        self.revoked_players.insert(player);
        scrubbed
    }

    /// True iff a player-handle currently has any record in the reservoir.
    pub fn player_has_records(&self, player: PlayerHandle) -> bool {
        self.iter_records().any(|r| r.player == player)
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(t: u32, a: u8, sig: QualitySignal, p: u32, off: u16) -> QualitySignalRecord {
        QualitySignalRecord::new(TemplateId(t), ArchetypeId(a), sig, PlayerHandle(p), off)
    }

    #[test]
    fn ingest_basic_and_pack_roundtrip() {
        let mut r = Reservoir::new(0);
        let rec1 = rec(7, 3, QualitySignal::SandboxPass, 100, 50);
        r.ingest(rec1).unwrap();
        assert_eq!(r.total_ingested(), 1);
        assert_eq!(r.occupied(), 1);
        // pack/unpack
        let bytes = rec1.pack16();
        let back = QualitySignalRecord::unpack16(bytes).unwrap();
        assert_eq!(rec1, back);
    }

    #[test]
    fn k_anon_floor_blocks_solo_player_bias() {
        let mut r = Reservoir::with_k_anon_floor(0, 10);
        // single player feeding many signals to same cell ⇒ k=1 < 10
        for i in 0..50 {
            let _ = r.ingest(rec(1, 0, QualitySignal::SandboxPass, 999, i));
        }
        assert!(!r.cell_meets_k_anon(TemplateId(1), ArchetypeId(0)));
        assert_eq!(r.aggregate_cell_delta(TemplateId(1), ArchetypeId(0)), None);
        // now 10 distinct players ⇒ k = 10 ≥ floor
        for p in 0..10 {
            r.ingest(rec(2, 0, QualitySignal::SandboxPass, p, 0)).unwrap();
        }
        assert!(r.cell_meets_k_anon(TemplateId(2), ArchetypeId(0)));
        let agg = r.aggregate_cell_delta(TemplateId(2), ArchetypeId(0));
        assert!(agg.is_some());
        assert!(agg.unwrap() > 0);
    }

    #[test]
    fn anti_poisoning_per_cell_rate_limit() {
        let mut r = Reservoir::new(0);
        // single player flooding a single cell : after PER_CELL_RATE_LIMIT
        // contributions, further contributions are rejected.
        for i in 0..PER_CELL_RATE_LIMIT {
            r.ingest(rec(0, 0, QualitySignal::SandboxPass, 1, i as u16))
                .expect("under limit");
        }
        let result = r.ingest(rec(0, 0, QualitySignal::SandboxPass, 1, 9999));
        assert!(matches!(result, Err(ReservoirError::PerCellRateLimit { .. })));
    }

    #[test]
    fn sovereign_revoke_scrubs_and_blocks_future() {
        let mut r = Reservoir::with_k_anon_floor(0, 3);
        // player A and B and C contribute to cell (0, 0).
        for p in [10u32, 11, 12] {
            r.ingest(rec(0, 0, QualitySignal::SandboxPass, p, 0)).unwrap();
        }
        assert!(r.cell_meets_k_anon(TemplateId(0), ArchetypeId(0)));
        // Player A invokes revoke. Distinct-count drops to 2 ⇒ k-anon fails.
        let scrubbed = r.sovereign_revoke(PlayerHandle(10));
        assert_eq!(scrubbed, 1);
        assert!(!r.cell_meets_k_anon(TemplateId(0), ArchetypeId(0)));
        // Future contribution from revoked player is rejected.
        let result = r.ingest(rec(0, 0, QualitySignal::SandboxPass, 10, 5));
        assert!(matches!(result, Err(ReservoirError::SourceRevoked(10))));
        assert!(r.is_player_revoked(PlayerHandle(10)));
    }

    #[test]
    fn fifo_eviction_at_capacity() {
        let mut r = Reservoir::new(0);
        // Fill ring + overflow by 1.
        for i in 0..(RESERVOIR_CAPACITY + 1) {
            r.ingest(rec(i as u32 % 1000, 0, QualitySignal::SandboxPass, i as u32, 0))
                .unwrap();
        }
        // After CAPACITY+1 ingests, occupied is exactly CAPACITY.
        assert_eq!(r.occupied(), RESERVOIR_CAPACITY);
        assert_eq!(r.total_ingested() as usize, RESERVOIR_CAPACITY + 1);
    }

    #[test]
    fn k_anon_cells_returns_only_qualifying() {
        let mut r = Reservoir::with_k_anon_floor(0, 3);
        // cell (0,0) gets 3 distinct players — qualifies.
        for p in [1u32, 2, 3] {
            r.ingest(rec(0, 0, QualitySignal::SandboxPass, p, 0)).unwrap();
        }
        // cell (1,0) gets 1 player — does not qualify.
        r.ingest(rec(1, 0, QualitySignal::SandboxPass, 99, 0)).unwrap();
        let cells = r.k_anon_cells();
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0], (TemplateId(0), ArchetypeId(0)));
    }
}
