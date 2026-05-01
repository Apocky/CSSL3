//! § integration tests — end-to-end smoke under Mode-C.

use cssl_host_mycelium_desktop::{handle_command, IpcCommand, IpcResponse};

#[path = "common/mod.rs"]
mod common;

#[test]
fn e2e_substrate_only_chat_smoke() {
    let app = common::make_app();
    let result = app
        .run_turn("orchestrate the system design plan")
        .expect("turn ok");
    assert_eq!(result.turn_id, 1);
    assert!(!result.final_reply.is_empty());

    // Audit-port should have collected at least one event.
    assert!(
        !app.audit.is_empty(),
        "audit-port must have collected events from the turn"
    );
}

#[test]
fn e2e_command_dispatch_round_trip() {
    let mut app = common::make_app();

    // Start a session.
    let resp = handle_command(&mut app, IpcCommand::StartSession);
    assert!(matches!(resp, IpcResponse::SessionStarted { .. }));

    // Send a message.
    let resp = handle_command(
        &mut app,
        IpcCommand::SendMessage {
            content: "narrate the scene".into(),
        },
    );
    assert!(matches!(resp, IpcResponse::MessageReply { .. }));

    // Pull history.
    let resp = handle_command(&mut app, IpcCommand::GetHistory { limit: 10 });
    if let IpcResponse::History { turns } = resp {
        assert_eq!(turns.len(), 1);
    } else {
        panic!("expected History");
    }

    // Doc-count.
    let resp = handle_command(&mut app, IpcCommand::GetSubstrateDocCount);
    assert!(matches!(resp, IpcResponse::SubstrateDocCount { .. }));
}
