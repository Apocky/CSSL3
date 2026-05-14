// § events.rs : JSONL trace parser ← cssl-rt emits canonical schema
// ══════════════════════════════════════════════════════════════════
// schema(canonical) :
//   {"ts_ns":<u64>,
//    "src":"cssl-rt::host_<mod>",
//    "op":"<domain>.<verb>",
//    "kind":"entry|exit|branch|skip|error",
//    "args":{...},
//    "result":{...|null},
//    "latency_ns":<u64|null>,
//    "note":<string|null>}
// ══════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Entry,
    Exit,
    Branch,
    Skip,
    Error,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::Entry => "entry",
            EventKind::Exit => "exit",
            EventKind::Branch => "branch",
            EventKind::Skip => "skip",
            EventKind::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub ts_ns: u64,
    #[serde(default)]
    pub src: String,
    pub op: String,
    pub kind: EventKind,
    #[serde(default)]
    pub args: serde_json::Value,
    #[serde(default)]
    pub result: serde_json::Value,
    #[serde(default)]
    pub latency_ns: Option<u64>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Error)]
pub enum EventLoadError {
    #[error("io error reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("parse error at {path}:{line_no}: {source}")]
    Parse {
        path: String,
        line_no: usize,
        #[source]
        source: serde_json::Error,
    },
}

/// Load a JSONL trace file, returning events sorted by `ts_ns` ascending.
/// Blank lines + leading-`#` comments are tolerated.
pub fn load_jsonl(path: &Path) -> Result<Vec<Event>, EventLoadError> {
    let raw = std::fs::read_to_string(path).map_err(|e| EventLoadError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    parse_jsonl(&raw, &path.display().to_string())
}

pub fn parse_jsonl(raw: &str, path_label: &str) -> Result<Vec<Event>, EventLoadError> {
    let mut out: Vec<Event> = Vec::new();
    for (i, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let evt: Event =
            serde_json::from_str(trimmed).map_err(|e| EventLoadError::Parse {
                path: path_label.to_string(),
                line_no: i + 1,
                source: e,
            })?;
        out.push(evt);
    }
    out.sort_by_key(|e| e.ts_ns);
    Ok(out)
}
