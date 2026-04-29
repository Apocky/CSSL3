//! `AssetBudget` — per-asset-class memory budgeting + eviction policy.
//!
//! § DESIGN
//!   Asset systems eventually need to bound memory : a level might list
//!   hundreds of textures, but the GPU and CPU have finite VRAM / RAM.
//!   `AssetBudget` is the canonical surface for that bound at stage-0.
//!
//!   At stage-0 the budget tracks bytes-in-use across logical asset
//!   classes (texture, model, audio, font, blob) and supports two
//!   eviction policies :
//!
//!   - `EvictionPolicy::Lru` — least-recently-used. The oldest
//!     `touch`'d entry is evicted first.
//!   - `EvictionPolicy::SmallestFirst` — evict from the smallest
//!     entry up. Useful for clearing out a long tail of small assets
//!     to make room for one big one.
//!
//!   The budget itself is a passive accounting structure : it accepts
//!   `try_reserve(class, bytes, key)` requests and refuses ones that
//!   would exceed the cap. Eviction is exposed via `evict_to_fit` —
//!   callers decide WHEN to evict, the budget decides WHICH entry.
//!
//! § PRIME-DIRECTIVE
//!   The budget collects no information beyond what the caller hands
//!   it (class + size + key). No introspection, no telemetry, no
//!   leakage outside the structure's lifetime. Drop releases all
//!   tracked entries silently.

use crate::error::{AssetError, Result};

/// Logical asset class for budget accounting.
///
/// The class set is closed at stage-0 to keep the surface auditable.
/// Generic `Other(name)` is the escape hatch for slices that don't fit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AssetClass {
    /// 2D / 3D textures and image assets.
    Texture,
    /// Mesh / scene-graph / animation assets.
    Model,
    /// PCM / compressed audio buffers.
    Audio,
    /// Font / glyph-cache assets.
    Font,
    /// Generic byte-blob (catch-all).
    Blob,
    /// Custom-named class (escape hatch).
    Other(String),
}

impl AssetClass {
    /// Stable string name for logging / errors.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Texture => "texture",
            Self::Model => "model",
            Self::Audio => "audio",
            Self::Font => "font",
            Self::Blob => "blob",
            Self::Other(s) => s.as_str(),
        }
    }
}

/// Eviction policy applied when `evict_to_fit` is called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionPolicy {
    /// Evict the least-recently-used entry first.
    Lru,
    /// Evict the smallest entry first.
    SmallestFirst,
}

/// Internal entry tracked by the budget.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BudgetEntry {
    /// Caller-supplied opaque key (e.g. an asset path).
    key: String,
    /// Bytes the entry occupies.
    bytes: u64,
    /// Monotonic touch counter at last access (for LRU).
    last_touch: u64,
}

/// Per-class memory budget with simple eviction policy.
#[derive(Debug, Clone)]
pub struct AssetBudget {
    class: AssetClass,
    cap_bytes: u64,
    in_use_bytes: u64,
    policy: EvictionPolicy,
    next_touch: u64,
    entries: Vec<BudgetEntry>,
}

impl AssetBudget {
    /// Build a new budget for `class` with the given cap (bytes) and
    /// eviction policy.
    #[must_use]
    pub fn new(class: AssetClass, cap_bytes: u64, policy: EvictionPolicy) -> Self {
        Self {
            class,
            cap_bytes,
            in_use_bytes: 0,
            policy,
            next_touch: 0,
            entries: Vec::new(),
        }
    }

    /// Cap.
    #[must_use]
    pub const fn cap(&self) -> u64 {
        self.cap_bytes
    }

    /// Bytes currently in use.
    #[must_use]
    pub const fn in_use(&self) -> u64 {
        self.in_use_bytes
    }

    /// Bytes free below the cap.
    #[must_use]
    pub const fn free(&self) -> u64 {
        self.cap_bytes.saturating_sub(self.in_use_bytes)
    }

    /// Number of tracked entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Is the budget empty (no tracked entries) ?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Class this budget governs.
    #[must_use]
    pub const fn class(&self) -> &AssetClass {
        &self.class
    }

    /// Eviction policy.
    #[must_use]
    pub const fn policy(&self) -> EvictionPolicy {
        self.policy
    }

    /// Reserve `bytes` for an asset identified by `key`. Returns
    /// `BudgetExceeded` if the reservation would push usage above
    /// the cap.
    pub fn try_reserve(&mut self, key: impl Into<String>, bytes: u64) -> Result<()> {
        if self.in_use_bytes.saturating_add(bytes) > self.cap_bytes {
            return Err(AssetError::budget(
                self.class.name().to_string(),
                self.in_use_bytes.saturating_add(bytes),
                self.cap_bytes,
            ));
        }
        let touch = self.bump_touch();
        self.entries.push(BudgetEntry {
            key: key.into(),
            bytes,
            last_touch: touch,
        });
        self.in_use_bytes = self.in_use_bytes.saturating_add(bytes);
        Ok(())
    }

    /// Release the reservation for `key`. No-op if `key` not tracked.
    pub fn release(&mut self, key: &str) -> u64 {
        if let Some(idx) = self.entries.iter().position(|e| e.key == key) {
            let e = self.entries.remove(idx);
            self.in_use_bytes = self.in_use_bytes.saturating_sub(e.bytes);
            e.bytes
        } else {
            0
        }
    }

    /// Touch (mark recently-used) the entry for `key`. Affects LRU
    /// eviction order. No-op if `key` not tracked.
    pub fn touch(&mut self, key: &str) {
        let next = self.bump_touch();
        if let Some(e) = self.entries.iter_mut().find(|e| e.key == key) {
            e.last_touch = next;
        }
    }

    /// Evict entries until at least `bytes` are free. Returns the list
    /// of keys that were evicted, in eviction order.
    ///
    /// If the budget cannot satisfy the request even after evicting
    /// every entry, the function evicts what it can and returns the
    /// keys ; the caller checks `free()` to detect a still-overflowing
    /// state.
    pub fn evict_to_fit(&mut self, bytes_needed: u64) -> Vec<String> {
        let mut evicted = Vec::new();
        while self.free() < bytes_needed && !self.entries.is_empty() {
            let idx = self.pick_victim_index();
            let e = self.entries.remove(idx);
            self.in_use_bytes = self.in_use_bytes.saturating_sub(e.bytes);
            evicted.push(e.key);
        }
        evicted
    }

    /// Reserve `bytes` for `key`, evicting older entries per the
    /// budget's policy as needed. If the budget cap itself is smaller
    /// than `bytes`, no amount of eviction helps and the call returns
    /// `BudgetExceeded`.
    ///
    /// Returns the list of evicted keys (in eviction order) on success.
    pub fn reserve_with_eviction(
        &mut self,
        key: impl Into<String>,
        bytes: u64,
    ) -> Result<Vec<String>> {
        if bytes > self.cap_bytes {
            return Err(AssetError::budget(
                self.class.name().to_string(),
                bytes,
                self.cap_bytes,
            ));
        }
        let evicted = self.evict_to_fit(bytes);
        // After eviction the reservation must fit ; if it doesn't,
        // surface the budget error.
        self.try_reserve(key, bytes)?;
        Ok(evicted)
    }

    fn bump_touch(&mut self) -> u64 {
        self.next_touch = self.next_touch.wrapping_add(1);
        self.next_touch
    }

    fn pick_victim_index(&self) -> usize {
        match self.policy {
            EvictionPolicy::Lru => {
                let mut best = 0;
                let mut best_t = self.entries[0].last_touch;
                for (i, e) in self.entries.iter().enumerate().skip(1) {
                    if e.last_touch < best_t {
                        best = i;
                        best_t = e.last_touch;
                    }
                }
                best
            }
            EvictionPolicy::SmallestFirst => {
                let mut best = 0;
                let mut best_b = self.entries[0].bytes;
                for (i, e) in self.entries.iter().enumerate().skip(1) {
                    if e.bytes < best_b {
                        best = i;
                        best_b = e.bytes;
                    }
                }
                best
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_name_is_stable() {
        assert_eq!(AssetClass::Texture.name(), "texture");
        assert_eq!(AssetClass::Model.name(), "model");
        assert_eq!(AssetClass::Audio.name(), "audio");
        assert_eq!(AssetClass::Font.name(), "font");
        assert_eq!(AssetClass::Blob.name(), "blob");
        assert_eq!(AssetClass::Other("zoo".into()).name(), "zoo");
    }

    #[test]
    fn new_budget_starts_empty() {
        let b = AssetBudget::new(AssetClass::Texture, 1024, EvictionPolicy::Lru);
        assert_eq!(b.cap(), 1024);
        assert_eq!(b.in_use(), 0);
        assert_eq!(b.free(), 1024);
        assert!(b.is_empty());
        assert_eq!(b.len(), 0);
    }

    #[test]
    fn try_reserve_succeeds_below_cap() {
        let mut b = AssetBudget::new(AssetClass::Audio, 1024, EvictionPolicy::Lru);
        b.try_reserve("a", 256).unwrap();
        assert_eq!(b.in_use(), 256);
        assert_eq!(b.free(), 768);
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn try_reserve_rejects_above_cap() {
        let mut b = AssetBudget::new(AssetClass::Audio, 256, EvictionPolicy::Lru);
        let r = b.try_reserve("big", 512);
        assert!(matches!(r, Err(AssetError::BudgetExceeded { .. })));
        assert_eq!(b.in_use(), 0);
    }

    #[test]
    fn release_returns_bytes_and_updates_usage() {
        let mut b = AssetBudget::new(AssetClass::Texture, 1024, EvictionPolicy::Lru);
        b.try_reserve("a", 256).unwrap();
        b.try_reserve("b", 128).unwrap();
        assert_eq!(b.release("a"), 256);
        assert_eq!(b.in_use(), 128);
    }

    #[test]
    fn release_unknown_key_is_no_op() {
        let mut b = AssetBudget::new(AssetClass::Texture, 1024, EvictionPolicy::Lru);
        assert_eq!(b.release("nope"), 0);
    }

    #[test]
    fn lru_evicts_oldest_first() {
        let mut b = AssetBudget::new(AssetClass::Texture, 1024, EvictionPolicy::Lru);
        b.try_reserve("a", 256).unwrap();
        b.try_reserve("b", 256).unwrap();
        b.try_reserve("c", 256).unwrap();
        b.touch("a"); // a is now most-recent
                      // Evict 256 bytes — should pick `b` (oldest non-touched)
        let evicted = b.evict_to_fit(384);
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0], "b");
    }

    #[test]
    fn smallest_first_evicts_smallest() {
        let mut b = AssetBudget::new(AssetClass::Audio, 10_000, EvictionPolicy::SmallestFirst);
        b.try_reserve("big", 4_000).unwrap();
        b.try_reserve("med", 1_000).unwrap();
        b.try_reserve("tiny", 100).unwrap();
        // Need 200 free, currently 4_900 free → no eviction needed.
        let evicted = b.evict_to_fit(200);
        assert!(evicted.is_empty());
        // Need 5_500 free → evict tiniest first (100), then medium (1000).
        let evicted = b.evict_to_fit(5_500);
        assert_eq!(evicted, vec!["tiny", "med"]);
    }

    #[test]
    fn reserve_with_eviction_evicts_then_reserves() {
        let mut b = AssetBudget::new(AssetClass::Texture, 1024, EvictionPolicy::Lru);
        b.try_reserve("a", 512).unwrap();
        b.try_reserve("b", 256).unwrap();
        // Need 384 ; only 256 free, so 'a' (oldest, but largest) is LRU evicted.
        let evicted = b.reserve_with_eviction("c", 384).unwrap();
        assert_eq!(evicted, vec!["a"]);
        assert!(b.in_use() <= b.cap());
    }

    #[test]
    fn reserve_with_eviction_rejects_over_cap() {
        let mut b = AssetBudget::new(AssetClass::Audio, 256, EvictionPolicy::Lru);
        let r = b.reserve_with_eviction("x", 512);
        assert!(matches!(r, Err(AssetError::BudgetExceeded { .. })));
    }

    #[test]
    fn touch_updates_lru_order() {
        let mut b = AssetBudget::new(AssetClass::Texture, 1024, EvictionPolicy::Lru);
        b.try_reserve("a", 256).unwrap();
        b.try_reserve("b", 256).unwrap();
        b.try_reserve("c", 256).unwrap();
        // Without touch, oldest is 'a' → eviction should pick a.
        let mut b2 = b.clone();
        let ev = b2.evict_to_fit(512);
        assert_eq!(ev[0], "a");
        // With touch on 'a', oldest is 'b' → eviction should pick b.
        b.touch("a");
        let ev = b.evict_to_fit(512);
        assert_eq!(ev[0], "b");
    }

    #[test]
    fn class_accessor_returns_constructed_class() {
        let b = AssetBudget::new(AssetClass::Font, 256, EvictionPolicy::Lru);
        assert_eq!(b.class().name(), "font");
        assert_eq!(b.policy(), EvictionPolicy::Lru);
    }

    #[test]
    fn budget_clone_is_independent() {
        let mut b = AssetBudget::new(AssetClass::Texture, 1024, EvictionPolicy::Lru);
        b.try_reserve("a", 256).unwrap();
        let mut b2 = b.clone();
        b2.release("a");
        assert_eq!(b.in_use(), 256);
        assert_eq!(b2.in_use(), 0);
    }
}
