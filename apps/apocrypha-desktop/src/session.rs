//! A verified sign-in. Ported from the Android client's `AuthSession.java`.

use crate::protocol::{self, ProtocolError, Result};

#[derive(Debug, Clone)]
pub struct AuthSession {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: String,
    pub email: String,
    pub expires_at: i64,
}

fn reject(message: &str) -> ProtocolError {
    ProtocolError::Rejected(message.to_string())
}

fn token(data: &serde_json::Value, name: &str) -> Result<String> {
    let value = protocol::text(data, name, 16_384)?;
    if value.trim().is_empty() || value.chars().any(char::is_whitespace) {
        return Err(reject("The sign-in session is invalid."));
    }
    Ok(value)
}

impl AuthSession {
    /// Builds a session only from tokens whose owner the service confirmed.
    ///
    /// `verified_user` must be the body of `/auth/v1/user` fetched with the
    /// access token itself — trusting the id embedded in the token response
    /// alone would let a swapped payload rebind a session to another account.
    pub fn from_response(tokens: &serde_json::Value, verified_user: &serde_json::Value, now: i64) -> Result<Self> {
        let access = token(tokens, "access_token")?;
        let refresh = token(tokens, "refresh_token")?;
        let user = protocol::text(verified_user, "id", 64)?;
        let shaped = user.len() == 36
            && user
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f') || b == b'-');
        let confirmed = verified_user
            .get("email_confirmed_at")
            .and_then(|value| value.as_str())
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        if !shaped || !confirmed {
            return Err(reject("A verified email account is required."));
        }
        if let Some(claimed) = tokens.get("user").and_then(|value| value.get("id")).and_then(|v| v.as_str()) {
            if claimed != user {
                return Err(reject("The sign-in response belongs to another account."));
            }
        }
        let expires = tokens
            .get("expires_at")
            .and_then(|value| value.as_i64())
            .unwrap_or_else(|| now + tokens.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(0));
        if expires <= now || expires > now + 604_800 {
            return Err(reject("The sign-in session has an invalid expiry."));
        }
        Ok(Self {
            access_token: access,
            refresh_token: refresh,
            user_id: user,
            email: protocol::email(&protocol::text(verified_user, "email", 254)?)?,
            expires_at: expires,
        })
    }

    /// The only part of a session that is ever written to disk.
    pub fn saved(&self) -> serde_json::Value {
        serde_json::json!({ "refresh_token": self.refresh_token, "user_id": self.user_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000;

    fn tokens() -> serde_json::Value {
        serde_json::json!({
            "access_token": "access-token-value",
            "refresh_token": "refresh-token-value",
            "expires_in": 3600,
        })
    }

    fn user() -> serde_json::Value {
        serde_json::json!({
            "id": "0f9c1d2e-3a4b-4c6d-8e8f-90a1b2c3d4e5",
            "email": "person@example.com",
            "email_confirmed_at": "2026-01-01T00:00:00Z",
        })
    }

    #[test]
    fn a_confirmed_account_produces_a_session() {
        let session = AuthSession::from_response(&tokens(), &user(), NOW).unwrap();
        assert_eq!(session.email, "person@example.com");
        assert_eq!(session.expires_at, NOW + 3600);
        assert_eq!(session.saved()["refresh_token"], "refresh-token-value");
        assert!(session.saved().get("access_token").is_none(), "the access token must never be persisted");
    }

    #[test]
    fn an_unconfirmed_email_is_refused() {
        let mut unconfirmed = user();
        unconfirmed["email_confirmed_at"] = serde_json::Value::Null;
        assert!(AuthSession::from_response(&tokens(), &unconfirmed, NOW).is_err());
        unconfirmed["email_confirmed_at"] = serde_json::json!("   ");
        assert!(AuthSession::from_response(&tokens(), &unconfirmed, NOW).is_err());
    }

    #[test]
    fn a_token_payload_may_not_claim_a_different_account() {
        let mut swapped = tokens();
        swapped["user"] = serde_json::json!({ "id": "11111111-2222-4333-8444-555555555555" });
        assert!(AuthSession::from_response(&swapped, &user(), NOW).is_err());
    }

    #[test]
    fn expiries_outside_a_week_are_refused() {
        let mut expired = tokens();
        expired["expires_at"] = serde_json::json!(NOW - 1);
        assert!(AuthSession::from_response(&expired, &user(), NOW).is_err());
        let mut forever = tokens();
        forever["expires_at"] = serde_json::json!(NOW + 604_801);
        assert!(AuthSession::from_response(&forever, &user(), NOW).is_err());
    }

    #[test]
    fn tokens_containing_whitespace_are_refused() {
        for name in ["access_token", "refresh_token"] {
            let mut broken = tokens();
            broken[name] = serde_json::json!("value with space");
            assert!(AuthSession::from_response(&broken, &user(), NOW).is_err());
            broken[name] = serde_json::json!("   ");
            assert!(AuthSession::from_response(&broken, &user(), NOW).is_err());
        }
    }
}
