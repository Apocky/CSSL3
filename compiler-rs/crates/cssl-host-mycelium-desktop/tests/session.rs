//! § session tests — buffer + replay + max-turn eviction.

use cssl_host_mycelium_desktop::{Session, StoredTurn};

fn fake_turn(id: u64) -> StoredTurn {
    StoredTurn {
        turn_id: id,
        user_input: format!("input-{id}"),
        reply: format!("reply-{id}"),
        tool_calls: Vec::new(),
        elapsed_ms: 10,
        timestamp_unix: 100 + id,
    }
}

#[test]
fn session_records_turn() {
    let mut s = Session::new(10);
    assert!(s.is_empty());
    s.record(fake_turn(1));
    assert_eq!(s.len(), 1);
    assert_eq!(s.turns.front().unwrap().turn_id, 1);
}

#[test]
fn session_replay_last_n() {
    let mut s = Session::new(10);
    for i in 1..=5 {
        s.record(fake_turn(i));
    }
    let last3 = s.replay_last_n(3);
    assert_eq!(last3.len(), 3);
    assert_eq!(last3[0].turn_id, 3);
    assert_eq!(last3[2].turn_id, 5);
}

#[test]
fn session_snapshot_immutable() {
    let mut s = Session::new(10);
    s.record(fake_turn(1));
    let snap1 = s.snapshot();
    s.record(fake_turn(2));
    let snap2 = s.snapshot();
    // First snapshot retains its own len ; subsequent record didn't mutate it.
    assert_eq!(snap1.turn_count, 1);
    assert_eq!(snap2.turn_count, 2);
}

#[test]
fn session_max_turns_evicts_oldest() {
    let mut s = Session::new(3);
    for i in 1..=5 {
        s.record(fake_turn(i));
    }
    assert_eq!(s.len(), 3);
    // Oldest two evicted ; should retain 3, 4, 5.
    assert_eq!(s.turns.front().unwrap().turn_id, 3);
    assert_eq!(s.turns.back().unwrap().turn_id, 5);
}
