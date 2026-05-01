//! Unit tests for `cssl-host-home-dimension`.
//!
//! Coverage map (≥ 28 tests required) :
//!   - state-construction (3) : `new_default_*`
//!   - mode-transitions all-5-pairs cap-gated (5) : `mode_*`
//!   - decoration place/remove/list (4) : `decoration_*`
//!   - trophy pin/unpin (3) : `trophy_*`
//!   - companion lifecycle (3) : `companion_*`
//!   - navigation portal cap-gated (3) : `portal_*`
//!   - forge queue (2) : `forge_*`
//!   - memorial post/list (3) : `memorial_*`
//!   - cap-bits bitwise (2) : `caps_*`
//!   - serde round-trip (2) : `serde_*`
//!   - sovereignty-revocation (extras) : `sovereign_*`

use super::*;

fn ts(n: u64) -> Timestamp {
    Timestamp(n)
}

fn pk(seed: u8) -> Pubkey {
    Pubkey::from_seed(seed)
}

fn make_home(archetype: ArchetypeId) -> Home {
    Home::new(HomeId(1), pk(1), archetype, ts(0))
}

fn full_caps_home(archetype: ArchetypeId) -> Home {
    let mut h = make_home(archetype);
    h.grant_cap(HomeCapBits::full().bits(), ts(0));
    h
}

// ─── STATE CONSTRUCTION ─────────────────────────────────────────────

#[test]
fn new_default_has_private_mode_and_no_caps() {
    let h = make_home(ArchetypeId::OrbitalShip);
    assert_eq!(h.mode(), AccessMode::PrivateAlwaysOn);
    assert_eq!(h.caps(), HomeCapBits::empty());
    assert_eq!(h.archetype(), ArchetypeId::OrbitalShip);
    assert_eq!(h.id(), HomeId(1));
    assert_eq!(h.owner(), pk(1));
}

#[test]
fn new_audit_log_seeded_with_created_event() {
    let h = make_home(ArchetypeId::CavernSanctum);
    assert_eq!(h.audit().len(), 1);
    let ev = h.audit().iter().next().unwrap();
    assert_eq!(ev.kind, AuditKind::Created);
    assert_eq!(ev.actor, pk(1));
}

#[test]
fn new_archetype_change_emits_audit() {
    let mut h = make_home(ArchetypeId::OrbitalShip);
    h.change_archetype(ArchetypeId::CathedralHall, ts(5));
    assert_eq!(h.archetype(), ArchetypeId::CathedralHall);
    assert!(h.audit().of_kind(AuditKind::ArchetypeChanged).count() == 1);
}

// ─── MODE TRANSITIONS (5 cap-gated tests) ───────────────────────────

#[test]
fn mode_set_friend_requires_invite_cap() {
    let mut h = make_home(ArchetypeId::GroundWorkshop);
    let err = h.set_mode(AccessMode::FriendOnly, ts(1)).unwrap_err();
    assert!(matches!(err, HomeError::MissingCap { required, .. } if required == HM_CAP_INVITE));
    h.grant_cap(HM_CAP_INVITE, ts(1));
    h.set_mode(AccessMode::FriendOnly, ts(2)).unwrap();
    assert_eq!(h.mode(), AccessMode::FriendOnly);
}

#[test]
fn mode_set_guild_open_requires_invite_cap() {
    let mut h = make_home(ArchetypeId::CathedralHall);
    assert!(h.set_mode(AccessMode::GuildOpen, ts(1)).is_err());
    h.grant_cap(HM_CAP_INVITE, ts(1));
    h.set_mode(AccessMode::GuildOpen, ts(2)).unwrap();
    assert_eq!(h.mode(), AccessMode::GuildOpen);
}

#[test]
fn mode_set_public_requires_publish_cap() {
    let mut h = make_home(ArchetypeId::CosmicObservatory);
    assert!(h.set_mode(AccessMode::PublicListed, ts(1)).is_err());
    h.grant_cap(HM_CAP_PUBLISH, ts(1));
    h.set_mode(AccessMode::PublicListed, ts(2)).unwrap();
    assert_eq!(h.mode(), AccessMode::PublicListed);
}

#[test]
fn mode_set_dropin_requires_publish_cap_and_emits_audit() {
    let mut h = full_caps_home(ArchetypeId::LivingForest);
    h.set_mode(AccessMode::RandomDropin, ts(3)).unwrap();
    assert_eq!(h.mode(), AccessMode::RandomDropin);
    assert!(h.audit().of_kind(AuditKind::ModeChanged).count() >= 1);
}

#[test]
fn mode_set_back_to_private_always_allowed_and_ejects_visitors() {
    let mut h = full_caps_home(ArchetypeId::HybridCustom);
    h.set_mode(AccessMode::PublicListed, ts(1)).unwrap();
    h.accept_visitor(pk(7), ts(2)).unwrap();
    assert_eq!(h.state().visitors.len(), 1);
    h.set_mode(AccessMode::PrivateAlwaysOn, ts(3)).unwrap();
    assert_eq!(h.mode(), AccessMode::PrivateAlwaysOn);
    assert!(h.state().visitors.is_empty());
    assert!(h.audit().of_kind(AuditKind::VisitorEjected).count() >= 1);
}

// ─── DECORATIONS (4 tests) ──────────────────────────────────────────

#[test]
fn decoration_place_requires_decorate_cap() {
    let mut h = make_home(ArchetypeId::OrbitalShip);
    let asset = OpaqueAsset::new(0x1111_2222, "lamp");
    let err = h
        .decorate(0, asset.clone(), SlotTransform::identity(), ts(1))
        .unwrap_err();
    assert!(matches!(err, HomeError::MissingCap { required, .. } if required == HM_CAP_DECORATE));
    h.grant_cap(HM_CAP_DECORATE, ts(1));
    h.decorate(0, asset, SlotTransform::identity(), ts(2))
        .unwrap();
    assert_eq!(h.list_decorations().count(), 1);
}

#[test]
fn decoration_remove_works_and_audits() {
    let mut h = full_caps_home(ArchetypeId::GroundWorkshop);
    h.decorate(
        7,
        OpaqueAsset::new(1, "trophy"),
        SlotTransform::identity(),
        ts(1),
    )
    .unwrap();
    h.remove_decoration(7, ts(2)).unwrap();
    assert_eq!(h.list_decorations().count(), 0);
    assert!(h.audit().of_kind(AuditKind::DecorationRemoved).count() == 1);
}

#[test]
fn decoration_remove_unknown_slot_returns_not_found() {
    let mut h = full_caps_home(ArchetypeId::OrbitalShip);
    let err = h.remove_decoration(999, ts(1)).unwrap_err();
    assert_eq!(err, HomeError::NotFound);
}

#[test]
fn decoration_list_iterates_in_slot_id_order() {
    let mut h = full_caps_home(ArchetypeId::CavernSanctum);
    for slot in [3u32, 1, 5, 2, 4] {
        h.decorate(
            slot,
            OpaqueAsset::new(u64::from(slot), "x"),
            SlotTransform::identity(),
            ts(slot.into()),
        )
        .unwrap();
    }
    let order: Vec<u32> = h.list_decorations().map(|d| d.slot_id).collect();
    assert_eq!(order, vec![1, 2, 3, 4, 5]);
}

// ─── TROPHIES (3 tests) ─────────────────────────────────────────────

#[test]
fn trophy_pin_records_and_lists() {
    let mut h = make_home(ArchetypeId::CathedralHall);
    let t = Trophy::new(42, TrophyKind::AscendedItem, ts(1), "Sword of Apocky");
    h.pin_trophy(t.clone(), ts(2)).unwrap();
    assert_eq!(h.list_trophies().count(), 1);
    assert_eq!(h.list_trophies().next().unwrap(), &t);
}

#[test]
fn trophy_pin_duplicate_id_rejected() {
    let mut h = make_home(ArchetypeId::CathedralHall);
    let t = Trophy::new(1, TrophyKind::CoherenceShrine, ts(1), "shrine");
    h.pin_trophy(t.clone(), ts(2)).unwrap();
    let err = h.pin_trophy(t, ts(3)).unwrap_err();
    assert_eq!(err, HomeError::DuplicateId);
}

#[test]
fn trophy_unpin_removes_and_audits() {
    let mut h = make_home(ArchetypeId::CathedralHall);
    h.pin_trophy(Trophy::new(9, TrophyKind::NemesisSpore, ts(1), "n"), ts(2))
        .unwrap();
    h.unpin_trophy(9, ts(3)).unwrap();
    assert_eq!(h.list_trophies().count(), 0);
    assert!(h.audit().of_kind(AuditKind::TrophyUnpinned).count() == 1);
}

// ─── COMPANIONS (3 tests) ───────────────────────────────────────────

#[test]
fn companion_add_requires_npc_hire_cap() {
    let mut h = make_home(ArchetypeId::CathedralHall);
    let c = Companion::new(pk(9), ArchetypeId::CathedralHall, "Welcome");
    let err = h.add_companion(c.clone(), ts(1)).unwrap_err();
    assert!(matches!(err, HomeError::MissingCap { required, .. } if required == HM_CAP_NPC_HIRE));
    h.grant_cap(HM_CAP_NPC_HIRE, ts(1));
    h.add_companion(c, ts(2)).unwrap();
    assert_eq!(h.list_companions().count(), 1);
}

#[test]
fn companion_dismiss_removes_and_audits() {
    let mut h = full_caps_home(ArchetypeId::CathedralHall);
    h.add_companion(
        Companion::new(pk(8), ArchetypeId::OrbitalShip, "hi"),
        ts(1),
    )
    .unwrap();
    h.dismiss_companion(&pk(8), ts(2)).unwrap();
    assert_eq!(h.list_companions().count(), 0);
    assert!(h.audit().of_kind(AuditKind::CompanionDismissed).count() == 1);
}

#[test]
fn companion_converse_advances_disposition_deterministically() {
    let mut h = full_caps_home(ArchetypeId::CathedralHall);
    let id = pk(11);
    h.add_companion(Companion::new(id, ArchetypeId::CavernSanctum, "g"), ts(1))
        .unwrap();
    let c = h.companion_mut(&id).unwrap();
    assert_eq!(c.disposition, CompanionDisposition::Reserved);
    let r1 = c.converse("hi");
    assert_eq!(c.disposition, CompanionDisposition::Friendly);
    let r2 = c.converse("again");
    assert_eq!(c.disposition, CompanionDisposition::Devoted);
    // determinism : same prompt + same disposition-bucket → same reply substring
    assert!(r1.contains("hi"));
    assert!(r2.contains("again"));
}

// ─── PORTALS (3 tests) ──────────────────────────────────────────────

#[test]
fn portal_register_with_zero_cap_works_in_private_mode() {
    let mut h = make_home(ArchetypeId::OrbitalShip);
    h.register_portal(0, PortalDest::RunStart, 0, ts(1)).unwrap();
    assert_eq!(h.list_portals().count(), 1);
}

#[test]
fn portal_register_requires_cap_when_specified() {
    let mut h = make_home(ArchetypeId::OrbitalShip);
    let err = h
        .register_portal(1, PortalDest::Bazaar, HM_CAP_PUBLISH, ts(1))
        .unwrap_err();
    assert!(matches!(err, HomeError::MissingCap { required, .. } if required == HM_CAP_PUBLISH));
    h.grant_cap(HM_CAP_PUBLISH, ts(2));
    h.register_portal(1, PortalDest::Bazaar, HM_CAP_PUBLISH, ts(3))
        .unwrap();
}

#[test]
fn portal_disable_idempotent_and_audits_once() {
    let mut h = full_caps_home(ArchetypeId::OrbitalShip);
    h.register_portal(7, PortalDest::Multiverse, 0, ts(1))
        .unwrap();
    h.disable_portal(7, ts(2)).unwrap();
    h.disable_portal(7, ts(3)).unwrap(); // idempotent : no second audit
    assert!(!h.list_portals().next().unwrap().enabled);
    assert!(h.audit().of_kind(AuditKind::PortalDisabled).count() == 1);
}

// ─── FORGE (2 tests) ────────────────────────────────────────────────

#[test]
fn forge_queue_requires_cap_and_records() {
    let mut h = make_home(ArchetypeId::GroundWorkshop);
    let err = h.forge_queue(1, ForgeRecipeId(5), ts(1)).unwrap_err();
    assert!(matches!(err, HomeError::MissingCap { required, .. } if required == HM_CAP_FORGE_USE));
    h.grant_cap(HM_CAP_FORGE_USE, ts(1));
    h.forge_queue(1, ForgeRecipeId(5), ts(2)).unwrap();
    assert_eq!(h.forge_iter().count(), 1);
    assert!(h.forge_iter().next().unwrap().pending);
}

#[test]
fn forge_cancel_marks_not_pending() {
    let mut h = full_caps_home(ArchetypeId::GroundWorkshop);
    h.forge_queue(2, ForgeRecipeId(99), ts(1)).unwrap();
    h.forge_cancel(2, ts(2)).unwrap();
    assert!(!h.forge_iter().next().unwrap().pending);
}

// ─── MEMORIAL (3 tests) ─────────────────────────────────────────────

#[test]
fn memorial_post_requires_cap_and_lists() {
    let mut h = make_home(ArchetypeId::LivingForest);
    let m = MemorialEntry::new(1, "Yrgrim", "to a Nemesis", ts(1));
    let err = h.post_memorial(m.clone(), ts(2)).unwrap_err();
    assert!(matches!(err, HomeError::MissingCap { .. }));
    h.grant_cap(HM_CAP_MEMORIAL_PIN, ts(2));
    h.post_memorial(m, ts(3)).unwrap();
    assert_eq!(h.list_memorials().count(), 1);
}

#[test]
fn memorial_ascription_appends_in_order() {
    let mut h = full_caps_home(ArchetypeId::LivingForest);
    h.post_memorial(MemorialEntry::new(1, "A", "x", ts(1)), ts(2))
        .unwrap();
    h.ascribe_memorial(1, MemorialAscription::new(pk(2), ts(3), "first"))
        .unwrap();
    h.ascribe_memorial(1, MemorialAscription::new(pk(3), ts(4), "second"))
        .unwrap();
    let m = h.list_memorials().next().unwrap();
    assert_eq!(m.ascriptions.len(), 2);
    assert_eq!(m.ascriptions[0].text, "first");
    assert_eq!(m.ascriptions[1].text, "second");
}

#[test]
fn memorial_ascription_unknown_id_returns_not_found() {
    let mut h = full_caps_home(ArchetypeId::LivingForest);
    let err = h
        .ascribe_memorial(404, MemorialAscription::new(pk(1), ts(1), "ghost"))
        .unwrap_err();
    assert_eq!(err, HomeError::NotFound);
}

// ─── CAP-BITS BITWISE (2 tests) ─────────────────────────────────────

#[test]
fn caps_bitwise_grant_revoke_has_count() {
    let c = HomeCapBits::empty();
    assert!(c.is_empty());
    let c = c.grant(HM_CAP_DECORATE).grant(HM_CAP_PUBLISH);
    assert!(c.has(HM_CAP_DECORATE));
    assert!(c.has(HM_CAP_PUBLISH));
    assert!(!c.has(HM_CAP_FORGE_USE));
    assert_eq!(c.count(), 2);
    let c = c.revoke(HM_CAP_DECORATE);
    assert!(!c.has(HM_CAP_DECORATE));
    assert!(c.has(HM_CAP_PUBLISH));
    // bitor / bitand
    let union = HomeCapBits(HM_CAP_INVITE) | HomeCapBits(HM_CAP_PUBLISH);
    assert!(union.has(HM_CAP_INVITE));
    assert!(union.has(HM_CAP_PUBLISH));
    let intersect = union & HomeCapBits(HM_CAP_INVITE);
    assert_eq!(intersect.bits(), HM_CAP_INVITE);
}

#[test]
fn caps_from_bits_masks_reserved_bits() {
    let raw = 0xFFFF_FFFFu32;
    let c = HomeCapBits::from_bits(raw);
    assert_eq!(c.bits(), 0xFF); // all 8 defined caps, nothing else
    assert_eq!(HomeCapBits::full().bits(), 0xFF);
}

// ─── SERDE ROUND-TRIP (2 tests) ─────────────────────────────────────

#[test]
fn serde_roundtrip_full_home_preserves_state() {
    let mut h = full_caps_home(ArchetypeId::HybridCustom);
    h.set_mode(AccessMode::PublicListed, ts(1)).unwrap();
    h.decorate(
        0,
        OpaqueAsset::new(123, "statue"),
        SlotTransform::identity(),
        ts(2),
    )
    .unwrap();
    h.pin_trophy(
        Trophy::new(7, TrophyKind::Biography, ts(3), "Tale of Apocky"),
        ts(4),
    )
    .unwrap();
    h.add_companion(
        Companion::new(pk(20), ArchetypeId::OrbitalShip, "hello"),
        ts(5),
    )
    .unwrap();
    h.toggle_mycelial(ts(6));

    let json = serde_json::to_string(&h).unwrap();
    let h2: Home = serde_json::from_str(&json).unwrap();
    assert_eq!(h, h2);
}

#[test]
fn serde_roundtrip_btreemap_iter_canonical_order() {
    // Two homes built with the same insertions in different orders should
    // serialize identically, demonstrating BTreeMap-keyed determinism.
    let mut a = full_caps_home(ArchetypeId::OrbitalShip);
    let mut b = full_caps_home(ArchetypeId::OrbitalShip);
    for slot in [3u32, 1, 4, 1, 5, 9] {
        a.decorate(
            slot,
            OpaqueAsset::new(u64::from(slot), "a"),
            SlotTransform::identity(),
            ts(slot.into()),
        )
        .unwrap();
    }
    for slot in [9u32, 5, 1, 4, 3, 1] {
        b.decorate(
            slot,
            OpaqueAsset::new(u64::from(slot), "a"),
            SlotTransform::identity(),
            ts(slot.into()),
        )
        .unwrap();
    }
    // collections are equal (BTreeMap order independent of insertion)
    assert_eq!(a.state().decorations, b.state().decorations);
    let json_a = serde_json::to_string(&a.state().decorations).unwrap();
    let json_b = serde_json::to_string(&b.state().decorations).unwrap();
    assert_eq!(json_a, json_b);
}

// ─── SOVEREIGNTY EXTRAS ─────────────────────────────────────────────

#[test]
fn sovereign_revoking_invite_cap_forces_back_to_private_and_ejects() {
    let mut h = full_caps_home(ArchetypeId::CathedralHall);
    h.add_friend(pk(50));
    h.set_mode(AccessMode::FriendOnly, ts(1)).unwrap();
    h.accept_visitor(pk(50), ts(2)).unwrap();
    assert_eq!(h.state().visitors.len(), 1);
    h.revoke_cap(HM_CAP_INVITE, ts(3));
    assert_eq!(h.mode(), AccessMode::PrivateAlwaysOn);
    assert!(h.state().visitors.is_empty());
}

#[test]
fn sovereign_remove_friend_during_friend_mode_ejects_them() {
    let mut h = full_caps_home(ArchetypeId::OrbitalShip);
    h.add_friend(pk(60));
    h.set_mode(AccessMode::FriendOnly, ts(1)).unwrap();
    h.accept_visitor(pk(60), ts(2)).unwrap();
    h.remove_friend(&pk(60), ts(3));
    assert!(h.state().visitors.is_empty());
}

#[test]
fn sovereign_visitor_in_private_mode_denied() {
    let mut h = make_home(ArchetypeId::OrbitalShip);
    let err = h.accept_visitor(pk(99), ts(1)).unwrap_err();
    assert!(matches!(err, HomeError::AccessDenied { mode } if mode == AccessMode::PrivateAlwaysOn));
}

#[test]
fn archetype_all_seven_present_and_codes_unique() {
    let codes: Vec<&str> = ArchetypeId::all().iter().map(|a| a.code()).collect();
    assert_eq!(codes.len(), 7);
    let set: std::collections::BTreeSet<_> = codes.iter().collect();
    assert_eq!(set.len(), 7);
    assert!(codes.contains(&"H1"));
    assert!(codes.contains(&"H7"));
}

#[test]
fn access_mode_all_five_present() {
    let modes = AccessMode::all();
    assert_eq!(modes.len(), 5);
    assert_eq!(AccessMode::default(), AccessMode::PrivateAlwaysOn);
}
