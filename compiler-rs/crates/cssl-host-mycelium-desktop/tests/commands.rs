//! § commands tests — IPC dispatch round-trip.

use cssl_host_mycelium_desktop::{handle_command, IpcCommand, IpcResponse};

#[path = "common/mod.rs"]
mod common;

#[test]
fn start_session_returns_session_id() {
    let mut app = common::make_app();
    let resp = handle_command(&mut app, IpcCommand::StartSession);
    match resp {
        IpcResponse::SessionStarted { session_id } => {
            assert!(session_id.starts_with("session-"));
        }
        other => panic!("expected SessionStarted, got {other:?}"),
    }
}

#[test]
fn send_message_routes_through_agent_loop() {
    let mut app = common::make_app();
    let resp = handle_command(
        &mut app,
        IpcCommand::SendMessage {
            content: "describe the world".into(),
        },
    );
    match resp {
        IpcResponse::MessageReply {
            turn_id, content, ..
        } => {
            assert_eq!(turn_id, 1);
            assert!(!content.is_empty(), "Mode-C must produce reply");
        }
        other => panic!("expected MessageReply, got {other:?}"),
    }
}

#[test]
fn query_substrate_returns_hits() {
    let mut app = common::make_app();
    let resp = handle_command(
        &mut app,
        IpcCommand::QuerySubstrate {
            query: "mycelium desktop substrate".into(),
            top_k: 3,
        },
    );
    match resp {
        IpcResponse::SubstrateMatches { hits } => {
            // Stage-0 build may or may not have docs embedded ;
            // we only assert the response shape.
            assert!(hits.len() <= 3, "top_k respected");
        }
        other => panic!("expected SubstrateMatches, got {other:?}"),
    }
}

#[test]
fn get_history_respects_limit() {
    let mut app = common::make_app();
    for i in 0..5 {
        handle_command(
            &mut app,
            IpcCommand::SendMessage {
                content: format!("turn {i}"),
            },
        );
    }
    let resp = handle_command(&mut app, IpcCommand::GetHistory { limit: 3 });
    match resp {
        IpcResponse::History { turns } => {
            assert_eq!(turns.len(), 3, "limit respected");
            // Last 3 should be turns 3, 4, 5 (turn-ids 3..=5).
            assert_eq!(turns[0].turn_id, 3);
            assert_eq!(turns[2].turn_id, 5);
        }
        other => panic!("expected History, got {other:?}"),
    }
}

#[test]
fn grant_cap_round_trip() {
    let mut app = common::make_default_user_app();
    let resp = handle_command(
        &mut app,
        IpcCommand::GrantCap {
            tool: "file_write".into(),
            mode: "auto".into(),
        },
    );
    assert!(matches!(resp, IpcResponse::CapGranted { .. }));
    let resp = handle_command(
        &mut app,
        IpcCommand::RevokeCap {
            tool: "file_write".into(),
        },
    );
    assert!(matches!(resp, IpcResponse::CapRevoked { .. }));
}

#[test]
fn revoke_all_sovereign_caps_round_trip() {
    let mut app = common::make_app();
    let resp = handle_command(&mut app, IpcCommand::RevokeAllSovereignCaps);
    assert!(matches!(resp, IpcResponse::AllSovereignRevoked));
}

#[test]
fn update_config_round_trip() {
    let mut app = common::make_app();
    let mut new_cfg = app.config.clone();
    new_cfg.knowledge_top_k = 7;
    let resp = handle_command(
        &mut app,
        IpcCommand::UpdateConfig { config: new_cfg },
    );
    assert!(matches!(resp, IpcResponse::ConfigUpdated));
    let resp = handle_command(&mut app, IpcCommand::GetConfig);
    if let IpcResponse::Config { config } = resp {
        assert_eq!(config.knowledge_top_k, 7);
    } else {
        panic!("expected Config");
    }
}

#[test]
fn unknown_field_invalid_json_error() {
    // Construct a malformed-tool grant — exercises the error path.
    let mut app = common::make_app();
    let resp = handle_command(
        &mut app,
        IpcCommand::GrantCap {
            tool: "no_such_tool".into(),
            mode: "auto".into(),
        },
    );
    match resp {
        IpcResponse::Error { code, .. } => {
            assert_eq!(code, "cap_policy", "unknown tool routes to cap_policy");
        }
        other => panic!("expected Error, got {other:?}"),
    }
}
