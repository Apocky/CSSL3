// § meta_progress ← persistence + spend invariants
// ════════════════════════════════════════════════════════════════════
// § I> echoes-banked-survives-death
// § I> class-XP capped at 1M per class
// § I> idempotent-class-unlock
// ════════════════════════════════════════════════════════════════════

use cssl_host_roguelike_run::meta_progress::{MetaErr, MetaProgress};

#[test]
fn deposit_then_spend_balanced() {
    let mut m = MetaProgress::new();
    m.deposit_echoes(500);
    assert_eq!(m.echoes_total, 500);
    let remaining = m.spend_echoes(200).unwrap();
    assert_eq!(remaining, 300);
    assert_eq!(m.echoes_total, 300);
}

#[test]
fn double_unlock_class_returns_already_unlocked() {
    let mut m = MetaProgress::new();
    m.unlock_class(1).unwrap();
    let err = m.unlock_class(1).unwrap_err();
    assert!(matches!(err, MetaErr::ClassAlreadyUnlocked(1)));
}

#[test]
fn class_xp_capped_at_1m() {
    let mut m = MetaProgress::new();
    m.grant_class_xp(7, 800_000);
    m.grant_class_xp(7, 800_000); // total would be 1.6M
    assert_eq!(m.class_xp_for(7), 1_000_000); // capped
}

#[test]
fn empty_perk_key_rejected() {
    let mut m = MetaProgress::new();
    let err = m.unlock_perk("").unwrap_err();
    assert!(matches!(err, MetaErr::EmptyPerkKey));
}

#[test]
fn perk_lookup_works() {
    let mut m = MetaProgress::new();
    m.unlock_perk("deep-1").unwrap();
    assert!(m.has_perk("deep-1"));
    assert!(!m.has_perk("verdant"));
}
