//! § config tests — JSON round-trip + validation invariants.

use std::env::temp_dir;

use cssl_host_mycelium_desktop::{load_from_path, save_to_path, AppConfig, UiTheme};

#[test]
fn default_config_serializable() {
    let cfg = AppConfig::default();
    let json = serde_json::to_string(&cfg).expect("serialize");
    assert!(json.contains("\"caps\""));
    assert!(json.contains("\"ui_theme\""));
}

#[test]
fn load_from_path_round_trip() {
    let cfg = AppConfig::default();
    let path = temp_dir().join("mycelium-config-roundtrip.json");
    save_to_path(&cfg, &path).expect("save ok");
    let loaded = load_from_path(&path).expect("load ok");
    assert_eq!(loaded.knowledge_top_k, cfg.knowledge_top_k);
    assert_eq!(loaded.context_token_budget, cfg.context_token_budget);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn save_to_path_creates_file() {
    let cfg = AppConfig::default();
    let path = temp_dir().join("mycelium-config-create.json");
    let _ = std::fs::remove_file(&path);
    save_to_path(&cfg, &path).expect("save ok");
    assert!(path.exists(), "file written");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn invalid_json_returns_error() {
    let path = temp_dir().join("mycelium-config-invalid.json");
    std::fs::write(&path, "{ not json }").expect("write seed");
    let result = load_from_path(&path);
    assert!(result.is_err());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn theme_serde_kebab_case() {
    let json = serde_json::to_string(&UiTheme::HighContrast).expect("serialize");
    assert_eq!(json, "\"high-contrast\"");
    let back: UiTheme = serde_json::from_str("\"high-contrast\"").expect("parse");
    assert_eq!(back, UiTheme::HighContrast);
}
