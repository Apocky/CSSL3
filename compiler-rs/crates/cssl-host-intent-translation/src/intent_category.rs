//! § intent_category — 10 IntentCategory + Classification + KeywordCodebook
#![allow(clippy::module_name_repetitions)]

use std::collections::HashSet;

use crate::tokenize::vocab_id_for;

/// 10 main intent-categories + 2 sentinels per intent_translation.csl.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntentCategory {
    DescribeObject = 0,
    DescribeEntity = 1,
    DescribeEnv = 2,
    DescribeBehavior = 3,
    InvokeAction = 4,
    QueryState = 5,
    RevisePrior = 6,
    RevokeCrystallize = 7,
    NarrateAuthor = 8,
    DebugInspect = 9,
    Ambiguous = 254,
    Invalid = 255,
}

impl IntentCategory {
    pub const ALL: [IntentCategory; 10] = [
        Self::DescribeObject,
        Self::DescribeEntity,
        Self::DescribeEnv,
        Self::DescribeBehavior,
        Self::InvokeAction,
        Self::QueryState,
        Self::RevisePrior,
        Self::RevokeCrystallize,
        Self::NarrateAuthor,
        Self::DebugInspect,
    ];

    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::DescribeObject,
            1 => Self::DescribeEntity,
            2 => Self::DescribeEnv,
            3 => Self::DescribeBehavior,
            4 => Self::InvokeAction,
            5 => Self::QueryState,
            6 => Self::RevisePrior,
            7 => Self::RevokeCrystallize,
            8 => Self::NarrateAuthor,
            9 => Self::DebugInspect,
            254 => Self::Ambiguous,
            _ => Self::Invalid,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classification {
    pub category: IntentCategory,
    pub confidence_pct: u8,
}

const KEYWORDS: &[(IntentCategory, &[&str])] = &[
    (IntentCategory::DescribeObject, &["sword","shield","armor","chest","stone","key","ring","lamp","blade","tome","orb","helmet","gauntlet","boot"]),
    (IntentCategory::DescribeEntity, &["sage","warrior","knight","wizard","creature","ghost","dragon","spirit","child","merchant","npc","mage"]),
    (IntentCategory::DescribeEnv, &["forest","cathedral","void","cave","mountain","river","city","tavern","dungeon","field","sky","ocean","ruin","plain"]),
    (IntentCategory::DescribeBehavior, &["responds","reacts","triggers","pulses","glows","whispers","follows","guards","watches","remembers","fades","rotates"]),
    (IntentCategory::InvokeAction, &["open","approach","speak","attack","use","cast","drop","take","drink","close","push","throw"]),
    (IntentCategory::QueryState, &["what","where","when","who","why","how","is","are"]),
    (IntentCategory::RevisePrior, &["more","less","smaller","larger","ornate","simpler","again","instead","change","tweak","adjust"]),
    (IntentCategory::RevokeCrystallize, &["remove","undo","delete","cancel","revoke","forget","discard"]),
    (IntentCategory::NarrateAuthor, &["narrate","compose","write","tale","author","co-author","story"]),
    (IntentCategory::DebugInspect, &["debug","inspect","trace","log","telemetry","dev"]),
];

#[derive(Debug, Clone)]
pub struct KeywordCodebook {
    /// One HashSet of vocab-ids per IntentCategory (parallel to ALL).
    sets: [HashSet<u32>; 10],
}

impl KeywordCodebook {
    pub fn build() -> Self {
        let mut sets: [HashSet<u32>; 10] = Default::default();
        for (cat, words) in KEYWORDS {
            let idx = *cat as usize;
            for w in *words {
                sets[idx].insert(vocab_id_for(w));
            }
        }
        Self { sets }
    }

    pub fn is_keyword(&self, category: IntentCategory, vocab_id: u32) -> bool {
        let idx = category as usize;
        if idx >= 10 {
            return false;
        }
        self.sets[idx].contains(&vocab_id)
    }
}
