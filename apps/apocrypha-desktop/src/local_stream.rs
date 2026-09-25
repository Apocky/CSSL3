use std::collections::{hash_map::DefaultHasher, HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::local::{redact, session_id, valid_tool, ConsentDecision, LocalClient, LocalConfig, Result, SendOptions};

pub const WORK_EVENT: &str = "apocrypha://work-event";
pub const WORK_STREAM_EVENT: &str = "apocrypha://work-stream";
const MAX_FRAME_BYTES: usize = 2 * 1024 * 1024;
type Emit = Arc<dyn Fn(&str, Value) + Send + Sync>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkEvent {
    pub seq: u64,
    pub at: String,
    pub kind: String,
    pub data: Map<String, Value>,
}

#[derive(Default)]
struct SseDecoder { data: String }

impl SseDecoder {
    fn line(&mut self, line: &str) -> Result<Option<WorkEvent>> {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            if self.data.is_empty() { return Ok(None); }
            let data = std::mem::take(&mut self.data);
            let event: WorkEvent = serde_json::from_str(&data)
                .map_err(|_| "The host sent an invalid event. Reload the task to recover.".to_string())?;
            if event.seq == 0 || event.seq > 9_007_199_254_740_991 || event.at.is_empty()
                || !matches!(event.kind.as_str(), "session" | "phase" | "token" | "tool_request" | "tool_result" | "consent_request" | "consent_resolved" | "error" | "usage")
            {
                return Err("The host sent an unsupported event. Reload the task to recover.".into());
            }
            if !valid_event_data(&event) { return Err("The host sent an invalid task event. Reload the task to recover.".into()); }
            return Ok(Some(event));
        }
        if let Some(data) = line.strip_prefix("data:") {
            let data = data.strip_prefix(' ').unwrap_or(data);
            if self.data.len() + data.len() + 1 > MAX_FRAME_BYTES { return Err("The host event exceeded the supported size. Reload the task.".into()); }
            self.data.push_str(data);
            self.data.push('\n');
        }
        Ok(None)
    }
}

fn valid_event_data(event: &WorkEvent) -> bool {
    let data = &event.data;
    let strings = |keys: &[&str]| keys.iter().all(|key| data.get(*key).is_some_and(Value::is_string));
    match event.kind.as_str() {
        "session" => strings(&["turn_id", "prompt"]) && data.get("turn_id").and_then(Value::as_str).is_some_and(|id| session_id(id).is_ok()),
        "token" => strings(&["delta"]),
        "phase" => matches!(data.get("phase").and_then(Value::as_str), Some("queued" | "thinking" | "awaiting_consent" | "tool" | "writing" | "done" | "failed" | "cancelled")),
        "tool_request" => strings(&["id", "name"]),
        "tool_result" => valid_tool(&Value::Object(data.clone())),
        "consent_request" => strings(&["id", "tool", "risk", "summary", "detail"]) && data.get("id").and_then(Value::as_str).is_some_and(|id| session_id(id).is_ok()),
        "consent_resolved" => strings(&["id", "decision"]),
        "error" => strings(&["message"]),
        "usage" => true,
        _ => false,
    }
}

#[derive(Default)]
struct EventCursor {
    last: u64,
    fingerprints: HashMap<u64, u64>,
    order: VecDeque<u64>,
}

impl EventCursor {
    fn accept(&mut self, event: &WorkEvent) -> Result<bool> {
        let mut hasher = DefaultHasher::new();
        serde_json::to_string(event).map_err(|_| "The host event could not be decoded.".to_string())?.hash(&mut hasher);
        let fingerprint = hasher.finish();
        if event.seq <= self.last {
            if self.fingerprints.get(&event.seq) == Some(&fingerprint) { return Ok(false); }
            return Err("The host event history changed or reset. Reload this task before continuing.".into());
        }
        if event.seq != self.last + 1 {
            return Err("There is a gap in the host event history. Saved turns remain available; reload after the host restores replay.".into());
        }
        self.last = event.seq;
        self.fingerprints.insert(event.seq, fingerprint);
        self.order.push_back(event.seq);
        if self.order.len() > 512 {
            if let Some(seq) = self.order.pop_front() { self.fingerprints.remove(&seq); }
        }
        Ok(true)
    }
}

struct Selection {
    id: Uuid,
    epoch: u64,
    stop: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    pending: Arc<Mutex<HashSet<Uuid>>>,
    interrupt: Option<mpsc::Sender<()>>,
}

pub struct LocalController {
    client: Result<LocalClient>,
    epoch: Arc<AtomicU64>,
    selection: Mutex<Option<Selection>>,
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalRequest {
    Subscribe { session_id: String, epoch: u64 },
    Detach { epoch: u64 },
    Rename { session_id: String, title: String },
}

impl LocalController {
    pub fn new() -> Self {
        Self { client: LocalConfig::load().map(LocalClient::new), epoch: Arc::new(AtomicU64::new(0)), selection: Mutex::new(None) }
    }

    fn client(&self) -> Result<&LocalClient> { self.client.as_ref().map_err(Clone::clone) }

    pub fn bootstrap(&self) -> Value {
        match &self.client {
            Ok(client) => client.bootstrap(),
            Err(error) => json!({ "online": false, "host": null, "health": null, "workspace": null, "sessions": [], "notice": error }),
        }
    }

    pub fn open(&self, id: Uuid) -> Result<Value> {
        let epoch = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        self.stop_selection(None)?;
        let mut snapshot = self.client()?.open(id)?;
        let mut selected = self.selection.lock().map_err(|_| "The task selection is unavailable.".to_string())?;
        if self.epoch.load(Ordering::SeqCst) != epoch { return Err("A newer task selection replaced this request.".into()); }
        *selected = Some(Selection {
            id, epoch, stop: Arc::new(AtomicBool::new(false)), connected: Arc::new(AtomicBool::new(false)),
            pending: Arc::new(Mutex::new(HashSet::new())), interrupt: None,
        });
        snapshot["epoch"] = json!(epoch);
        snapshot["after_seq"] = json!(0);
        Ok(snapshot)
    }

    pub fn create(&self, title: &str) -> Result<Value> {
        self.open(self.client()?.create(title)?)
    }

    pub fn send(&self, id: Uuid, options: SendOptions) -> Result<Value> { self.client()?.send(id, options) }
    pub fn cancel(&self, id: Uuid) -> Result<Value> { self.client()?.cancel(id) }

    pub fn consent(&self, id: Uuid, epoch: u64, request: Uuid, decision: ConsentDecision) -> Result<Value> {
        let pending = {
            let selected = self.selection.lock().map_err(|_| "The task selection is unavailable.".to_string())?;
            let selection = selected.as_ref().filter(|selection| selection.id == id && selection.epoch == epoch && selection.connected.load(Ordering::SeqCst))
                .ok_or("Reload the current task before deciding an approval.")?;
            selection.pending.clone()
        };
        if !pending.lock().map_err(|_| "Approval state is unavailable.".to_string())?.contains(&request) {
            return Err("That approval is no longer pending in the current task.".into());
        }
        let response = self.client()?.consent(request, decision)?;
        if response["resolved"] == true { pending.lock().map_err(|_| "Approval state is unavailable.".to_string())?.remove(&request); }
        Ok(response)
    }

    pub fn request(&self, request: LocalRequest, emit: Emit) -> Result<Value> {
        match request {
            LocalRequest::Subscribe { session_id: id, epoch } => { self.subscribe(session_id(&id)?, epoch, emit)?; Ok(json!({ "subscribed": true })) }
            LocalRequest::Detach { epoch } => { self.stop_selection(Some(epoch))?; Ok(json!({ "detached": true })) }
            LocalRequest::Rename { session_id: id, title } => self.client()?.rename(session_id(&id)?, &title),
        }
    }

    fn stop_selection(&self, expected: Option<u64>) -> Result<()> {
        let mut selected = self.selection.lock().map_err(|_| "The task selection is unavailable.".to_string())?;
        if selected.as_ref().is_some_and(|selection| expected.is_none() || expected == Some(selection.epoch)) {
            if let Some(selection) = selected.take() {
                selection.stop.store(true, Ordering::SeqCst);
                selection.connected.store(false, Ordering::SeqCst);
                if let Some(interrupt) = selection.interrupt { let _ = interrupt.send(()); }
            }
        }
        Ok(())
    }

    fn subscribe(&self, id: Uuid, epoch: u64, emit: Emit) -> Result<()> {
        let client = self.client()?.clone();
        let mut selected = self.selection.lock().map_err(|_| "The task selection is unavailable.".to_string())?;
        let selection = selected.as_mut().filter(|selection| selection.id == id && selection.epoch == epoch)
            .ok_or("That stream belongs to a previous task selection.")?;
        if selection.interrupt.is_some() { return Ok(()); }
        let (interrupt, receiver) = mpsc::channel();
        let worker = Worker {
            client, id, epoch, current_epoch: self.epoch.clone(), stop: selection.stop.clone(),
            connected: selection.connected.clone(), pending: selection.pending.clone(), emit,
        };
        std::thread::Builder::new().name("apocrypha-work-stream".into()).spawn(move || worker.run(receiver))
            .map_err(|_| "The local event reader could not start.".to_string())?;
        selection.interrupt = Some(interrupt);
        Ok(())
    }
}

impl Drop for LocalController {
    fn drop(&mut self) { let _ = self.stop_selection(None); }
}

struct Worker {
    client: LocalClient,
    id: Uuid,
    epoch: u64,
    current_epoch: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    pending: Arc<Mutex<HashSet<Uuid>>>,
    emit: Emit,
}

enum StreamExit { Retry(String), Gap(String), Detached }

impl Worker {
    fn active(&self) -> bool { !self.stop.load(Ordering::SeqCst) && self.current_epoch.load(Ordering::SeqCst) == self.epoch }

    fn status(&self, status: &str, message: &str) {
        if self.active() { (self.emit)(WORK_STREAM_EVENT, json!({ "session_id": self.id.to_string(), "epoch": self.epoch, "status": status, "message": message })); }
    }

    fn run(&self, interrupt: mpsc::Receiver<()>) {
        let mut cursor = EventCursor::default();
        let mut retries = 0;
        while self.active() {
            self.status(if retries == 0 { "connecting" } else { "reconnecting" }, "");
            let outcome = self.read(&mut cursor);
            self.connected.store(false, Ordering::SeqCst);
            match outcome {
                StreamExit::Detached => return,
                StreamExit::Gap(message) => { self.status("gap", &message); return; }
                StreamExit::Retry(message) => self.status("reconnecting", &message),
            }
            retries = (retries + 1).min(5);
            if interrupt.recv_timeout(Duration::from_millis(500 * retries)).is_ok() || !self.active() { return; }
        }
    }

    fn read(&self, cursor: &mut EventCursor) -> StreamExit {
        let (response, token) = match self.client.stream(self.id, cursor.last) {
            Ok(response) => response,
            Err(error) => return StreamExit::Retry(error),
        };
        let mut reader = BufReader::new(response.into_reader());
        let mut decoder = SseDecoder::default();
        let mut bytes = Vec::new();
        while self.active() {
            bytes.clear();
            match reader.by_ref().take(MAX_FRAME_BYTES as u64 + 1).read_until(b'\n', &mut bytes) {
                Ok(0) => return StreamExit::Retry("The event stream closed. Reconnecting without resending the task.".into()),
                Err(_) => return StreamExit::Retry("The event stream timed out. Reconnecting without resending the task.".into()),
                _ => {}
            }
            if !self.active() { return StreamExit::Detached; }
            if bytes.len() > MAX_FRAME_BYTES { return StreamExit::Gap("The host event line exceeded the supported size.".into()); }
            let line = match std::str::from_utf8(&bytes) {
                Ok(line) => line,
                Err(_) => return StreamExit::Gap("The host event stream contains invalid text.".into()),
            };
            if line.trim_end() == ": attached" {
                self.connected.store(true, Ordering::SeqCst);
                self.status("connected", "");
            }
            let event = match decoder.line(line) {
                Ok(Some(event)) => event,
                Ok(None) => continue,
                Err(error) => return StreamExit::Gap(error),
            };
            match cursor.accept(&event) {
                Ok(false) => continue,
                Err(error) => return StreamExit::Gap(error),
                Ok(true) => {}
            }
            if let Ok(mut pending) = self.pending.lock() {
                match event.kind.as_str() {
                    "consent_request" => {
                        if let Some(id) = event.data.get("id").and_then(Value::as_str).and_then(|id| session_id(id).ok()) { pending.insert(id); }
                    }
                    "consent_resolved" => {
                        if let Some(id) = event.data.get("id").and_then(Value::as_str).and_then(|id| session_id(id).ok()) { pending.remove(&id); }
                    }
                    "session" | "error" => pending.clear(),
                    "phase" if event.data.get("terminal") == Some(&Value::Bool(true)) => pending.clear(),
                    _ => {}
                }
            }
            let mut payload = json!({ "session_id": self.id.to_string(), "epoch": self.epoch, "event": event });
            redact(&mut payload, &token);
            if self.active() { (self.emit)(WORK_EVENT, payload); }
        }
        StreamExit::Detached
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;
    use std::path::PathBuf;

    fn event(seq: u64, delta: &str) -> WorkEvent {
        serde_json::from_value(json!({ "seq": seq, "at": "2026-09-16", "kind": "token", "data": { "delta": delta } })).unwrap()
    }

    #[test]
    fn sequence_gate_rejects_one_missing_changed_or_reset_event_but_accepts_exact_replay() {
        let mut cursor = EventCursor::default();
        assert!(cursor.accept(&event(1, "first")).unwrap());
        assert!(!cursor.accept(&event(1, "first")).unwrap());
        assert!(cursor.accept(&event(3, "gap")).is_err());
        assert_eq!(cursor.last, 1);
        assert!(cursor.accept(&event(2, "second")).unwrap());
        assert!(cursor.accept(&event(1, "new host epoch")).is_err());
        assert_eq!(cursor.last, 2);
    }

    #[test]
    fn sse_handles_crlf_comments_multiline_and_rejects_malformed_or_unbounded_frames() {
        let mut decoder = SseDecoder::default();
        assert!(decoder.line(": ping\r\n").unwrap().is_none());
        assert!(decoder.line("data: {\"seq\":1,\"at\":\"now\",\r\n").unwrap().is_none());
        assert!(decoder.line("data: \"kind\":\"token\",\"data\":{\"delta\":\"hello\"}}\r\n").unwrap().is_none());
        let decoded = decoder.line("\r\n").unwrap().unwrap();
        assert_eq!(decoded.data["delta"], "hello");
        decoder.line("data: not-json\n").unwrap();
        assert!(decoder.line("\n").is_err());
        assert!(decoder.line(&format!("data: {}", "x".repeat(MAX_FRAME_BYTES))).is_err());
    }

    #[test]
    fn helper_contract_cannot_name_an_external_endpoint_or_shell_command() {
        assert!(serde_json::from_value::<LocalRequest>(json!({ "operation": "subscribe", "session_id": Uuid::new_v4().to_string(), "epoch": 1 })).is_ok());
        for request in [json!({ "operation": "request", "url": "https://example.com" }), json!({ "operation": "shell", "command": "powershell" }), json!({ "operation": "detach", "epoch": 1, "url": "http://127.0.0.1" })] {
            assert!(serde_json::from_value::<LocalRequest>(request).is_err());
        }
    }

    #[test]
    fn a_live_socket_stream_does_not_block_cancel_and_never_exposes_the_bearer() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/local-fixtures").join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(&root).unwrap();
        let token = "fixture-native-stream-private-token";
        std::fs::write(root.join("work.token"), token).unwrap();
        let client = LocalClient::new(LocalConfig { endpoint, state_dir: root.clone() });
        let id = Uuid::new_v4();
        let (release, released) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_headers(&stream, &format!("GET /sessions/{id}/stream?after=0"), token);
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {}\n\n", serde_json::to_string(&event(1, token)).unwrap()).unwrap();
            stream.flush().unwrap();
            let (mut cancel, _) = listener.accept().unwrap();
            read_headers(&cancel, &format!("POST /sessions/{id}/cancel"), token);
            let body = "{\"cancelled\":true}";
            write!(cancel, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
            let _ = stream.write_all(b": detached\n\n");
        });
        let (sender, receiver) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker = Worker {
            client: client.clone(), id, epoch: 1, current_epoch: Arc::new(AtomicU64::new(1)), stop: stop.clone(),
            connected: Arc::new(AtomicBool::new(false)), pending: Arc::new(Mutex::new(HashSet::new())),
            emit: Arc::new(move |name, value| { if name == WORK_EVENT { sender.send(value).unwrap(); } }),
        };
        let reader = std::thread::spawn(move || worker.read(&mut EventCursor::default()));
        let frame = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(frame["event"]["data"]["delta"], "[redacted]");
        assert!(!frame.to_string().contains(token));
        assert_eq!(client.cancel(id).unwrap()["cancelled"], true);
        stop.store(true, Ordering::SeqCst);
        release.send(()).unwrap();
        assert!(matches!(reader.join().unwrap(), StreamExit::Detached));
        server.join().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    fn read_headers(socket: &std::net::TcpStream, expected: &str, token: &str) {
        socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut reader = BufReader::new(socket.try_clone().unwrap());
        let mut headers = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" { break; }
            assert!(!line.is_empty());
            headers.push_str(&line);
        }
        assert!(headers.starts_with(expected));
        assert!(!headers.lines().next().unwrap().contains(token));
        assert!(headers.contains(&format!("Authorization: Bearer {token}")));
    }

    use url::Url;
}