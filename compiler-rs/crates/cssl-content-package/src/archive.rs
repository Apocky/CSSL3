//! § archive — TARLITE payload-archive format.
//!
//! § DESIGN
//!   `.ccpkg` payloads bundle multiple files (CSSL source · GLTF assets ·
//!   shader bytecode · audio stems · etc.). We avoid pulling a `tar` /
//!   `zip` / `zstd` dep — the whole crate ships pure-Rust on Ed25519 +
//!   BLAKE3 + serde_json + thiserror — and roll a deterministic LITE
//!   archive format optimised for round-trip stability and BLAKE3-friendly
//!   stream-hashing.
//!
//! § BYTE LAYOUT
//!
//! ```text
//! offset  size  field           notes
//! ──────  ────  ──────────────  ─────────────────────────────────────────
//! 0       4     magic           "TLIT" = 0x54 0x4C 0x49 0x54
//! 4       4     entry_count     u32 LE · number of file-entries
//! 8       N     entries...      see entry layout below
//! ──────  ────  ──────────────  ─────────────────────────────────────────
//!
//! Each entry :
//!   2     path_len      u16 LE · max 65535
//!   path  path_bytes    UTF-8 path · forward-slash separators only · no `..`
//!   8     content_len   u64 LE
//!   N     content_bytes raw file bytes
//! ```
//!
//! § INVARIANTS (verified at decode time)
//!   - All paths are valid UTF-8.
//!   - Forward-slash separators only (no `\`).
//!   - No `.` / `..` segments (path-traversal defense).
//!   - No leading `/`.
//!   - Entries appear in lexicographic order of `path` (deterministic-pack).
//!
//! § DETERMINISM
//!   `archive_pack` sorts entries by path before writing, so any input
//!   `&[ArchiveEntry]` produces a byte-stable archive — required for
//!   reproducible BLAKE3 digests across signers.

use thiserror::Error;

/// 4-byte archive magic prefix : ASCII "TLIT" — Tar-LITe.
pub const ARCHIVE_MAGIC: [u8; 4] = *b"TLIT";

/// Header bytes : magic(4) + entry_count(4).
const ARCHIVE_HEADER_BYTES: usize = 8;

/// § One file inside the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    /// UTF-8 path, forward-slash separators, no `..`.
    pub path: String,
    /// Raw file bytes.
    pub content: Vec<u8>,
}

/// § Archive errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ArchiveError {
    #[error("archive too short : got {got} bytes, need at least {need}")]
    TooShort { got: usize, need: usize },
    #[error("bad magic prefix : not 'TLIT'")]
    BadMagic,
    #[error("path contains illegal segment '{0}'")]
    IllegalPath(String),
    #[error("path is not valid UTF-8")]
    PathNotUtf8,
    #[error("path length {0} exceeds 65535")]
    PathTooLong(usize),
    #[error("content length overflow")]
    ContentLenOverflow,
    #[error("entries are not in lexicographic order : '{0}' followed by '{1}'")]
    EntriesNotSorted(String, String),
    #[error("duplicate path '{0}'")]
    DuplicatePath(String),
}

/// § Pack a `&[ArchiveEntry]` into a TARLITE byte buffer.
///
/// Entries are sorted by path before writing for deterministic output.
/// Path traversal defense rejects `..` / `.` / leading-`/` / backslash.
pub fn archive_pack(entries: &[ArchiveEntry]) -> Result<Vec<u8>, ArchiveError> {
    // Validate paths first, in-place.
    for e in entries {
        validate_path(&e.path)?;
        if e.path.len() > u16::MAX as usize {
            return Err(ArchiveError::PathTooLong(e.path.len()));
        }
    }
    // Clone-sort by path. (Cheap because vec-of-borrows is fine via index sort.)
    let mut sorted: Vec<&ArchiveEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    // Reject duplicates.
    for w in sorted.windows(2) {
        if w[0].path == w[1].path {
            return Err(ArchiveError::DuplicatePath(w[0].path.clone()));
        }
    }
    let mut out = Vec::with_capacity(ARCHIVE_HEADER_BYTES + sorted.iter().map(|e|
        2 + e.path.len() + 8 + e.content.len()
    ).sum::<usize>());
    out.extend_from_slice(&ARCHIVE_MAGIC);
    out.extend_from_slice(&(sorted.len() as u32).to_le_bytes());
    for e in &sorted {
        out.extend_from_slice(&(e.path.len() as u16).to_le_bytes());
        out.extend_from_slice(e.path.as_bytes());
        out.extend_from_slice(&(e.content.len() as u64).to_le_bytes());
        out.extend_from_slice(&e.content);
    }
    Ok(out)
}

/// § Unpack a TARLITE byte buffer into `Vec<ArchiveEntry>`.
///
/// Validates magic, lex-sort invariant (rejecting unsorted archives so the
/// signed bytes are unambiguous), and path-traversal defenses.
pub fn archive_unpack(bytes: &[u8]) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    if bytes.len() < ARCHIVE_HEADER_BYTES {
        return Err(ArchiveError::TooShort {
            got: bytes.len(),
            need: ARCHIVE_HEADER_BYTES,
        });
    }
    if bytes[0..4] != ARCHIVE_MAGIC {
        return Err(ArchiveError::BadMagic);
    }
    let entry_count = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    let mut cursor = ARCHIVE_HEADER_BYTES;
    let mut out = Vec::with_capacity(entry_count);
    let mut prev_path: Option<String> = None;
    for _ in 0..entry_count {
        if cursor + 2 > bytes.len() {
            return Err(ArchiveError::TooShort {
                got: bytes.len(),
                need: cursor + 2,
            });
        }
        let path_len =
            u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]) as usize;
        cursor += 2;
        if cursor + path_len > bytes.len() {
            return Err(ArchiveError::TooShort {
                got: bytes.len(),
                need: cursor + path_len,
            });
        }
        let path =
            std::str::from_utf8(&bytes[cursor..cursor + path_len])
                .map_err(|_| ArchiveError::PathNotUtf8)?
                .to_string();
        validate_path(&path)?;
        cursor += path_len;
        if cursor + 8 > bytes.len() {
            return Err(ArchiveError::TooShort {
                got: bytes.len(),
                need: cursor + 8,
            });
        }
        let content_len = u64::from_le_bytes([
            bytes[cursor],
            bytes[cursor + 1],
            bytes[cursor + 2],
            bytes[cursor + 3],
            bytes[cursor + 4],
            bytes[cursor + 5],
            bytes[cursor + 6],
            bytes[cursor + 7],
        ]) as usize;
        cursor += 8;
        let cursor_end =
            cursor.checked_add(content_len).ok_or(ArchiveError::ContentLenOverflow)?;
        if cursor_end > bytes.len() {
            return Err(ArchiveError::TooShort {
                got: bytes.len(),
                need: cursor_end,
            });
        }
        let content = bytes[cursor..cursor_end].to_vec();
        cursor = cursor_end;
        if let Some(prev) = &prev_path {
            if prev.as_str() >= path.as_str() {
                return Err(ArchiveError::EntriesNotSorted(prev.clone(), path.clone()));
            }
        }
        prev_path = Some(path.clone());
        out.push(ArchiveEntry { path, content });
    }
    Ok(out)
}

/// § Validate a path : forward-slash only, no `..`/`.`, no leading-/, no backslash.
fn validate_path(p: &str) -> Result<(), ArchiveError> {
    if p.is_empty() {
        return Err(ArchiveError::IllegalPath("(empty)".to_string()));
    }
    if p.contains('\\') {
        return Err(ArchiveError::IllegalPath(p.to_string()));
    }
    if p.starts_with('/') {
        return Err(ArchiveError::IllegalPath(p.to_string()));
    }
    for seg in p.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return Err(ArchiveError::IllegalPath(p.to_string()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_entries() -> Vec<ArchiveEntry> {
        vec![
            ArchiveEntry {
                path: "scenes/main.cssl".to_string(),
                content: b"§ scene main\n  ¬ harm".to_vec(),
            },
            ArchiveEntry {
                path: "assets/torch.gltf".to_string(),
                content: vec![0x67, 0x6c, 0x54, 0x46], // "glTF"
            },
        ]
    }

    #[test]
    fn pack_unpack_roundtrip() {
        let entries = fixture_entries();
        let bytes = archive_pack(&entries).unwrap();
        let back = archive_unpack(&bytes).unwrap();
        // Lex sort : assets/ < scenes/ → the sorted order must match.
        let mut sorted = entries;
        sorted.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(back, sorted);
    }

    #[test]
    fn empty_archive_roundtrips() {
        let bytes = archive_pack(&[]).unwrap();
        assert_eq!(bytes.len(), ARCHIVE_HEADER_BYTES);
        let back = archive_unpack(&bytes).unwrap();
        assert!(back.is_empty());
    }

    #[test]
    fn bad_magic_rejected() {
        let mut bytes = archive_pack(&fixture_entries()).unwrap();
        bytes[0] = 0;
        assert_eq!(archive_unpack(&bytes), Err(ArchiveError::BadMagic));
    }

    #[test]
    fn pack_rejects_dotdot() {
        let bad = vec![ArchiveEntry {
            path: "../etc/passwd".to_string(),
            content: vec![],
        }];
        assert!(matches!(archive_pack(&bad), Err(ArchiveError::IllegalPath(_))));
    }

    #[test]
    fn pack_rejects_backslash() {
        let bad = vec![ArchiveEntry {
            path: "windows\\path".to_string(),
            content: vec![],
        }];
        assert!(matches!(archive_pack(&bad), Err(ArchiveError::IllegalPath(_))));
    }

    #[test]
    fn pack_rejects_leading_slash() {
        let bad = vec![ArchiveEntry {
            path: "/abs/path".to_string(),
            content: vec![],
        }];
        assert!(matches!(archive_pack(&bad), Err(ArchiveError::IllegalPath(_))));
    }

    #[test]
    fn pack_rejects_dot_segment() {
        let bad = vec![ArchiveEntry {
            path: "a/./b".to_string(),
            content: vec![],
        }];
        assert!(matches!(archive_pack(&bad), Err(ArchiveError::IllegalPath(_))));
    }

    #[test]
    fn pack_rejects_duplicate_path() {
        let bad = vec![
            ArchiveEntry {
                path: "a.cssl".to_string(),
                content: vec![1],
            },
            ArchiveEntry {
                path: "a.cssl".to_string(),
                content: vec![2],
            },
        ];
        assert!(matches!(archive_pack(&bad), Err(ArchiveError::DuplicatePath(_))));
    }

    #[test]
    fn pack_is_deterministic() {
        let entries = fixture_entries();
        let a = archive_pack(&entries).unwrap();
        let b = archive_pack(&entries).unwrap();
        assert_eq!(a, b);
        // And reversed-input gives same output (sort-by-path inside pack).
        let mut reversed = entries;
        reversed.reverse();
        let c = archive_pack(&reversed).unwrap();
        assert_eq!(a, c);
    }

    #[test]
    fn truncated_archive_rejected() {
        let bytes = archive_pack(&fixture_entries()).unwrap();
        let truncated = &bytes[..bytes.len() - 5];
        assert!(matches!(archive_unpack(truncated), Err(ArchiveError::TooShort { .. })));
    }

    #[test]
    fn out_of_order_archive_rejected() {
        // Hand-craft : 2 entries with paths "z.cssl" then "a.cssl" (NOT sorted).
        let mut buf = Vec::new();
        buf.extend_from_slice(&ARCHIVE_MAGIC);
        buf.extend_from_slice(&2u32.to_le_bytes());
        // entry 1 : "z.cssl"
        let p1 = "z.cssl";
        buf.extend_from_slice(&(p1.len() as u16).to_le_bytes());
        buf.extend_from_slice(p1.as_bytes());
        buf.extend_from_slice(&0u64.to_le_bytes());
        // entry 2 : "a.cssl"
        let p2 = "a.cssl";
        buf.extend_from_slice(&(p2.len() as u16).to_le_bytes());
        buf.extend_from_slice(p2.as_bytes());
        buf.extend_from_slice(&0u64.to_le_bytes());
        assert!(matches!(
            archive_unpack(&buf),
            Err(ArchiveError::EntriesNotSorted(_, _))
        ));
    }

    #[test]
    fn empty_path_rejected() {
        let bad = vec![ArchiveEntry {
            path: "".to_string(),
            content: vec![],
        }];
        assert!(matches!(archive_pack(&bad), Err(ArchiveError::IllegalPath(_))));
    }
}
