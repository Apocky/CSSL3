//! HTTPS transport for the public account contract.
//!
//! Ported from the Android client's `ApiClient.java`. All networking lives in
//! the Rust process: the webview never holds a token and never reaches the
//! network itself, so a rendering-side defect cannot leak or misdirect one.

use std::io::Read;
use std::time::Duration;

use crate::protocol::{self, ProtocolError};
use crate::session::AuthSession;

const MAX_REQUEST_BYTES: usize = 131_072;
const MAX_RESPONSE_BYTES: usize = 4_194_304;
const MAX_DIAGNOSTIC_BYTES: usize = 16_384;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// An HTTP status the service returned, with a sentence for the person.
    #[error("{message}")]
    Http { status: u16, message: String },
    /// A local or contract rejection; the message is already person-readable.
    #[error("{0}")]
    Rejected(String),
    #[error("The connection timed out. A sent message may still be running. Check the conversation before sending again.")]
    Timeout,
    #[error("Unable to connect securely. Check your internet connection and retry.")]
    Transport,
}

impl From<ProtocolError> for ApiError {
    fn from(error: ProtocolError) -> Self {
        ApiError::Rejected(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ApiError>;

pub struct ApiClient {
    agent: ureq::Agent,
    public_key: Option<String>,
}

impl Default for ApiClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiClient {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            .redirects(0)
            .user_agent(protocol::USER_AGENT)
            .build();
        Self { agent, public_key: None }
    }

    pub fn configured(&self) -> bool {
        self.public_key.is_some()
    }

    /// Reads the site's public configuration and remembers the anon key.
    pub fn configure(&mut self) -> Result<()> {
        let response = self.request(
            &format!("{}/api/mobile/config", protocol::SITE),
            "GET",
            None,
            None,
            false,
        )?;
        self.public_key = Some(protocol::config(&response)?);
        Ok(())
    }

    pub fn send_code(&self, address: &str) -> Result<()> {
        let body = serde_json::json!({ "email": protocol::email(address)?, "create_user": false });
        self.auth("/otp", Some(body), None)?;
        Ok(())
    }

    pub fn create_account(&self, address: &str) -> Result<()> {
        let body = serde_json::json!({ "email": protocol::email(address)?, "create_user": true });
        self.auth("/otp", Some(body), None)?;
        Ok(())
    }

    /// Returns `None` when the service requires email confirmation first.
    pub fn create_with_password(&self, address: &str, password: &str) -> Result<Option<AuthSession>> {
        if password.len() < 8 || password.len() > 4096 {
            return Err(ApiError::Rejected(
                "Choose a password with at least eight characters.".into(),
            ));
        }
        let body = serde_json::json!({ "email": protocol::email(address)?, "password": password });
        let response = self.auth("/signup", Some(body), None)?;
        match response.get("access_token") {
            Some(token) if !token.is_null() => Ok(Some(self.session(&response)?)),
            _ => Ok(None),
        }
    }

    pub fn verify_code(&self, address: &str, value: &str) -> Result<AuthSession> {
        let body = serde_json::json!({
            "email": protocol::email(address)?,
            "token": protocol::code(value)?,
            "type": "email",
        });
        let tokens = self.auth("/verify", Some(body), None)?;
        self.session(&tokens)
    }

    pub fn password(&self, address: &str, password: &str) -> Result<AuthSession> {
        if password.is_empty() || password.len() > 4096 {
            return Err(ApiError::Rejected("Enter your account password.".into()));
        }
        let body = serde_json::json!({ "email": protocol::email(address)?, "password": password });
        let tokens = self.auth("/token?grant_type=password", Some(body), None)?;
        self.session(&tokens)
    }

    pub fn refresh(&self, refresh: &str, expected_user: &str) -> Result<AuthSession> {
        let body = serde_json::json!({ "refresh_token": refresh });
        let tokens = self.auth("/token?grant_type=refresh_token", Some(body), None)?;
        let result = self.session(&tokens)?;
        if result.user_id != expected_user {
            return Err(ApiError::Rejected(
                "The saved session belongs to another account.".into(),
            ));
        }
        Ok(result)
    }

    pub fn logout(&self, token: &str) -> Result<()> {
        self.auth("/logout?scope=local", Some(serde_json::json!({})), Some(token))?;
        Ok(())
    }

    /// A call against the account API on apocky.com; `None` body means GET.
    pub fn account(&self, path: &str, body: Option<serde_json::Value>, token: &str) -> Result<serde_json::Value> {
        let method = if body.is_some() { "POST" } else { "GET" };
        self.request(&format!("{}{path}", protocol::API), method, body, Some(token), false)
    }

    fn session(&self, tokens: &serde_json::Value) -> Result<AuthSession> {
        let access = protocol::text(tokens, "access_token", 16_384)?;
        let user = self.request(
            &format!("{}/auth/v1/user", protocol::AUTH),
            "GET",
            None,
            Some(&access),
            true,
        )?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_secs() as i64)
            .unwrap_or_default();
        AuthSession::from_response(tokens, &user, now).map_err(ApiError::from)
    }

    fn auth(&self, path: &str, body: Option<serde_json::Value>, token: Option<&str>) -> Result<serde_json::Value> {
        if self.public_key.is_none() {
            return Err(ApiError::Rejected(
                "Sign-in is unavailable. Check your connection and retry.".into(),
            ));
        }
        self.request(&format!("{}/auth/v1{path}", protocol::AUTH), "POST", body, token, true)
    }

    fn request(
        &self,
        raw_url: &str,
        method: &str,
        body: Option<serde_json::Value>,
        token: Option<&str>,
        auth: bool,
    ) -> Result<serde_json::Value> {
        let url = protocol::endpoint(raw_url)?;
        let read_timeout = if raw_url == format!("{}/turn", protocol::API) {
            Duration::from_secs(300)
        } else {
            Duration::from_secs(30)
        };
        let mut request = self
            .agent
            .request_url(method, &url)
            .timeout(read_timeout)
            .set("Accept", "application/json");
        if auth {
            if let Some(key) = &self.public_key {
                request = request.set("apikey", key);
            }
        }
        if let Some(value) = token {
            request = request.set("Authorization", &format!("Bearer {value}"));
        }
        if raw_url.starts_with(&format!("{}/", protocol::SITE)) {
            request = request.set("Origin", protocol::SITE);
        }

        let outcome = match body {
            Some(payload) => {
                let encoded = serde_json::to_vec(&payload)
                    .map_err(|_| ApiError::Rejected("The request could not be prepared.".into()))?;
                if encoded.len() > MAX_REQUEST_BYTES {
                    return Err(ApiError::Rejected("The request is too large.".into()));
                }
                request.set("Content-Type", "application/json").send_bytes(&encoded)
            }
            None => request.call(),
        };

        let response = match outcome {
            Ok(response) => response,
            Err(ureq::Error::Status(status, response)) => {
                if (300..400).contains(&status) {
                    return Err(ApiError::Http {
                        status,
                        message: "The service tried to redirect this secure request. Update the app or retry later.".into(),
                    });
                }
                let trace = response.header("X-Apocky-Trace-Id").map(str::to_string);
                let json_error = response
                    .header("Content-Type")
                    .map(|value| value.to_ascii_lowercase().starts_with("application/json"))
                    .unwrap_or(false);
                let diagnostic = if !auth && json_error {
                    read_bounded(response, MAX_DIAGNOSTIC_BYTES).unwrap_or_default()
                } else {
                    String::new()
                };
                return Err(failure(status, auth, &diagnostic, trace.as_deref()));
            }
            Err(ureq::Error::Transport(transport)) => {
                return Err(match transport.kind() {
                    ureq::ErrorKind::Io => ApiError::Timeout,
                    _ => ApiError::Transport,
                });
            }
        };

        if response.status() == 204 {
            return Ok(serde_json::json!({}));
        }
        let json_body = response
            .header("Content-Type")
            .map(|value| value.to_ascii_lowercase().starts_with("application/json"))
            .unwrap_or(false);
        if !json_body {
            return Err(ApiError::Rejected(
                "The service returned an unexpected response.".into(),
            ));
        }
        let text = read_bounded(response, MAX_RESPONSE_BYTES)?;
        if text.is_empty() {
            return Ok(serde_json::json!({}));
        }
        serde_json::from_str(&text).map_err(|_| {
            ApiError::Rejected("The service response could not be verified. Update the app or retry later.".into())
        })
    }
}

fn read_bounded(response: ureq::Response, limit: usize) -> Result<String> {
    let mut buffer = Vec::new();
    response
        .into_reader()
        .take(limit as u64 + 1)
        .read_to_end(&mut buffer)
        .map_err(|_| ApiError::Transport)?;
    if buffer.len() > limit {
        return Err(ApiError::Rejected("The service response is too large.".into()));
    }
    String::from_utf8(buffer)
        .map_err(|_| ApiError::Rejected("The service returned an unexpected response.".into()))
}

/// Maps a status onto the same sentences the Android client shows, so a person
/// comparing phone and desktop sees one vocabulary for one failure.
fn failure(status: u16, auth: bool, body: &str, trace: Option<&str>) -> ApiError {
    let base = match status {
        401 => "Your session was rejected. Sign out and sign in again.".to_string(),
        403 if !auth => "This account could not access this conversation. Sign in again or try a new conversation.".to_string(),
        400 | 422 | 403 if auth => "Sign-in was not accepted. Check your email, code or password. If additional verification is required, use apocky.com.".to_string(),
        404 => "This service or conversation is unavailable. Retry after the service is updated.".to_string(),
        429 => "Too many requests. Wait a little before trying again.".to_string(),
        other => format!("Apocrypha is temporarily unavailable (HTTP {other})."),
    };
    let code = support_code(body)
        .map(|value| format!(" Support code: {value}."))
        .unwrap_or_default();
    let reference = trace
        .filter(|value| is_trace(value))
        .map(|value| format!(" Trace: {value}."))
        .unwrap_or_default();
    ApiError::Http {
        status,
        message: format!("{base}{code}{reference}"),
    }
}

fn support_code(body: &str) -> Option<String> {
    let candidate = serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("code")?
        .as_str()?
        .to_string();
    let (prefix, rest) = candidate.split_once('_')?;
    let known = matches!(prefix, "ACCOUNT" | "CONTROL" | "OBSERVATION" | "BRIDGE");
    let shaped = (1..=72).contains(&rest.len())
        && rest.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
    (known && shaped).then_some(candidate)
}

fn is_trace(value: &str) -> bool {
    let hex = |s: &str| s.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'));
    if value.len() == 32 {
        return hex(value);
    }
    let parts: Vec<&str> = value.split('-').collect();
    parts.len() == 5
        && [8usize, 4, 4, 4, 12] == parts.iter().map(|p| p.len()).collect::<Vec<_>>()[..]
        && parts.iter().all(|p| hex(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_map_to_the_shared_vocabulary() {
        let unauthorised = failure(401, false, "", None);
        assert!(unauthorised.to_string().contains("Sign out and sign in again"));
        let signin = failure(400, true, "", None);
        assert!(signin.to_string().contains("Sign-in was not accepted"));
        let forbidden = failure(403, false, "", None);
        assert!(forbidden.to_string().contains("could not access this conversation"));
        let throttled = failure(429, false, "", None);
        assert!(throttled.to_string().contains("Too many requests"));
        let unknown = failure(503, false, "", None);
        assert!(unknown.to_string().contains("HTTP 503"));
    }

    #[test]
    fn only_recognised_support_codes_reach_the_person() {
        assert_eq!(
            support_code("{\"code\":\"ACCOUNT_NOT_PROVISIONED\"}").as_deref(),
            Some("ACCOUNT_NOT_PROVISIONED")
        );
        assert_eq!(support_code("{\"code\":\"SECRET_leak\"}"), None);
        assert_eq!(support_code("{\"code\":\"account_lowercase\"}"), None);
        assert_eq!(support_code("not json"), None);
        assert_eq!(support_code("{\"message\":\"no code here\"}"), None);
    }

    #[test]
    fn trace_identifiers_must_look_like_identifiers() {
        assert!(is_trace(&"a".repeat(32)));
        assert!(is_trace("0f9c1d2e-3a4b-5c6d-7e8f-90a1b2c3d4e5"));
        assert!(!is_trace("not-a-trace"));
        assert!(!is_trace(&"z".repeat(32)));
        assert!(!is_trace("<script>alert(1)</script>"));
    }

    #[test]
    fn an_unconfigured_client_refuses_to_authenticate() {
        let client = ApiClient::new();
        assert!(!client.configured());
        let error = client.send_code("person@example.com").unwrap_err();
        assert!(error.to_string().contains("Sign-in is unavailable"));
    }

    /// Reaches the real service. Run with `cargo test -- --ignored` when
    /// checking a release; it is out of the default run so the suite stays
    /// offline and deterministic.
    #[test]
    #[ignore = "reaches https://www.apocky.com"]
    fn the_live_service_still_serves_a_configuration_this_client_accepts() {
        let mut client = ApiClient::new();
        client.configure().expect("the live /api/mobile/config must satisfy the contract");
        assert!(client.configured());
    }

    #[test]
    fn short_passwords_are_refused_before_any_request() {
        let client = ApiClient::new();
        let error = client.create_with_password("person@example.com", "short").unwrap_err();
        assert!(error.to_string().contains("at least eight characters"));
    }
}
