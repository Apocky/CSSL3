//! [`StackTrace`] capture + [`StackFrame`] representation.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 1.5.
//!
//! § DESIGN
//!   - `std::backtrace::Backtrace` is stable since Rust 1.65 ; we use it as
//!     the capture-mechanism. No external `backtrace` crate dep needed.
//!   - The `debug-info` feature gates capture : when ON, backtraces are
//!     captured eagerly ; when OFF, [`StackTrace::capture`] returns an
//!     empty trace (zero-overhead).
//!   - All file-paths in captured frames are HASHED via a thread-local
//!     [`cssl_telemetry::PathHasher`] before the frame escapes the capture-site.
//!     Raw-paths NEVER cross the module boundary.
//!   - Frame-list is capped at [`MAX_FRAMES`] = 32 to avoid alloc-storms on
//!     deep recursion.
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - § 1 surveillance : path-hash-only discipline (D130) preserved across
//!     stack-frame boundaries.

use core::fmt;
use std::cell::RefCell;

use cssl_telemetry::{PathHash, PathHasher};

/// Maximum frames kept in a [`StackTrace`]. Bounded to avoid alloc-storms.
pub const MAX_FRAMES: usize = 32;

// ───────────────────────────────────────────────────────────────────────
// § Thread-local PathHasher for stack-frame path-hashing.
// ───────────────────────────────────────────────────────────────────────

thread_local! {
    /// Thread-local [`PathHasher`] used to hash captured frame-paths.
    /// Set via [`install_thread_path_hasher`] @ engine-init ; if not set,
    /// captured frames carry [`PathHash::zero()`] (sentinel for "no hasher
    /// installed"). Tests typically install a fixed-seed hasher.
    static THREAD_PATH_HASHER: RefCell<Option<PathHasher>> = const { RefCell::new(None) };
}

/// Install a thread-local [`PathHasher`] used for hashing captured stack-
/// frame file-paths. Call once @ engine-init per thread that captures
/// stack-traces.
///
/// § DESIGN
///   - This is a thread-local because the engine has multiple threads
///     (render, audio, work-pool) ; each may capture from different sites.
///   - The hasher is `Clone`-able : install a copy from the engine's
///     master hasher (same salt = same hash domain).
pub fn install_thread_path_hasher(hasher: PathHasher) {
    THREAD_PATH_HASHER.with(|cell| {
        *cell.borrow_mut() = Some(hasher);
    });
}

/// Clear the thread-local [`PathHasher`]. Tests use this to reset state.
pub fn clear_thread_path_hasher() {
    THREAD_PATH_HASHER.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

/// Hash a path-string with the thread-local hasher ; falls back to
/// [`PathHash::zero()`] if no hasher is installed.
fn hash_path_with_thread_hasher(path: &str) -> PathHash {
    THREAD_PATH_HASHER.with(|cell| {
        cell.borrow()
            .as_ref()
            .map_or(PathHash::zero(), |h| h.hash_str(path))
    })
}

// ───────────────────────────────────────────────────────────────────────
// § StackFrame — single captured frame.
// ───────────────────────────────────────────────────────────────────────

/// A single captured stack-frame.
///
/// § INVARIANTS
///   - `function` is owned `String` (demangled symbol name) ; small enough
///     to be cheap to clone.
///   - `file_path_hash` is a [`PathHash`] (D130-enforced).
///   - `line` is u32 (1-indexed).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StackFrame {
    /// Demangled symbol-name. May be `"<unknown>"` for stripped frames.
    pub function: String,
    /// 32-byte BLAKE3 hash of the source file path.
    pub file_path_hash: PathHash,
    /// 1-indexed line-number in the source file. 0 for unknown.
    pub line: u32,
}

impl StackFrame {
    /// Construct a [`StackFrame`].
    #[must_use]
    pub fn new(function: impl Into<String>, file_path_hash: PathHash, line: u32) -> Self {
        Self {
            function: function.into(),
            file_path_hash,
            line,
        }
    }

    /// Sentinel "unknown" frame ; used when no symbols are available.
    #[must_use]
    pub fn unknown() -> Self {
        Self {
            function: String::from("<unknown>"),
            file_path_hash: PathHash::zero(),
            line: 0,
        }
    }
}

impl fmt::Display for StackFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.function)
        } else {
            write!(f, "{} @ {}:{}", self.function, self.file_path_hash, self.line)
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § StackTrace — captured + bounded list of frames.
// ───────────────────────────────────────────────────────────────────────

/// A captured stack-trace.
///
/// § DESIGN
///   - Frame-list is bounded at [`MAX_FRAMES`] ; longer traces are truncated
///     from the top (innermost frames preserved).
///   - When `debug-info` feature is OFF, [`StackTrace::capture`] returns
///     [`StackTrace::empty`] for zero-overhead.
///   - Display format is human-readable : one frame per line, indented.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct StackTrace {
    pub frames: Vec<StackFrame>,
}

impl StackTrace {
    /// Construct an empty stack-trace.
    #[must_use]
    pub fn empty() -> Self {
        Self { frames: Vec::new() }
    }

    /// Construct from a (truncated) list of frames. Length is capped at
    /// [`MAX_FRAMES`].
    #[must_use]
    pub fn from_frames(frames: Vec<StackFrame>) -> Self {
        let mut frames = frames;
        if frames.len() > MAX_FRAMES {
            frames.truncate(MAX_FRAMES);
        }
        Self { frames }
    }

    /// Capture the current stack-trace.
    ///
    /// § BEHAVIOR
    ///   - With `debug-info` feature : uses `std::backtrace::Backtrace` to
    ///     capture frames ; parses the debug-formatted output to extract
    ///     function-names + file-paths + lines ; hashes paths via the
    ///     thread-local hasher.
    ///   - Without `debug-info` feature : returns [`StackTrace::empty`].
    ///
    /// § NOTE on parsing
    ///   `std::backtrace::Backtrace::frames()` is unstable ; the `Display`
    ///   format is the only stable surface. We parse the human-readable
    ///   form. A frame line looks like :
    ///   `  N: function_name`
    ///   `       at /path/to/file.rs:LINE`
    ///   Our parser is best-effort : malformed lines are kept as
    ///   `function`-only frames.
    #[must_use]
    pub fn capture() -> Self {
        if !cfg!(feature = "debug-info") {
            return Self::empty();
        }
        let bt = std::backtrace::Backtrace::force_capture();
        let s = format!("{bt}");
        Self::parse_backtrace_string(&s)
    }

    /// Parse a `std::backtrace::Backtrace` Display string into frames.
    /// Best-effort ; resilient to partial / malformed input.
    ///
    /// § VISIBILITY
    ///   Public for integration-test convenience (Wave-Jε-1 path-hash
    ///   discipline tests inject synthetic backtrace strings to verify
    ///   path-hash-only behavior without relying on real OS captures).
    pub fn parse_backtrace_string(s: &str) -> Self {
        let mut frames: Vec<StackFrame> = Vec::new();
        // The Display form alternates :
        //   <N>: <function-name>
        //       at <file-path>:<line>
        // We scan line-by-line, grouping by the "<N>:" anchor.
        let mut current_function: Option<String> = None;
        for raw_line in s.lines() {
            let line = raw_line.trim_start();
            if let Some(rest) = leading_index_colon_strip(line) {
                // Flush previous frame (if any) with no file-info.
                if let Some(prev_fn) = current_function.take() {
                    if frames.len() < MAX_FRAMES {
                        frames.push(StackFrame::new(prev_fn, PathHash::zero(), 0));
                    }
                }
                current_function = Some(rest.to_string());
            } else if let Some(at_rest) = line.strip_prefix("at ") {
                // file:line ; commit the pending frame with file-info.
                if let Some(func) = current_function.take() {
                    let (path, ln) = split_path_line(at_rest);
                    let phash = hash_path_with_thread_hasher(path);
                    if frames.len() < MAX_FRAMES {
                        frames.push(StackFrame::new(func, phash, ln));
                    }
                }
            }
        }
        // Flush trailing function w/o file-info.
        if let Some(func) = current_function {
            if frames.len() < MAX_FRAMES {
                frames.push(StackFrame::new(func, PathHash::zero(), 0));
            }
        }
        Self { frames }
    }

    /// `true` if no frames captured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Number of frames captured.
    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }
}

/// Strip a leading `<N>:` prefix from a frame-line. Returns the remainder
/// (the function-name) on success ; `None` if no such prefix.
fn leading_index_colon_strip(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    if i >= bytes.len() || bytes[i] != b':' {
        return None;
    }
    Some(line[i + 1..].trim_start())
}

/// Split a `<path>:<line>` into (path, line). Best-effort : non-numeric
/// suffixes yield line=0.
fn split_path_line(s: &str) -> (&str, u32) {
    let s = s.trim();
    if let Some(idx) = s.rfind(':') {
        let (lhs, rhs) = s.split_at(idx);
        let rhs = rhs.trim_start_matches(':');
        if let Ok(line) = rhs.parse::<u32>() {
            return (lhs, line);
        }
    }
    (s, 0)
}

impl fmt::Display for StackTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, frame) in self.frames.iter().enumerate() {
            writeln!(f, "  {i:>2}: {frame}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        clear_thread_path_hasher, install_thread_path_hasher, leading_index_colon_strip,
        split_path_line, StackFrame, StackTrace, MAX_FRAMES,
    };
    use cssl_telemetry::{PathHash, PathHasher};

    fn h() -> PathHasher {
        PathHasher::from_seed([3u8; 32])
    }

    #[test]
    fn stack_frame_display_with_line() {
        let p = h().hash_str("/src/foo.rs");
        let frame = StackFrame::new("module::function", p, 42);
        let s = format!("{frame}");
        assert!(s.contains("module::function"));
        assert!(s.contains(":42"));
    }

    #[test]
    fn stack_frame_display_without_line() {
        let frame = StackFrame::new("anonymous", PathHash::zero(), 0);
        assert_eq!(format!("{frame}"), "anonymous");
    }

    #[test]
    fn stack_frame_unknown_sentinel() {
        let f = StackFrame::unknown();
        assert_eq!(f.function, "<unknown>");
        assert_eq!(f.file_path_hash, PathHash::zero());
        assert_eq!(f.line, 0);
    }

    #[test]
    fn stack_trace_empty_default() {
        let t = StackTrace::empty();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn stack_trace_from_frames_truncates_at_max() {
        let frames: Vec<StackFrame> = (0..MAX_FRAMES + 10)
            .map(|i| StackFrame::new(format!("f{i}"), PathHash::zero(), i as u32))
            .collect();
        let t = StackTrace::from_frames(frames);
        assert_eq!(t.len(), MAX_FRAMES);
    }

    #[test]
    fn stack_trace_from_frames_short_preserves_all() {
        let frames: Vec<StackFrame> = (0..5)
            .map(|i| StackFrame::new(format!("f{i}"), PathHash::zero(), i as u32))
            .collect();
        let t = StackTrace::from_frames(frames);
        assert_eq!(t.len(), 5);
    }

    #[test]
    fn leading_index_colon_strip_basic() {
        assert_eq!(
            leading_index_colon_strip("0: foo::bar"),
            Some("foo::bar")
        );
        assert_eq!(
            leading_index_colon_strip("17:  some_module::fn"),
            Some("some_module::fn")
        );
    }

    #[test]
    fn leading_index_colon_strip_rejects_non_index() {
        assert!(leading_index_colon_strip("not an index").is_none());
        assert!(leading_index_colon_strip("at /path:1").is_none());
        assert!(leading_index_colon_strip("").is_none());
        assert!(leading_index_colon_strip("17 missing colon").is_none());
    }

    #[test]
    fn split_path_line_basic() {
        let (p, l) = split_path_line("/usr/lib/foo.rs:42");
        assert_eq!(p, "/usr/lib/foo.rs");
        assert_eq!(l, 42);
    }

    #[test]
    fn split_path_line_no_line() {
        let (p, l) = split_path_line("/no/line/info.rs");
        assert_eq!(p, "/no/line/info.rs");
        assert_eq!(l, 0);
    }

    #[test]
    fn split_path_line_windows_drive() {
        // Drive-letter `:` should not be confused for line-separator if
        // followed by a non-numeric. But if everything after the LAST `:`
        // is numeric, that wins.
        let (_p, l) = split_path_line("C:\\src\\foo.rs:99");
        assert_eq!(l, 99);
    }

    #[test]
    fn parse_backtrace_string_extracts_frames() {
        let bt = "\
            stack backtrace:\n\
               0: do_thing\n\
                         at /repo/src/thing.rs:10\n\
               1: another\n\
                         at /repo/src/another.rs:20\n";
        clear_thread_path_hasher();
        let t = StackTrace::parse_backtrace_string(bt);
        assert!(t.len() >= 2);
        assert_eq!(t.frames[0].function, "do_thing");
        assert_eq!(t.frames[0].line, 10);
        assert_eq!(t.frames[1].function, "another");
        assert_eq!(t.frames[1].line, 20);
    }

    #[test]
    fn parse_backtrace_string_preserves_function_only_frames() {
        let bt = "0: lone\n  1: missing_file\n";
        let t = StackTrace::parse_backtrace_string(bt);
        // Both functions captured ; both have line 0 (no `at` line).
        assert_eq!(t.frames.len(), 2);
        assert_eq!(t.frames[0].function, "lone");
        assert_eq!(t.frames[0].line, 0);
        assert_eq!(t.frames[1].function, "missing_file");
        assert_eq!(t.frames[1].line, 0);
    }

    #[test]
    fn parse_backtrace_string_caps_at_max_frames() {
        let mut bt = String::new();
        for i in 0..(MAX_FRAMES + 20) {
            bt.push_str(&format!("{i}: fn_{i}\n"));
        }
        let t = StackTrace::parse_backtrace_string(&bt);
        assert!(t.frames.len() <= MAX_FRAMES);
    }

    #[test]
    fn install_and_use_thread_path_hasher() {
        let hasher = h();
        install_thread_path_hasher(hasher);
        let bt = "0: f\n   at /test.rs:1\n";
        let t = StackTrace::parse_backtrace_string(bt);
        assert!(t.frames[0].file_path_hash != PathHash::zero());
        clear_thread_path_hasher();
    }

    #[test]
    fn no_thread_hasher_yields_zero_hash() {
        clear_thread_path_hasher();
        let bt = "0: f\n   at /test.rs:1\n";
        let t = StackTrace::parse_backtrace_string(bt);
        assert_eq!(t.frames[0].file_path_hash, PathHash::zero());
    }

    #[test]
    fn capture_returns_some_frames_when_debug_info_on() {
        if !cfg!(feature = "debug-info") {
            // Skip when feature off ; capture() is no-op.
            return;
        }
        clear_thread_path_hasher();
        let t = StackTrace::capture();
        // Live capture inside test should yield at least one frame.
        assert!(!t.is_empty());
    }

    #[test]
    fn stack_trace_display_writes_indexed_lines() {
        let frames = vec![
            StackFrame::new("alpha", PathHash::zero(), 0),
            StackFrame::new("beta", PathHash::zero(), 0),
        ];
        let t = StackTrace::from_frames(frames);
        let s = format!("{t}");
        assert!(s.contains("0:"));
        assert!(s.contains("alpha"));
        assert!(s.contains("beta"));
    }

    #[test]
    fn stack_trace_eq_and_hash_compatible() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let f0 = StackFrame::new("a", PathHash::zero(), 1);
        let f1 = StackFrame::new("a", PathHash::zero(), 1);
        assert_eq!(f0, f1);
        let mut h0 = DefaultHasher::new();
        let mut h1 = DefaultHasher::new();
        f0.hash(&mut h0);
        f1.hash(&mut h1);
        assert_eq!(h0.finish(), h1.finish());
    }
}
