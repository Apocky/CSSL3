//! § RemixKind — 6 variants describing the relationship to the parent.
//!
//!   Fork         · creator-fork branching new direction
//!   Extension    · adds-to parent (e.g., new chapters · new NPCs)
//!   Translation  · language/locale port preserving semantics
//!   Adaptation   · cross-medium port (e.g., text-storylet → audio-pack)
//!   Improvement  · bug-fix / quality-enhance / accessibility-add
//!   Bundle       · composite of multiple parents (collection · pack)
//!
//! Stable serialization : kebab-case strings (json) · u8-tag (binary).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemixKind {
    Fork,
    Extension,
    Translation,
    Adaptation,
    Improvement,
    Bundle,
}

impl RemixKind {
    /// Stable u8 tag for canonical-bytes (signature input).
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            RemixKind::Fork => 0x01,
            RemixKind::Extension => 0x02,
            RemixKind::Translation => 0x03,
            RemixKind::Adaptation => 0x04,
            RemixKind::Improvement => 0x05,
            RemixKind::Bundle => 0x06,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            RemixKind::Fork => "fork",
            RemixKind::Extension => "extension",
            RemixKind::Translation => "translation",
            RemixKind::Adaptation => "adaptation",
            RemixKind::Improvement => "improvement",
            RemixKind::Bundle => "bundle",
        }
    }

    /// Inverse of `tag()`. Returns None for unknown tag-byte.
    #[must_use]
    pub const fn from_tag(t: u8) -> Option<Self> {
        match t {
            0x01 => Some(RemixKind::Fork),
            0x02 => Some(RemixKind::Extension),
            0x03 => Some(RemixKind::Translation),
            0x04 => Some(RemixKind::Adaptation),
            0x05 => Some(RemixKind::Improvement),
            0x06 => Some(RemixKind::Bundle),
            _ => None,
        }
    }
}

/// All 6 variants in canonical order. Useful for exhaustive-iteration tests
/// + UI dropdown rendering.
pub const REMIX_KINDS: [RemixKind; 6] = [
    RemixKind::Fork,
    RemixKind::Extension,
    RemixKind::Translation,
    RemixKind::Adaptation,
    RemixKind::Improvement,
    RemixKind::Bundle,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_roundtrip_all_six() {
        for k in REMIX_KINDS.iter().copied() {
            let t = k.tag();
            assert_eq!(RemixKind::from_tag(t), Some(k));
        }
        assert!(RemixKind::from_tag(0x00).is_none());
        assert!(RemixKind::from_tag(0xFF).is_none());
    }

    #[test]
    fn as_str_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for k in REMIX_KINDS.iter().copied() {
            assert!(seen.insert(k.as_str()), "{} duplicated", k.as_str());
        }
        assert_eq!(seen.len(), 6);
    }

    #[test]
    fn json_kebab_case() {
        let j = serde_json::to_string(&RemixKind::Translation).unwrap();
        assert_eq!(j, "\"translation\"");
    }
}
