// § transport.rs — HTTP transport trait + mock impl
// Stage-0 : ureq is NOT in workspace.dependencies. We define the trait
// interface here so test-suites and offline-runs are decoupled from real
// network I/O. G1 wires a real `UreqHttpTransport` impl into this trait.

use crate::{StripeError, StripeResult};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// § HttpRequest — shape passed to `HttpTransport::send`.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: HttpMethod,
    /// Full URL including base + path (e.g. `https://api.stripe.com/v1/checkout/sessions`).
    pub url: String,
    /// Header pairs — deterministic via BTreeMap.
    pub headers: BTreeMap<String, String>,
    /// `application/x-www-form-urlencoded` body. Empty for GET.
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Delete,
}

impl HttpMethod {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Delete => "DELETE",
        }
    }
}

/// § HttpResponse — shape returned by `HttpTransport::send`.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    #[must_use]
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// § HttpTransport — pluggable HTTP layer.
///
/// G1 will provide `UreqHttpTransport` once `ureq` lands in
/// `workspace.dependencies`. Stage-0 tests use only `MockHttpTransport`.
pub trait HttpTransport: Send + Sync {
    /// Send the request, return the response. Network errors map to
    /// `StripeError::Network`.
    fn send(&self, req: HttpRequest) -> StripeResult<HttpResponse>;
}

// ══════════════════════════════════════════════════════════════════
// § MockHttpTransport — deterministic, scriptable in tests
// ══════════════════════════════════════════════════════════════════

/// § Programmed reply for the next-N matching requests.
#[derive(Debug, Clone)]
pub struct ProgrammedReply {
    /// Match request whose URL ends with this suffix (e.g. `/v1/refunds`).
    pub url_suffix: String,
    pub method: HttpMethod,
    pub response: HttpResponse,
}

/// § MockHttpTransport — records all requests, replays scripted responses.
///
/// Default behavior when no programmed-reply matches: return
/// `StripeError::Network("no mock reply programmed")`.
#[derive(Debug, Default)]
pub struct MockHttpTransport {
    state: Mutex<MockState>,
}

#[derive(Debug, Default)]
struct MockState {
    program: Vec<ProgrammedReply>,
    requests: Vec<HttpRequest>,
    /// Optional override : every send returns this network-error.
    force_network_error: Option<String>,
}

impl MockHttpTransport {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Program a one-shot reply (consumed on first match).
    pub fn program(&self, reply: ProgrammedReply) {
        let mut s = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        s.program.push(reply);
    }

    /// Force the next call (and all subsequent calls) to return a network error.
    pub fn force_network_error(&self, msg: impl Into<String>) {
        let mut s = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        s.force_network_error = Some(msg.into());
    }

    #[must_use]
    pub fn recorded_requests(&self) -> Vec<HttpRequest> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .requests
            .clone()
    }
}

impl HttpTransport for MockHttpTransport {
    fn send(&self, req: HttpRequest) -> StripeResult<HttpResponse> {
        let mut s = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        s.requests.push(req.clone());

        if let Some(err) = s.force_network_error.clone() {
            return Err(StripeError::Network(err));
        }

        let pos = s
            .program
            .iter()
            .position(|r| r.method == req.method && req.url.ends_with(&r.url_suffix));
        if let Some(idx) = pos {
            let reply = s.program.remove(idx);
            Ok(reply.response)
        } else {
            Err(StripeError::Network(format!(
                "no mock reply programmed for {} {}",
                req.method.as_str(),
                req.url
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_returns_programmed_reply() {
        let t = MockHttpTransport::new();
        t.program(ProgrammedReply {
            url_suffix: "/v1/checkout/sessions".into(),
            method: HttpMethod::Post,
            response: HttpResponse {
                status: 200,
                body: r#"{"id":"cs_test_123"}"#.into(),
            },
        });
        let resp = t
            .send(HttpRequest {
                method: HttpMethod::Post,
                url: "https://api.stripe.com/v1/checkout/sessions".into(),
                headers: BTreeMap::new(),
                body: String::new(),
            })
            .expect("programmed");
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("cs_test_123"));
    }

    #[test]
    fn mock_returns_network_error_when_unprogrammed() {
        let t = MockHttpTransport::new();
        let err = t
            .send(HttpRequest {
                method: HttpMethod::Post,
                url: "https://api.stripe.com/v1/refunds".into(),
                headers: BTreeMap::new(),
                body: String::new(),
            })
            .expect_err("must error");
        assert!(matches!(err, StripeError::Network(_)));
    }

    #[test]
    fn mock_force_network_error_path() {
        let t = MockHttpTransport::new();
        t.force_network_error("simulated outage");
        let err = t
            .send(HttpRequest {
                method: HttpMethod::Get,
                url: "https://api.stripe.com/v1/customers".into(),
                headers: BTreeMap::new(),
                body: String::new(),
            })
            .expect_err("must error");
        match err {
            StripeError::Network(m) => assert!(m.contains("simulated outage")),
            other => panic!("wrong variant: {other:?}"),
        }
    }
}
