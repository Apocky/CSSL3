//! Wire contract for the public apocky.com account API.
//!
//! This is a faithful port of the Android client's `Protocol.java`. The two
//! clients must accept and reject exactly the same payloads, so any change
//! here is a change to the shared contract, not a desktop-only detail.

use serde::{Deserialize, Serialize};
use url::Url;

pub const SITE: &str = "https://www.apocky.com";
pub const AUTH: &str = "https://pzirbmyfmrbtkllrtcmx.supabase.co";
pub const MAX_TEXT_BYTES: usize = 16_384;
pub const USER_AGENT: &str = concat!("Apocrypha-Desktop/", env!("CARGO_PKG_VERSION"));

const SITE_HOST: &str = "www.apocky.com";
const AUTH_HOST: &str = "pzirbmyfmrbtkllrtcmx.supabase.co";

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// Carries a sentence that is safe to show the person using the app.
    #[error("{0}")]
    Rejected(String),
}

pub type Result<T> = std::result::Result<T, ProtocolError>;

fn reject(message: &str) -> ProtocolError {
    ProtocolError::Rejected(message.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Conversation {
    pub id: String,
    pub title: String,
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Accepts only the RFC 4122 version-4 shape the service issues.
pub fn id(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return Err(reject("Invalid conversation reference."));
    }
    for (index, byte) in bytes.iter().enumerate() {
        let ok = match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            14 => *byte == b'4',
            19 => matches!(*byte, b'8' | b'9' | b'a' | b'b'),
            _ => byte.is_ascii_digit() || matches!(*byte, b'a'..=b'f'),
        };
        if !ok {
            return Err(reject("Invalid conversation reference."));
        }
    }
    Ok(value.to_string())
}

pub fn prompt(value: &str) -> Result<String> {
    let text = value.trim();
    let bytes = text.len();
    if bytes < 1 || bytes > MAX_TEXT_BYTES {
        return Err(reject("Write a message of up to 16 KB."));
    }
    // Rust strings cannot hold lone surrogates, so only the control-character
    // rule from the Android contract still has work to do here.
    if text
        .chars()
        .any(|ch| ch.is_control() && ch != '\n' && ch != '\r' && ch != '\t')
    {
        return Err(reject("The message contains an invalid character."));
    }
    Ok(text.to_string())
}

pub fn email(value: &str) -> Result<String> {
    let email = value.trim();
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    let single_at = parts.next().is_none();
    let no_space = !email.chars().any(char::is_whitespace);
    let domain_dotted = domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.');
    if email.len() > 254 || email.is_empty() || !single_at || !no_space || local.is_empty() || !domain_dotted {
        return Err(reject("Enter your apocky.com email address."));
    }
    Ok(email.to_string())
}

pub fn code(value: &str) -> Result<String> {
    let code = value.trim();
    if code.len() < 6 || code.len() > 10 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(reject("Enter the numeric code from your email."));
    }
    Ok(code.to_string())
}

/// The single gate every outbound request passes through.
///
/// Anything not named here cannot be reached by this application, which is
/// what keeps a compromised or mistaken caller from turning the client into a
/// general-purpose fetcher.
pub fn endpoint(raw: &str) -> Result<Url> {
    let url = Url::parse(raw).map_err(|_| reject("Untrusted endpoint."))?;
    if url.scheme() != "https" || url.port().is_some() || !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(reject("Untrusted endpoint."));
    }
    let host = url.host_str().unwrap_or_default();
    let path = url.path();
    let query = url.query();
    let permitted = match host {
        SITE_HOST => match (path, query) {
            ("/api/mobile/config", None)
            | ("/api/apocrypha/chat", None)
            | ("/api/apocrypha/sessions", None) => true,
            ("/api/apocrypha/sessions", Some(q)) => q
                .strip_prefix("session_id=")
                .map(|value| id(value).is_ok())
                .unwrap_or(false),
            _ => false,
        },
        AUTH_HOST => match (path, query) {
            ("/auth/v1/otp", None)
            | ("/auth/v1/verify", None)
            | ("/auth/v1/user", None)
            | ("/auth/v1/signup", None) => true,
            ("/auth/v1/token", Some("grant_type=password")) => true,
            ("/auth/v1/token", Some("grant_type=refresh_token")) => true,
            ("/auth/v1/logout", Some("scope=local")) => true,
            _ => false,
        },
        _ => false,
    };
    if !permitted {
        return Err(reject("Untrusted endpoint."));
    }
    Ok(url)
}

fn schema(value: &serde_json::Value, expected: &str) -> Result<()> {
    if value.get("schema_version").and_then(|v| v.as_str()) == Some(expected) {
        Ok(())
    } else {
        Err(reject("The service returned an unsupported response."))
    }
}

pub fn text(value: &serde_json::Value, key: &str, limit: usize) -> Result<String> {
    match value.get(key).and_then(|v| v.as_str()) {
        Some(found) if found.chars().count() <= limit => Ok(found.to_string()),
        _ => Err(reject("The response exceeded its contract.")),
    }
}

/// Verifies `/api/mobile/config` and returns the public authentication key.
pub fn config(data: &serde_json::Value) -> Result<String> {
    schema(data, "apocky.mobile-config.v1")?;
    let matches = data.get("site_url").and_then(|v| v.as_str()) == Some(SITE)
        && data.get("supabase_url").and_then(|v| v.as_str()) == Some(AUTH)
        && data.get("api_base").and_then(|v| v.as_str()) == Some("/api/mobile")
        && data.get("access").and_then(|v| v.as_str()) == Some("account");
    if !matches {
        return Err(reject("The app configuration does not match this service."));
    }
    let key = text(data, "supabase_publishable_key", 8192)?;
    if key.len() < 20 || key.contains('\n') || key.starts_with("sb_secret_") {
        return Err(reject("Invalid public authentication configuration."));
    }
    if !key.starts_with("sb_publishable_") && !anon_claim(&key) {
        return Err(reject("Only a public authentication key is accepted."));
    }
    Ok(key)
}

fn anon_claim(key: &str) -> bool {
    use base64::Engine;
    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .ok()
        .and_then(|raw| serde_json::from_slice::<serde_json::Value>(&raw).ok())
        .and_then(|claims| claims.get("role").and_then(|v| v.as_str()).map(str::to_string))
        .map(|role| role == "anon")
        .unwrap_or(false)
}

/// The exact three-key body `/api/apocrypha/chat` accepts; anything else is a 400.
pub fn chat_body(text_value: &str, session: &str, request: &str) -> Result<serde_json::Value> {
    Ok(serde_json::json!({
        "text": prompt(text_value)?,
        "session_id": id(session)?,
        "request_id": id(request)?,
    }))
}

/// Reads the terminal result of a streamed turn, refusing any reply not bound
/// to this exact request.
pub fn chat_result(result: &serde_json::Value, session: &str, request: &str) -> Result<String> {
    let bound = result.get("outcome").and_then(|v| v.as_str()) == Some("completed")
        && result.get("session_id").and_then(|v| v.as_str()) == Some(id(session)?.as_str())
        && result.get("request_id").and_then(|v| v.as_str()) == Some(id(request)?.as_str());
    if !bound {
        return Err(reject("The reply does not match this message."));
    }
    // The runtime states which model answered and a digest of what it said.
    // A reply arriving without that evidence is not one this client will show.
    let model = text(result, "model_id", 512)?;
    let digest = text(result, "response_digest", 64)?;
    let shaped = !model.is_empty()
        && digest.len() == 64
        && digest.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'));
    if !shaped {
        return Err(reject("The reply did not carry its response evidence."));
    }
    text(result, "text", 2_097_152)
}

/// The durable conversation list for this account.
///
/// The service returns the client's own identifiers here, so a conversation
/// listed can be opened directly by the id it carries.
pub fn sessions(value: &serde_json::Value) -> Result<Vec<Conversation>> {
    let rows = value
        .get("sessions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| reject("The service returned an unsupported response."))?;
    if rows.len() > 128 {
        return Err(reject("Too many conversation entries."));
    }
    let mut conversations = Vec::with_capacity(rows.len());
    for row in rows {
        let count = row
            .get("message_count")
            .and_then(|v| v.as_i64())
            .filter(|value| *value >= 0)
            .ok_or_else(|| reject("The conversation list returned an invalid thread."))?;
        let _ = count;
        conversations.push(Conversation {
            id: id(&text(row, "session_id", 64)?)?,
            title: text(row, "title", 4096)?,
        });
    }
    Ok(conversations)
}

pub struct History {
    pub messages: Vec<Message>,
    pub truncated: bool,
}

pub fn history(value: &serde_json::Value, expected: &str) -> Result<History> {
    let session = value
        .get("session")
        .ok_or_else(|| reject("The service returned an unsupported response."))?;
    schema(session, "apocv4.workspace-session-snapshot.v1")?;
    if text(session, "session_id", 64)? != id(expected)? {
        return Err(reject("The history belongs to another conversation."));
    }
    let rows = session
        .get("messages")
        .and_then(|v| v.as_array())
        .ok_or_else(|| reject("The service returned an unsupported response."))?;
    if rows.len() > 2000 {
        return Err(reject("The conversation is too large to display safely."));
    }
    let mut messages = Vec::with_capacity(rows.len());
    for row in rows {
        let role = text(row, "role", 32)?;
        if role != "user" && role != "assistant" {
            return Err(reject("Unknown message type."));
        }
        messages.push(Message {
            role,
            content: text(row, "content", 2_097_152)?,
            request_id: id(&text(row, "request_id", 64)?)?,
        });
    }
    let truncated = session
        .get("events_truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(History { messages, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_accept_only_version_four_uuids() {
        let good = new_id();
        assert!(id(&good).is_ok());
        for bad in [
            "",
            "not-a-uuid",
            "00000000-0000-0000-0000-000000000000",
            "00000000-0000-3000-8000-000000000000",
            "00000000-0000-4000-c000-000000000000",
            "00000000-0000-4000-8000-00000000000",
            "00000000-0000-4000-8000-0000000000000",
            "00000000_0000_4000_8000_000000000000",
            "00000000-0000-4000-8000-00000000000G",
        ] {
            assert!(id(bad).is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn prompts_are_bounded_and_control_free() {
        assert_eq!(prompt("  hello  ").unwrap(), "hello");
        assert!(prompt("line\nbreak\tand\rreturn").is_ok());
        assert!(prompt("").is_err());
        assert!(prompt("   ").is_err());
        assert!(prompt("bad\u{0}byte").is_err());
        assert!(prompt(&"x".repeat(MAX_TEXT_BYTES)).is_ok());
        assert!(prompt(&"x".repeat(MAX_TEXT_BYTES + 1)).is_err());
    }

    #[test]
    fn emails_need_one_at_and_a_dotted_domain() {
        assert_eq!(email(" person@example.com ").unwrap(), "person@example.com");
        for bad in ["", "person", "person@", "@example.com", "a@b@c.com", "person@example", "person@.com", "person@example.", "person name@example.com"] {
            assert!(email(bad).is_err(), "{bad} must be refused");
        }
        assert!(email(&format!("{}@example.com", "a".repeat(250))).is_err());
    }

    #[test]
    fn endpoints_outside_the_allowlist_are_refused() {
        for good in [
            "https://www.apocky.com/api/mobile/config",
            "https://www.apocky.com/api/apocrypha/chat",
            "https://www.apocky.com/api/apocrypha/sessions",
            "https://pzirbmyfmrbtkllrtcmx.supabase.co/auth/v1/otp",
            "https://pzirbmyfmrbtkllrtcmx.supabase.co/auth/v1/token?grant_type=password",
            "https://pzirbmyfmrbtkllrtcmx.supabase.co/auth/v1/token?grant_type=refresh_token",
            "https://pzirbmyfmrbtkllrtcmx.supabase.co/auth/v1/logout?scope=local",
        ] {
            assert!(endpoint(good).is_ok(), "{good} must be permitted");
        }
        for bad in [
            "http://www.apocky.com/api/mobile/config",
            "https://apocky.com/api/mobile/config",
            "https://www.apocky.com.evil.test/api/mobile/config",
            "https://www.apocky.com@evil.test/api/mobile/config",
            "https://www.apocky.com:8443/api/mobile/config",
            "https://www.apocky.com/api/mobile/config#fragment",
            "https://www.apocky.com/api/mobile/config?token=secret",
            "https://www.apocky.com/api/admin/apocrypha/keys",
            "https://www.apocky.com/api/apocrypha/sessions?session_id=../secret",
            "https://www.apocky.com/api/mobile/turn",
            "https://www.apocky.com/api/apocrypha/member/jobs",
            "https://www.apocky.com/api/apocrypha/vision",
            "https://pzirbmyfmrbtkllrtcmx.supabase.co/auth/v1/token?grant_type=client_credentials",
            "https://pzirbmyfmrbtkllrtcmx.supabase.co/auth/v1/logout?scope=global",
            "https://pzirbmyfmrbtkllrtcmx.supabase.co/rest/v1/profiles",
            "https://evil.test/api/mobile/config",
        ] {
            assert!(endpoint(bad).is_err(), "{bad} must be refused");
        }
        let permitted = format!("https://www.apocky.com/api/apocrypha/sessions?session_id={}", new_id());
        assert!(endpoint(&permitted).is_ok());
    }

    fn live_config() -> serde_json::Value {
        serde_json::json!({
            "schema_version": "apocky.mobile-config.v1",
            "site_url": SITE,
            "supabase_url": AUTH,
            "supabase_publishable_key": "sb_publishable_0123456789abcdef",
            "api_base": "/api/mobile",
            "access": "account",
        })
    }

    #[test]
    fn config_refuses_anything_but_a_public_key_for_this_service() {
        assert!(config(&live_config()).is_ok());
        for (key, value) in [
            ("schema_version", serde_json::json!("apocky.mobile-config.v2")),
            ("site_url", serde_json::json!("https://evil.test")),
            ("supabase_url", serde_json::json!("https://evil.supabase.co")),
            ("api_base", serde_json::json!("/api/owner")),
            ("access", serde_json::json!("public")),
            ("supabase_publishable_key", serde_json::json!("sb_secret_0123456789abcdef")),
            ("supabase_publishable_key", serde_json::json!("short")),
        ] {
            let mut broken = live_config();
            broken[key] = value;
            assert!(config(&broken).is_err(), "{key} must be checked");
        }
    }

    #[test]
    fn service_role_tokens_are_refused_where_anon_tokens_pass() {
        use base64::Engine;
        let encode = |role: &str| {
            let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(format!("{{\"role\":\"{role}\"}}").as_bytes());
            format!("header.{claims}.signature")
        };
        let mut anon = live_config();
        anon["supabase_publishable_key"] = serde_json::json!(encode("anon"));
        assert!(config(&anon).is_ok());
        let mut service = live_config();
        service["supabase_publishable_key"] = serde_json::json!(encode("service_role"));
        assert!(config(&service).is_err());
    }

    fn terminal(session: &str, request: &str) -> serde_json::Value {
        serde_json::json!({
            "outcome": "completed",
            "session_id": session,
            "request_id": request,
            "text": "answer",
            "model_id": "apocrypha-runtime",
            "response_digest": "a".repeat(64),
        })
    }

    #[test]
    fn a_reply_must_be_bound_to_its_own_request() {
        let session = new_id();
        let request = new_id();
        let reply = terminal(&session, &request);
        assert_eq!(chat_result(&reply, &session, &request).unwrap(), "answer");
        assert!(chat_result(&reply, &new_id(), &request).is_err());
        assert!(chat_result(&reply, &session, &new_id()).is_err());
        let mut running = reply.clone();
        running["outcome"] = serde_json::json!("running");
        assert!(chat_result(&running, &session, &request).is_err());
    }

    #[test]
    fn a_reply_without_response_evidence_is_refused() {
        let session = new_id();
        let request = new_id();
        for (key, value) in [
            ("model_id", serde_json::json!("")),
            ("response_digest", serde_json::json!("not-a-digest")),
            ("response_digest", serde_json::json!("A".repeat(64))),
            ("model_id", serde_json::Value::Null),
        ] {
            let mut broken = terminal(&session, &request);
            broken[key] = value;
            assert!(chat_result(&broken, &session, &request).is_err(), "{key} must be checked");
        }
    }

    #[test]
    fn history_must_belong_to_the_conversation_that_asked_for_it() {
        let session = new_id();
        let request = new_id();
        let payload = serde_json::json!({
            "session": {
                "schema_version": "apocv4.workspace-session-snapshot.v1",
                "session_id": session,
                "events_truncated": true,
                "messages": [{ "role": "user", "content": "hi", "request_id": request }],
            },
        });
        let parsed = history(&payload, &session).unwrap();
        assert!(parsed.truncated);
        assert_eq!(parsed.messages.len(), 1);
        assert!(history(&payload, &new_id()).is_err());
        let mut wrong_schema = payload.clone();
        wrong_schema["session"]["schema_version"] = serde_json::json!("apocv4.workspace-session-snapshot.v2");
        assert!(history(&wrong_schema, &session).is_err());
    }

    #[test]
    fn conversation_lists_need_a_client_identifier_and_a_count() {
        let listed = new_id();
        let payload = serde_json::json!({
            "sessions": [{
                "session_id": listed,
                "title": "A conversation",
                "updated_at": "2026-09-09T00:00:00Z",
                "message_count": 4,
                "active_job_count": 0,
            }],
            "count": 1,
        });
        let parsed = sessions(&payload).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, listed);
        for (key, value) in [
            ("session_id", serde_json::json!("not-a-uuid")),
            ("message_count", serde_json::json!(-1)),
            ("message_count", serde_json::json!("four")),
            ("title", serde_json::Value::Null),
        ] {
            let mut broken = payload.clone();
            broken["sessions"][0][key] = value;
            assert!(sessions(&broken).is_err(), "{key} must be checked");
        }
        assert!(sessions(&serde_json::json!({ "count": 0 })).is_err());
    }
}
