use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::{Host, Url};
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, String>;
const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PROMPT_BYTES: usize = 256 * 1024;

#[derive(Clone)]
pub struct LocalConfig {
    pub endpoint: Url,
    pub state_dir: PathBuf,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    endpoint: Option<String>,
    state_dir: Option<PathBuf>,
}

impl LocalConfig {
    pub fn load() -> Result<Self> {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or("The local application data directory is unavailable.")?
            .join("Apocky").join("Apocrypha");
        let explicit = std::env::var_os("APOCRYPHA_DESKTOP_CONFIG");
        let path = explicit.as_ref().map(PathBuf::from).unwrap_or_else(|| base.join("local-work.json"));
        validate_local_path(&path)?;
        let file: ConfigFile = match File::open(&path) {
            Ok(file) => serde_json::from_slice(&read_limited(file, 16 * 1024)?)
                .map_err(|_| "The native local-work configuration is invalid.".to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && explicit.is_none() => ConfigFile::default(),
            Err(_) => return Err("The native local-work configuration could not be read.".into()),
        };
        let endpoint = std::env::var("APOCRYPHA_WORK_URL").ok()
            .or(file.endpoint).unwrap_or_else(|| "http://127.0.0.1:19130".into());
        let state_dir = std::env::var_os("APOCRYPHA_WORK_STATE_DIR").map(PathBuf::from)
            .or(file.state_dir).unwrap_or_else(|| default_state_dir(&base, cfg!(debug_assertions)));
        validate_local_path(&state_dir)?;
        Ok(Self { endpoint: loopback_endpoint(&endpoint)?, state_dir })
    }

    pub fn identity(&self) -> Value {
        json!({
            "connection_id": format!("{}|{}", self.endpoint, self.state_dir.display()),
            "service": "apocrypha-work", "endpoint": self.endpoint.as_str(),
            "state_dir": self.state_dir.to_string_lossy(),
        })
    }
}

fn default_state_dir(base: &Path, development: bool) -> PathBuf {
    if development { PathBuf::from("C:/Apocrypha/work") } else { base.join("work") }
}

fn validate_local_path(path: &Path) -> Result<()> {
    if !path.is_absolute() { return Err("The Work configuration and state directory must use absolute local paths.".into()); }
    #[cfg(windows)]
    if !matches!(path.components().next(), Some(std::path::Component::Prefix(prefix)) if matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_))) {
        return Err("Network shares and device paths are not allowed for local Work configuration or credentials.".into());
    }
    Ok(())
}

fn loopback_endpoint(raw: &str) -> Result<Url> {
    let url = Url::parse(raw).map_err(|_| "The Work endpoint must be a numeric loopback HTTP URL.".to_string())?;
    let loopback = match url.host() {
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        _ => false,
    };
    if url.scheme() != "http" || !loopback || !url.username().is_empty() || url.password().is_some()
        || url.query().is_some() || url.fragment().is_some() || url.path() != "/" || url.port() == Some(0)
    {
        return Err("Only a numeric loopback HTTP origin without credentials, a path, or a query is allowed.".into());
    }
    Ok(url)
}

pub fn session_id(raw: &str) -> Result<Uuid> {
    let id = Uuid::parse_str(raw).map_err(|_| "Select a valid local task.".to_string())?;
    if id.hyphenated().to_string() != raw.to_ascii_lowercase() {
        return Err("Select a valid local task.".into());
    }
    Ok(id)
}

fn read_limited(reader: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes)
        .map_err(|_| "The local response could not be read. Check task history before retrying a send.".to_string())?;
    if bytes.len() as u64 > limit { return Err("The local response exceeded the supported size.".into()); }
    Ok(bytes)
}

pub(crate) fn redact(value: &mut Value, token: &str) {
    match value {
        Value::String(text) => { *text = text.replace(token, "[redacted]"); }
        Value::Array(items) => items.iter_mut().for_each(|item| redact(item, token)),
        Value::Object(fields) => {
            let original = std::mem::take(fields);
            for (key, mut item) in original {
                if matches!(key.to_ascii_lowercase().as_str(), "token" | "access_token" | "refresh_token" | "authorization") { continue; }
                redact(&mut item, token);
                fields.insert(key.replace(token, "[redacted]"), item);
            }
        }
        _ => {}
    }
}

#[derive(Clone)]
pub struct LocalClient {
    pub config: LocalConfig,
    agent: ureq::Agent,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SendOptions {
    pub prompt: String,
    pub preset: Option<String>,
    pub sampling: Option<Value>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentDecision { Allow, AllowSession, Deny }

impl LocalClient {
    pub fn new(config: LocalConfig) -> Self {
        Self {
            config,
            agent: ureq::AgentBuilder::new().redirects(0).try_proxy_from_env(false)
                .timeout_connect(Duration::from_secs(3)).timeout_read(Duration::from_secs(20))
                .timeout_write(Duration::from_secs(5)).build(),
        }
    }

    fn token(&self) -> Result<String> {
        let file = File::open(self.config.state_dir.join("work.token"))
            .map_err(|_| "The local host token is unavailable. Start the Work host with the same state directory, then reconnect.".to_string())?;
        let bytes = read_limited(file, 4096)?;
        let value = std::str::from_utf8(&bytes).map_err(|_| "The local host token file is invalid.".to_string())?.trim();
        if value.len() < 16 || !value.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err("The local host token file is invalid.".into());
        }
        Ok(value.to_string())
    }

    fn request(&self, method: &str, path: &str, body: Option<Value>, health: bool) -> Result<Value> {
        let token = self.token()?;
        let endpoint = self.config.endpoint.join(path).map_err(|_| "The local route is invalid.".to_string())?;
        let request = self.agent.request_url(method, &endpoint).timeout(Duration::from_secs(20))
            .set("Authorization", &format!("Bearer {token}")).set("Accept", "application/json");
        let response = match body {
            Some(body) => request.send_json(body),
            None => request.call(),
        };
        let response = match response {
            Ok(response) => response,
            Err(ureq::Error::Status(503, response)) if health => response,
            Err(ureq::Error::Status(status, _)) => return Err(http_error(status)),
            Err(_) => return Err("The local Work host is offline or timed out. A submitted task may still be running; check history before sending again.".into()),
        };
        if !(200..300).contains(&response.status()) && !(health && response.status() == 503) {
            return Err(http_error(response.status()));
        }
        if !response.header("Content-Type").unwrap_or_default().to_ascii_lowercase().starts_with("application/json") {
            return Err("The local host returned an unsupported response format.".into());
        }
        let mut value: Value = serde_json::from_slice(&read_limited(response.into_reader(), MAX_JSON_BYTES)?)
            .map_err(|_| "The local host returned invalid JSON.".to_string())?;
        redact(&mut value, &token);
        Ok(value)
    }

    pub fn bootstrap(&self) -> Value {
        let mut view = json!({ "online": false, "host": self.config.identity(), "health": null, "workspace": null, "sessions": [], "notice": "" });
        let loaded = (|| -> Result<(Value, Value, Value)> {
            let health = self.request("GET", "/health", None, true)?;
            if health["service"] != "apocrypha-work" { return Err("The configured port is not an Apocrypha Work host.".into()); }
            let workspace = self.request("GET", "/workspace", None, false)?;
            let sessions = self.request("GET", "/sessions", None, false)?;
            validate_bootstrap(&health, &workspace, &sessions)?;
            Ok((health, workspace, sessions["sessions"].clone()))
        })();
        match loaded {
            Ok((health, workspace, sessions)) => {
                view["online"] = json!(true);
                view["health"] = health;
                view["workspace"] = workspace;
                view["sessions"] = sessions;
            }
            Err(error) => view["notice"] = json!(error),
        }
        view
    }

    pub fn open(&self, id: Uuid) -> Result<Value> {
        let value = self.request("GET", &format!("/sessions/{id}"), None, false)?;
        if value["session"]["id"].as_str() != Some(id.to_string().as_str()) || !valid_session(&value["session"])
            || !value["turns"].as_array().is_some_and(|turns| turns.iter().all(|turn| valid_turn(turn, id)))
        {
            return Err("The host returned a different or invalid task.".into());
        }
        Ok(value)
    }

    pub fn create(&self, title: &str) -> Result<Uuid> {
        let title = validate_title(title)?;
        let value = self.request("POST", "/sessions", Some(json!({ "title": title })), false)?;
        session_id(value["session"]["id"].as_str().unwrap_or_default())
    }

    pub fn send(&self, id: Uuid, options: SendOptions) -> Result<Value> {
        let prompt = options.prompt.trim();
        if prompt.is_empty() || prompt.len() > MAX_PROMPT_BYTES {
            return Err("Enter a task of at most 256 KB.".into());
        }
        if options.preset.as_ref().is_some_and(|preset| preset.is_empty() || preset.len() > 64 || !preset.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')) {
            return Err("Select a valid sampling preset.".into());
        }
        if let Some(sampling) = &options.sampling {
            if !sampling.is_object() || sampling.to_string().len() > 16 * 1024 {
                return Err("The sampling settings are invalid or too large.".into());
            }
        }
        let value = self.request("POST", &format!("/sessions/{id}/turns"), Some(json!({
            "prompt": prompt, "preset": options.preset, "sampling": options.sampling,
        })), false)?;
        session_id(value["turn_id"].as_str().unwrap_or_default())?;
        Ok(value)
    }

    pub fn cancel(&self, id: Uuid) -> Result<Value> {
        self.request("POST", &format!("/sessions/{id}/cancel"), Some(json!({})), false)
    }

    pub fn stream(&self, id: Uuid, after: u64) -> Result<(ureq::Response, String)> {
        let token = self.token()?;
        let mut endpoint = self.config.endpoint.join(&format!("/sessions/{id}/stream"))
            .map_err(|_| "The local stream route is invalid.".to_string())?;
        endpoint.query_pairs_mut().append_pair("after", &after.to_string());
        let response = self.agent.request_url("GET", &endpoint)
            .set("Authorization", &format!("Bearer {token}"))
            .set("Accept", "text/event-stream").set("Last-Event-ID", &after.to_string()).call();
        let response = match response {
            Ok(response) => response,
            Err(ureq::Error::Status(status, _)) => return Err(http_error(status)),
            Err(_) => return Err("The local event stream disconnected. Reconnecting without resending the task.".into()),
        };
        if response.status() != 200 || !response.header("Content-Type").unwrap_or_default().to_ascii_lowercase().starts_with("text/event-stream") {
            return Err("The local host did not return a supported event stream.".into());
        }
        Ok((response, token))
    }

    pub fn consent(&self, id: Uuid, decision: ConsentDecision) -> Result<Value> {
        self.request("POST", &format!("/consent/{id}"), Some(json!({ "decision": decision })), false)
    }

    pub fn rename(&self, id: Uuid, title: &str) -> Result<Value> {
        self.request("POST", &format!("/sessions/{id}/title"), Some(json!({ "title": validate_title(title)? })), false)
    }
}

fn validate_title(title: &str) -> Result<&str> {
    let title = title.trim();
    if title.is_empty() || title.len() > 256 { return Err("Enter a task title of at most 256 bytes.".into()); }
    Ok(title)
}

fn string_fields(value: &Value, keys: &[&str]) -> bool {
    keys.iter().all(|key| value[*key].is_string())
}

fn valid_session(value: &Value) -> bool {
    string_fields(value, &["id", "title", "createdAt", "lastActiveAt"])
        && session_id(value["id"].as_str().unwrap_or_default()).is_ok()
        && value["standingGrants"].as_array().is_some_and(|grants| grants.iter().all(Value::is_string))
}

fn valid_turn(value: &Value, id: Uuid) -> bool {
    string_fields(value, &["id", "sessionId", "prompt", "phase", "startedAt", "output"])
        && session_id(value["id"].as_str().unwrap_or_default()).is_ok()
        && value["sessionId"].as_str() == Some(id.to_string().as_str())
        && matches!(value["phase"].as_str(), Some("queued" | "thinking" | "awaiting_consent" | "tool" | "writing" | "done" | "failed" | "cancelled"))
        && value["toolCalls"].as_array().is_some_and(|tools| tools.iter().all(valid_tool))
}

pub(crate) fn valid_tool(value: &Value) -> bool {
    string_fields(value, &["id", "name", "summary", "content"]) && value["ok"].is_boolean()
        && value["elapsedMs"].is_number()
        && (value.get("diff").is_none() || (string_fields(&value["diff"], &["path", "patch"])
            && value["diff"]["added"].is_number() && value["diff"]["removed"].is_number()))
}

fn validate_bootstrap(health: &Value, workspace: &Value, sessions: &Value) -> Result<()> {
    let valid = health["status"].is_string() && health["engine"].is_object()
        && health["policy"].is_object() && health["mcp"].is_object() && health["sampling"].is_object()
        && health["presets"].as_array().is_some_and(|items| items.iter().all(|item| string_fields(item, &["id", "label"]) && item["profile"].is_object()))
        && workspace["roots"].as_array().is_some_and(|roots| roots.iter().all(|root| string_fields(root, &["label", "path"]) && root["writable"].is_boolean()))
        && workspace["tools"].as_array().is_some_and(|tools| tools.iter().all(|tool| string_fields(tool, &["name", "risk", "description"])))
        && sessions["sessions"].as_array().is_some_and(|items| items.iter().all(valid_session));
    if !valid { return Err("The local host returned an unsupported health, workspace, or task record.".into()); }
    Ok(())
}

fn http_error(status: u16) -> String {
    match status {
        300..=399 => "The local host tried to redirect a private request. Redirects are blocked.",
        401 | 403 => "The local host refused authentication. Check its state directory and reconnect.",
        404 => "That task or approval is no longer available. Reload the task before acting.",
        409 => "The host could not accept this action in the current task state. Reload the task before retrying.",
        400 | 413 | 422 => "The host rejected this task input. Review the request before retrying.",
        _ => "The local host could not confirm this action. Check task history before retrying.",
    }.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;

    const TOKEN: &str = "fixture-private-local-token-not-a-real-credential";

    struct Fixture { client: LocalClient, root: PathBuf }

    impl Fixture {
        fn new(endpoint: &str) -> Self {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("local-fixtures").join(Uuid::new_v4().to_string());
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("work.token"), TOKEN).unwrap();
            Self { client: LocalClient::new(LocalConfig { endpoint: loopback_endpoint(endpoint).unwrap(), state_dir: root.clone() }), root }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.root); }
    }

    #[test]
    fn local_endpoints_fail_closed_on_external_credentials_and_paths() {
        assert!(loopback_endpoint("http://127.0.0.1:19130").is_ok());
        assert!(loopback_endpoint("http://[::1]:19130").is_ok());
        for invalid in ["http://example.com", "http://localhost:19130", "http://192.168.0.2:19130", "https://127.0.0.1", "http://user:secret@127.0.0.1", "http://127.0.0.1/?token=x", "http://127.0.0.1/other", "http://127.0.0.1/#fragment"] {
            assert!(loopback_endpoint(invalid).is_err(), "rejected endpoint: {invalid}");
        }
        assert!(session_id("../../engine/acquire").is_err());
        assert!(session_id(&Uuid::new_v4().to_string()).is_ok());
    }

    #[test]
    fn installed_state_default_is_per_user_not_the_development_drive() {
        let base = PathBuf::from("C:/Users/fixture/AppData/Local/Apocky/Apocrypha");
        assert_eq!(default_state_dir(&base, false), base.join("work"));
        assert_ne!(default_state_dir(&base, false), default_state_dir(&base, true));
        assert!(validate_local_path(&base).is_ok());
        assert!(validate_local_path(Path::new("relative/state")).is_err());
        #[cfg(windows)]
        assert!(validate_local_path(Path::new(r"\\server\share\work")).is_err());
    }

    #[test]
    fn malformed_tool_outcomes_and_secret_object_keys_are_not_forwarded() {
        let mut value = json!({ "id": "tool-1", "name": "file_edit", "ok": false, "elapsedMs": 1, "summary": "failed", "content": "exit 1" });
        assert!(valid_tool(&value));
        value["ok"] = json!("true");
        assert!(!valid_tool(&value));
        let mut secret = json!({ TOKEN: { "authorization": TOKEN, "detail": TOKEN } });
        redact(&mut secret, TOKEN);
        assert!(!secret.to_string().contains(TOKEN));
        assert_eq!(secret["[redacted]"]["detail"], "[redacted]");
    }

    #[test]
    fn missing_token_is_offline_and_private_material_never_enters_a_view() {
        let fixture = Fixture::new("http://127.0.0.1:1");
        std::fs::remove_file(fixture.root.join("work.token")).unwrap();
        let view = fixture.client.bootstrap();
        assert_eq!(view["online"], false);
        assert!(view["health"].is_null());
        assert!(view["notice"].as_str().unwrap().contains("token is unavailable"));
        assert!(!view.to_string().contains(TOKEN));
    }

    #[test]
    fn actual_http_uses_bearer_header_and_redacts_an_echoed_token() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let fixture = Fixture::new(&format!("http://{}", listener.local_addr().unwrap()));
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut request = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" { break; }
                request.push_str(&line);
            }
            assert!(request.starts_with("GET /health HTTP/1.1"));
            assert!(!request.lines().next().unwrap().contains(TOKEN));
            assert!(request.contains(&format!("Authorization: Bearer {TOKEN}")));
            let body = json!({ "service": "apocrypha-work", "status": "degraded", "token": TOKEN, "nested": { "detail": format!("echo {TOKEN}") } }).to_string();
            write!(socket, "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        });
        let response = fixture.client.request("GET", "/health", None, true).unwrap();
        server.join().unwrap();
        assert_eq!(response["status"], "degraded");
        assert_eq!(response["nested"]["detail"], "echo [redacted]");
        assert!(!response.to_string().contains(TOKEN));
    }

    #[test]
    fn redirect_response_cannot_forward_the_bearer_to_another_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let trap = TcpListener::bind("127.0.0.1:0").unwrap();
        trap.set_nonblocking(true).unwrap();
        let destination = trap.local_addr().unwrap();
        let fixture = Fixture::new(&format!("http://{}", listener.local_addr().unwrap()));
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" { break; }
            }
            write!(socket, "HTTP/1.1 302 Found\r\nLocation: http://{destination}/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let error = fixture.client.request("GET", "/health", None, true).unwrap_err();
        server.join().unwrap();
        assert!(error.contains("Redirects are blocked"));
        assert!(trap.accept().is_err());
        assert!(!error.contains(TOKEN));
    }
}