// § cohort_query : HistoricalCohort + filters.
// § Caller (sibling W8-C2) supplies a validated merkle-root ; we trust it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// § CohortEvent — minimal record of a past in-engine event.
/// `scene_id` + `player_id` + `ts_secs` are the canonical query keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CohortEvent {
    pub scene_id: [u8; 16],
    pub player_id: [u8; 16],
    pub ts_secs: u64,
    /// Opaque event-payload digest (¬ raw payload).
    pub payload_digest: [u8; 32],
}

/// § HistoricalCohort — bundle of events with a caller-supplied merkle-root.
/// Events are sorted ascending by `ts_secs` (stable on equal-ts via index).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoricalCohort {
    pub cohort_id: [u8; 32],
    pub merkle_root: [u8; 32],
    events: Vec<CohortEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CohortFilter {
    pub scene_id: Option<[u8; 16]>,
    pub player_id: Option<[u8; 16]>,
    pub ts_lo_secs: Option<u64>,
    pub ts_hi_secs: Option<u64>,
}

impl CohortFilter {
    pub fn any() -> Self {
        Self { scene_id: None, player_id: None, ts_lo_secs: None, ts_hi_secs: None }
    }
    fn matches(&self, e: &CohortEvent) -> bool {
        if let Some(s) = self.scene_id { if e.scene_id != s { return false; } }
        if let Some(p) = self.player_id { if e.player_id != p { return false; } }
        if let Some(lo) = self.ts_lo_secs { if e.ts_secs < lo { return false; } }
        if let Some(hi) = self.ts_hi_secs { if e.ts_secs > hi { return false; } }
        true
    }
}

impl HistoricalCohort {
    /// § Construct cohort from caller-validated parts. Events are sorted
    /// in-place ascending by ts_secs ; ties broken by original-index (stable).
    pub fn new(cohort_id: [u8; 32], merkle_root: [u8; 32], mut events: Vec<CohortEvent>) -> Self {
        events.sort_by_key(|e| e.ts_secs);
        Self { cohort_id, merkle_root, events }
    }

    pub fn events(&self) -> &[CohortEvent] { &self.events }

    pub fn len(&self) -> usize { self.events.len() }
    pub fn is_empty(&self) -> bool { self.events.is_empty() }

    /// § Query : returns matching events sorted by ts_secs.
    pub fn query(&self, f: &CohortFilter) -> Vec<&CohortEvent> {
        self.events.iter().filter(|e| f.matches(e)).collect()
    }

    /// § Group events by player_id (BTreeMap-deterministic).
    pub fn group_by_player(&self) -> BTreeMap<[u8; 16], Vec<&CohortEvent>> {
        let mut out: BTreeMap<[u8; 16], Vec<&CohortEvent>> = BTreeMap::new();
        for e in &self.events {
            out.entry(e.player_id).or_default().push(e);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evt(ts: u64, scene: u8, player: u8) -> CohortEvent {
        CohortEvent {
            scene_id: [scene; 16],
            player_id: [player; 16],
            ts_secs: ts,
            payload_digest: [0xAB; 32],
        }
    }

    #[test]
    fn cohort_construct_sorts() {
        let c = HistoricalCohort::new([1u8; 32], [2u8; 32], vec![evt(30, 1, 1), evt(10, 1, 1), evt(20, 1, 1)]);
        let ts: Vec<u64> = c.events().iter().map(|e| e.ts_secs).collect();
        assert_eq!(ts, vec![10, 20, 30]);
    }

    #[test]
    fn cohort_empty_ok() {
        let c = HistoricalCohort::new([1u8; 32], [2u8; 32], vec![]);
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn query_by_scene() {
        let c = HistoricalCohort::new([1u8; 32], [2u8; 32], vec![
            evt(10, 1, 1), evt(20, 2, 1), evt(30, 1, 2),
        ]);
        let mut f = CohortFilter::any();
        f.scene_id = Some([1u8; 16]);
        let r = c.query(&f);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn query_by_player() {
        let c = HistoricalCohort::new([1u8; 32], [2u8; 32], vec![
            evt(10, 1, 1), evt(20, 2, 1), evt(30, 1, 2),
        ]);
        let mut f = CohortFilter::any();
        f.player_id = Some([2u8; 16]);
        let r = c.query(&f);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn query_by_ts_range() {
        let c = HistoricalCohort::new([1u8; 32], [2u8; 32], vec![
            evt(10, 1, 1), evt(20, 1, 1), evt(30, 1, 1), evt(40, 1, 1),
        ]);
        let mut f = CohortFilter::any();
        f.ts_lo_secs = Some(15);
        f.ts_hi_secs = Some(35);
        let r = c.query(&f);
        let ts: Vec<u64> = r.iter().map(|e| e.ts_secs).collect();
        assert_eq!(ts, vec![20, 30]);
    }

    #[test]
    fn group_by_player_deterministic() {
        let c = HistoricalCohort::new([1u8; 32], [2u8; 32], vec![
            evt(10, 1, 1), evt(20, 1, 2), evt(30, 1, 1),
        ]);
        let g = c.group_by_player();
        assert_eq!(g.len(), 2);
        assert_eq!(g[&[1u8; 16]].len(), 2);
        assert_eq!(g[&[2u8; 16]].len(), 1);
    }

    #[test]
    fn cohort_serde_round_trip() {
        let c = HistoricalCohort::new([1u8; 32], [2u8; 32], vec![evt(10, 1, 1), evt(20, 1, 2)]);
        let j = serde_json::to_string(&c).unwrap();
        let back: HistoricalCohort = serde_json::from_str(&j).unwrap();
        assert_eq!(c, back);
    }
}
