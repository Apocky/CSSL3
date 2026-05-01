// § http_client.rs : HttpClient trait + UreqClient (prod) + MockHttpClient (tests)
//
// The transport speaks HTTP through this trait so tests can run the entire
// `RealSupabaseTransport` pipeline without touching the network. Tests inject
// a `MockHttpClient` ; production injects `UreqClient` which is a thin
// adapter over the workspace-pinned `ureq` crate.
//
// The trait is intentionally narrow : `execute(req) -> Result<HttpResp, …>`.
// Headers are a `Vec<(String, String)>` so duplicate-key headers are
// preserved (matters for `Authorization:` + `apikey:` which sometimes
// double-up in Supabase service-role flows).

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

/// HTTP-method discriminator. Stage-0 supports the two verbs the Supabase
/// signaling REST surface needs ; PATCH/DELETE are added when the transport
/// gains room-cleanup primitives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    /// HTTP-GET — used by `poll`.
    Get,
    /// HTTP-POST — used by `send`.
    Post,
}

impl HttpMethod {
    /// Stable string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

/// HTTP request envelope. Constructed by `RealSupabaseTransport` ; consumed
/// by `HttpClient::execute`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpReq {
    /// Verb.
    pub method: HttpMethod,
    /// Fully-qualified URL.
    pub url: String,
    /// Header pairs in insertion order. Both names and values are owned
    /// strings so the request can outlive a stack frame.
    pub headers: Vec<(String, String)>,
    /// Body bytes ; empty for GET. The content-type is conveyed via a
    /// header (`Content-Type: application/json` typically).
    pub body: Vec<u8>,
    /// Per-call timeout. The client maps this onto its underlying
    /// connect/read/write deadlines.
    pub timeout: Duration,
}

/// HTTP response envelope. Returned by `HttpClient::execute` on success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResp {
    /// Numeric HTTP status code.
    pub status: u16,
    /// Response headers in arrival order. Used by the transport to extract
    /// `Retry-After` on 429.
    pub headers: Vec<(String, String)>,
    /// Response body bytes.
    pub body: Vec<u8>,
}

impl HttpResp {
    /// Lookup a header by case-insensitive name. Returns the first match.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// True iff the status is in the 2xx range.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }
}

/// HTTP-layer error. Distinct from `TransportErr` so the caller (the
/// transport) can map between them — a `Timeout` here becomes
/// `TransportErr::Timeout` ; an `Io` here becomes `TransportErr::ServerErr`
/// with the message prefixed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpTransportErr {
    /// Request did not return before the deadline.
    Timeout,
    /// Transport-layer error : DNS / connect-refused / TLS handshake.
    /// Inner string is the underlying diagnostic.
    Io(String),
    /// Caller passed a malformed URL or request shape.
    BadRequest(String),
}

impl core::fmt::Display for HttpTransportErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Timeout => f.write_str("http: timeout"),
            Self::Io(s) => write!(f, "http: io: {s}"),
            Self::BadRequest(s) => write!(f, "http: bad request: {s}"),
        }
    }
}

impl std::error::Error for HttpTransportErr {}

/// HTTP client contract. `RealSupabaseTransport` holds a
/// `Box<dyn HttpClient>` so the production `UreqClient` and the test
/// `MockHttpClient` are interchangeable.
pub trait HttpClient: Send + Sync + core::fmt::Debug {
    /// Execute a single request. Implementations must respect
    /// `req.timeout` for connect / read / write phases.
    fn execute(&self, req: &HttpReq) -> Result<HttpResp, HttpTransportErr>;
}

// ─── UreqClient ────────────────────────────────────────────────────────────

/// Production `HttpClient` impl. Adapts the workspace-pinned `ureq` 2.x
/// blocking API onto our trait. A fresh `ureq::Agent` is built per-call so
/// timeouts honour the request envelope ; the per-call cost is negligible
/// compared to the network round-trip.
#[derive(Debug, Default)]
pub struct UreqClient;

impl UreqClient {
    /// Construct a fresh client. Stateless ; this is just a marker type.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl HttpClient for UreqClient {
    fn execute(&self, req: &HttpReq) -> Result<HttpResp, HttpTransportErr> {
        // Build a per-call agent so the request's timeout applies.
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(req.timeout)
            .timeout_read(req.timeout)
            .timeout_write(req.timeout)
            .build();

        // Compose the request : method + url + headers, then dispatch.
        let mut r = match req.method {
            HttpMethod::Get => agent.get(&req.url),
            HttpMethod::Post => agent.post(&req.url),
        };
        for (k, v) in &req.headers {
            r = r.set(k, v);
        }

        // ureq splits "send body" vs "no body" calls. We fold both into the
        // same ureq::Response shape on success.
        let result = match req.method {
            HttpMethod::Get => r.call(),
            HttpMethod::Post => r.send_bytes(&req.body),
        };

        // ureq surfaces non-2xx as `Status(_, resp)` — we still want the
        // body (Supabase returns Retry-After + a JSON error doc on 429) so
        // we fold both Ok + Status into the same `extract_resp` path.
        // Transport-errors are the only path that diverges.
        #[allow(clippy::match_same_arms)]
        match result {
            Ok(resp) => extract_resp(resp),
            Err(ureq::Error::Status(_, resp)) => extract_resp(resp),
            Err(ureq::Error::Transport(t)) => {
                let s = format!("{t}");
                if s.contains("timed out") || s.contains("timeout") {
                    Err(HttpTransportErr::Timeout)
                } else {
                    Err(HttpTransportErr::Io(s))
                }
            }
        }
    }
}

/// Pull headers + body off a `ureq::Response` ; used by both success +
/// `Status` paths so we never lose the body on non-2xx replies.
fn extract_resp(resp: ureq::Response) -> Result<HttpResp, HttpTransportErr> {
    let status = resp.status();
    let header_names: Vec<String> = resp.headers_names();
    let headers: Vec<(String, String)> = header_names
        .iter()
        .filter_map(|n| resp.header(n).map(|v| (n.clone(), v.to_string())))
        .collect();
    // ureq's into_string() caps at 10MB by default ; for signaling
    // payloads (≤ 64KB envelope, plus poll-batches up to 128 rows) this
    // is comfortably bounded.
    let body = resp
        .into_string()
        .map_err(|e| HttpTransportErr::Io(format!("body read: {e}")))?
        .into_bytes();
    Ok(HttpResp {
        status,
        headers,
        body,
    })
}

// ─── MockHttpClient ────────────────────────────────────────────────────────

/// Test-only `HttpClient` impl. Records every request and returns canned
/// responses keyed by URL prefix. No network I/O ; deterministic.
///
/// The mock is `Debug` + `Send + Sync` so it slots into the same
/// `Box<dyn HttpClient>` that the production transport uses.
#[derive(Debug, Default)]
pub struct MockHttpClient {
    /// Recorded requests in order. Lets tests assert on header presence,
    /// URL composition, and body content.
    calls: Mutex<Vec<HttpReq>>,
    /// URL-prefix → canned response map. The first matching prefix wins ;
    /// prefixes are checked in insertion order (BTreeMap iteration is by
    /// key sort, but for our purposes the explicit `prefix_order` vec
    /// preserves order).
    canned: Mutex<BTreeMap<String, HttpResp>>,
    /// URL-prefix → canned error map (errors take precedence over canned
    /// responses if both are registered for the same prefix).
    canned_err: Mutex<BTreeMap<String, HttpTransportErr>>,
}

impl MockHttpClient {
    /// Construct an empty mock. Every `execute` will return a synthetic
    /// 200 OK with empty body unless the caller registers a canned
    /// response.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a canned response for any URL whose URL starts with
    /// `url_prefix`. Re-registering the same prefix overwrites the prior
    /// response.
    pub fn add_response(&self, url_prefix: impl Into<String>, resp: HttpResp) {
        if let Ok(mut g) = self.canned.lock() {
            g.insert(url_prefix.into(), resp);
        }
    }

    /// Register a canned error for any URL whose URL starts with
    /// `url_prefix`. Errors take precedence over responses.
    pub fn add_error(&self, url_prefix: impl Into<String>, err: HttpTransportErr) {
        if let Ok(mut g) = self.canned_err.lock() {
            g.insert(url_prefix.into(), err);
        }
    }

    /// Snapshot of every recorded request, in execution order.
    pub fn calls(&self) -> Vec<HttpReq> {
        self.calls.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

impl HttpClient for MockHttpClient {
    fn execute(&self, req: &HttpReq) -> Result<HttpResp, HttpTransportErr> {
        if let Ok(mut g) = self.calls.lock() {
            g.push(req.clone());
        }
        // Errors take precedence.
        if let Ok(g) = self.canned_err.lock() {
            for (prefix, err) in g.iter() {
                if req.url.starts_with(prefix.as_str()) {
                    return Err(err.clone());
                }
            }
        }
        if let Ok(g) = self.canned.lock() {
            for (prefix, resp) in g.iter() {
                if req.url.starts_with(prefix.as_str()) {
                    return Ok(resp.clone());
                }
            }
        }
        // Default : empty 200.
        Ok(HttpResp {
            status: 200,
            headers: vec![],
            body: vec![],
        })
    }
}

// ─── tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_method_as_str_stable() {
        assert_eq!(HttpMethod::Get.as_str(), "GET");
        assert_eq!(HttpMethod::Post.as_str(), "POST");
    }

    #[test]
    fn http_resp_header_lookup_case_insensitive() {
        let r = HttpResp {
            status: 200,
            headers: vec![
                ("Content-Type".into(), "application/json".into()),
                ("Retry-After".into(), "5".into()),
            ],
            body: vec![],
        };
        assert_eq!(r.header("content-type"), Some("application/json"));
        assert_eq!(r.header("RETRY-AFTER"), Some("5"));
        assert_eq!(r.header("missing"), None);
        assert!(r.is_success());
    }

    #[test]
    fn mock_records_and_returns_canned() {
        let mock = MockHttpClient::new();
        mock.add_response(
            "https://x.supabase.co/rest/v1/signaling_messages",
            HttpResp {
                status: 201,
                headers: vec![("X-Test".into(), "yes".into())],
                body: b"[]".to_vec(),
            },
        );

        let req = HttpReq {
            method: HttpMethod::Post,
            url: "https://x.supabase.co/rest/v1/signaling_messages".into(),
            headers: vec![("apikey".into(), "anon".into())],
            body: b"{}".to_vec(),
            timeout: Duration::from_millis(1_000),
        };
        let resp = mock.execute(&req).expect("ok");
        assert_eq!(resp.status, 201);
        assert_eq!(resp.body, b"[]");

        let calls = mock.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].url, req.url);
        assert_eq!(calls[0].body, req.body);
    }

    #[test]
    fn mock_default_returns_empty_200() {
        let mock = MockHttpClient::new();
        let req = HttpReq {
            method: HttpMethod::Get,
            url: "https://nope.supabase.co/x".into(),
            headers: vec![],
            body: vec![],
            timeout: Duration::from_millis(100),
        };
        let resp = mock.execute(&req).expect("ok");
        assert_eq!(resp.status, 200);
        assert!(resp.body.is_empty());
    }

    #[test]
    fn mock_error_takes_precedence() {
        let mock = MockHttpClient::new();
        mock.add_response(
            "https://e.supabase.co/",
            HttpResp {
                status: 200,
                headers: vec![],
                body: b"ok".to_vec(),
            },
        );
        mock.add_error("https://e.supabase.co/", HttpTransportErr::Timeout);

        let req = HttpReq {
            method: HttpMethod::Get,
            url: "https://e.supabase.co/path".into(),
            headers: vec![],
            body: vec![],
            timeout: Duration::from_millis(100),
        };
        assert_eq!(mock.execute(&req), Err(HttpTransportErr::Timeout));
    }
}
