//! § secrets — local-only API-key persistence at `~/.loa-secrets/anthropic.env`.
//!
//! § PRIME-DIRECTIVE
//!   - Keys NEVER leave this machine. The save-path resolves to the user's
//!     profile directory (Windows : `%USERPROFILE%\.loa-secrets\anthropic.env` ;
//!     Unix : `$HOME/.loa-secrets/anthropic.env`).
//!   - The wire-format is `KEY=VALUE` per-line ; consistent with the existing
//!     `~/.loa-secrets/cloudflare.env` pattern.
//!   - The IPC layer NEVER returns the raw key to JS — only the masked
//!     "sk-...XXXX" form via `mask_key`.
//!   - `tracing::info!` lines redact via `redact_for_log`.
//!   - On Windows, `icacls` is invoked best-effort to restrict the file ACL
//!     to the current user. Failures are non-fatal (the file already lives
//!     under user-profile which inherits user-only perms by default).
//!
//! § STAGE-0 SCOPE
//!   - Single-key surface : `ANTHROPIC_API_KEY`.
//!   - Append-or-replace semantics : if the file exists, the line is
//!     overwritten in-place (preserving any unrelated KEY=VALUE lines).
//!   - No keychain integration ; that lands in a follow-up wave.

use std::path::PathBuf;

/// Variable-name used in the env-file for the Anthropic key.
pub const ANTHROPIC_KEY_VAR: &str = "ANTHROPIC_API_KEY";

/// Subdirectory under `$HOME` / `%USERPROFILE%` where the env-file lives.
pub const SECRETS_SUBDIR: &str = ".loa-secrets";

/// File-name of the env-file containing the Anthropic key.
pub const ANTHROPIC_ENV_FILE: &str = "anthropic.env";

/// Errors raised by the secrets-port. Mirrors the `ConfigError` shape so
/// callers can wrap into `AppError::Config` if needed.
#[derive(Debug, thiserror::Error)]
pub enum SecretsError {
    /// Could not resolve the user-profile directory (no `HOME` /
    /// `USERPROFILE` env-var set). Catastrophic — never expected on a
    /// real desktop.
    #[error("no user-profile dir resolved (HOME / USERPROFILE unset)")]
    NoProfileDir,
    /// I/O-layer failure (path-not-found · permission · disk-full · …).
    #[error("io: {0}")]
    Io(String),
    /// The key string failed structural validation (empty / too long).
    #[error("invalid key: {0}")]
    InvalidKey(&'static str),
}

impl From<std::io::Error> for SecretsError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

/// Resolve `~/.loa-secrets/anthropic.env`. Returns the absolute path even
/// when the directory does not yet exist — callers that mutate the file
/// must `create_dir_all(parent)` first.
pub fn anthropic_env_path() -> Result<PathBuf, SecretsError> {
    let home = profile_dir()?;
    Ok(home.join(SECRETS_SUBDIR).join(ANTHROPIC_ENV_FILE))
}

/// Resolve the user-profile directory. Windows prefers `USERPROFILE` ;
/// Unix prefers `HOME` ; we accept either on either platform so test
/// harnesses can override via env-var.
fn profile_dir() -> Result<PathBuf, SecretsError> {
    if let Some(p) = std::env::var_os("USERPROFILE") {
        return Ok(PathBuf::from(p));
    }
    if let Some(p) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(p));
    }
    Err(SecretsError::NoProfileDir)
}

/// Validate a candidate API-key. Stage-0 rejects empty + > 1024-char keys ;
/// the upstream Anthropic key-format is `sk-ant-...` but we don't enforce
/// the prefix so future key-rotations don't break the UI.
fn validate_key(key: &str) -> Result<(), SecretsError> {
    if key.is_empty() {
        return Err(SecretsError::InvalidKey("empty"));
    }
    if key.len() > 1024 {
        return Err(SecretsError::InvalidKey("over 1024 chars"));
    }
    if key.contains('\n') || key.contains('\r') {
        return Err(SecretsError::InvalidKey("contains newline"));
    }
    Ok(())
}

/// Persist `key` to `~/.loa-secrets/anthropic.env` as `ANTHROPIC_API_KEY=<key>`.
///
/// § Semantics
///   - Creates `~/.loa-secrets/` if missing.
///   - If the file already exists with other `KEY=VALUE` lines, the
///     `ANTHROPIC_API_KEY` line is replaced in-place ; unrelated lines are
///     preserved.
///   - On Windows, attempts `icacls` to restrict ACL to current user
///     (best-effort ; non-fatal on failure).
pub fn save_anthropic_key(key: &str) -> Result<(), SecretsError> {
    validate_key(key)?;
    let path = anthropic_env_path()?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let new_body = upsert_key_line(&existing, ANTHROPIC_KEY_VAR, key);
    std::fs::write(&path, new_body)?;

    // Best-effort Windows ACL hardening — if it fails, the file still
    // sits under user-profile which is owner-only by default.
    #[cfg(target_os = "windows")]
    {
        let _ = restrict_acl_windows(&path);
    }

    tracing::info!(
        path = %path.display(),
        masked = redact_for_log(key),
        "anthropic key persisted"
    );
    Ok(())
}

/// Read the persisted `ANTHROPIC_API_KEY` from `~/.loa-secrets/anthropic.env`.
/// Returns `None` if the file is missing OR the key-line is absent.
pub fn load_anthropic_key() -> Result<Option<String>, SecretsError> {
    let path = anthropic_env_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let body = std::fs::read_to_string(&path)?;
    Ok(extract_key_line(&body, ANTHROPIC_KEY_VAR))
}

/// `true` iff `~/.loa-secrets/anthropic.env` exists AND contains a
/// non-empty `ANTHROPIC_API_KEY=...` line.
#[must_use]
pub fn has_anthropic_key() -> bool {
    matches!(load_anthropic_key(), Ok(Some(ref k)) if !k.is_empty())
}

/// Mask an API-key for display : `sk-...XXXX` (first-3 + last-4). Keys
/// shorter than 7 chars collapse to `***` to avoid leaking the prefix on
/// truncated input.
#[must_use]
pub fn mask_key(key: &str) -> String {
    if key.len() < 7 {
        return "***".to_string();
    }
    let prefix: String = key.chars().take(3).collect();
    let suffix: String = key.chars().rev().take(4).collect::<String>().chars().rev().collect();
    format!("{prefix}...{suffix}")
}

/// Same shape as `mask_key` but accepts `Option<&str>` for log-line use.
fn redact_for_log(key: &str) -> String {
    mask_key(key)
}

/// Replace-or-append `<var>=<value>` in a body of `KEY=VALUE` lines.
/// Unrelated lines are preserved verbatim. Trailing newline is preserved.
fn upsert_key_line(body: &str, var: &str, value: &str) -> String {
    let mut found = false;
    let mut out = String::with_capacity(body.len() + var.len() + value.len() + 2);
    for line in body.lines() {
        if let Some(rest) = strip_var_prefix(line, var) {
            // Preserve any trailing whitespace/comments after `=` semantics
            // by collapsing — we choose semantics-over-syntax here. The
            // body becomes the canonical `var=value` form on overwrite.
            let _ = rest;
            out.push_str(var);
            out.push('=');
            out.push_str(value);
            out.push('\n');
            found = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !found {
        out.push_str(var);
        out.push('=');
        out.push_str(value);
        out.push('\n');
    }
    out
}

/// Extract the value of `<var>=<value>` from a body. Returns `None` if the
/// var-line is missing OR the value is empty.
fn extract_key_line(body: &str, var: &str) -> Option<String> {
    for line in body.lines() {
        if let Some(rest) = strip_var_prefix(line, var) {
            let trimmed = rest.trim();
            if trimmed.is_empty() {
                return None;
            }
            return Some(trimmed.to_string());
        }
    }
    None
}

/// If `line` starts with `<var>=`, return the value side ; else `None`.
fn strip_var_prefix<'a>(line: &'a str, var: &str) -> Option<&'a str> {
    let trimmed = line.trim_start();
    let after_var = trimmed.strip_prefix(var)?;
    let after_eq = after_var.strip_prefix('=')?;
    Some(after_eq)
}

/// Best-effort `icacls` invocation to restrict file ACL to the current
/// user. Returns `Ok(())` even on icacls-failure — the file already sits
/// in user-profile which is owner-only by default.
#[cfg(target_os = "windows")]
fn restrict_acl_windows(path: &std::path::Path) -> Result<(), SecretsError> {
    use std::process::Command;
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "Apocky".to_string());
    // icacls <path> /inheritance:r /grant:r <user>:F
    let _ = Command::new("icacls")
        .arg(path.as_os_str())
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{user}:F"))
        .output();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    /// Serialize tests that mutate `USERPROFILE`/`HOME` so they don't
    /// race each other.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_profile<F: FnOnce()>(test: F) {
        let _g = ENV_LOCK.lock().unwrap();
        let temp = std::env::temp_dir().join(format!(
            "cssl-secrets-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&temp).unwrap();

        let prev_userprofile = std::env::var_os("USERPROFILE");
        let prev_home = std::env::var_os("HOME");
        std::env::set_var("USERPROFILE", &temp);
        std::env::set_var("HOME", &temp);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));

        match prev_userprofile {
            Some(v) => std::env::set_var("USERPROFILE", v),
            None => std::env::remove_var("USERPROFILE"),
        }
        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        let _ = fs::remove_dir_all(&temp);

        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn save_then_load_round_trip() {
        with_temp_profile(|| {
            let key = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAA-XYZW";
            save_anthropic_key(key).expect("save");
            let loaded = load_anthropic_key().expect("load");
            assert_eq!(loaded.as_deref(), Some(key));
            assert!(has_anthropic_key());
        });
    }

    #[test]
    fn mask_short_key_becomes_stars() {
        assert_eq!(mask_key(""), "***");
        assert_eq!(mask_key("abc"), "***");
        assert_eq!(mask_key("sk-1234"), "sk-...1234");
    }

    #[test]
    fn mask_long_key_preserves_prefix_and_suffix() {
        let key = "sk-ant-api03-ABCDEFGHIJKLMNOPQRSTUVWXYZ-abcd";
        let m = mask_key(key);
        assert_eq!(m, "sk-...abcd");
        assert!(!m.contains("ABCDEFG"));
    }

    #[test]
    fn save_replaces_existing_key_line() {
        with_temp_profile(|| {
            save_anthropic_key("sk-old-1234567890abcd").unwrap();
            save_anthropic_key("sk-new-1234567890abcd").unwrap();
            assert_eq!(
                load_anthropic_key().unwrap().as_deref(),
                Some("sk-new-1234567890abcd")
            );
            // Ensure no double-line accumulation : the file must have
            // exactly one ANTHROPIC_API_KEY= line.
            let path = anthropic_env_path().unwrap();
            let body = fs::read_to_string(&path).unwrap();
            let count = body
                .lines()
                .filter(|l| l.starts_with(ANTHROPIC_KEY_VAR))
                .count();
            assert_eq!(count, 1);
        });
    }

    #[test]
    fn save_preserves_unrelated_lines() {
        with_temp_profile(|| {
            // Pre-seed with an unrelated KEY=VALUE line.
            let path = anthropic_env_path().unwrap();
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "OTHER_KEY=preserve-me\n").unwrap();
            save_anthropic_key("sk-test-1234567890").unwrap();
            let body = fs::read_to_string(&path).unwrap();
            assert!(body.contains("OTHER_KEY=preserve-me"));
            assert!(body.contains("ANTHROPIC_API_KEY=sk-test-1234567890"));
        });
    }

    #[test]
    fn empty_key_is_rejected() {
        with_temp_profile(|| {
            assert!(matches!(
                save_anthropic_key(""),
                Err(SecretsError::InvalidKey(_))
            ));
        });
    }

    #[test]
    fn newline_in_key_is_rejected() {
        with_temp_profile(|| {
            assert!(matches!(
                save_anthropic_key("sk-bad\nkey"),
                Err(SecretsError::InvalidKey(_))
            ));
        });
    }

    #[test]
    fn missing_file_returns_none() {
        with_temp_profile(|| {
            assert_eq!(load_anthropic_key().unwrap(), None);
            assert!(!has_anthropic_key());
        });
    }
}
