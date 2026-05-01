//! § T11-WAVE3-REPLAY · `integrity.rs`
//!
//! SHA-256 file digest + manifest write/verify for replay-file integrity.
//!
//! § DESIGN
//!
//! Hand-rolled FIPS 180-4 SHA-256 (≈ 80 LOC) — no external crypto dep.
//! Rationale : (a) keep the dep-graph minimal per the slice-prompt's
//! "stdlib-heavy" directive ; (b) crypto-correctness for a *file integrity
//! check* is unconditionally critical AND simple ; (c) RFC 6234 test
//! vectors are public, so unit-testing against them is sufficient.
//!
//! § SCHEMA
//!
//! `ReplayManifest` is a sidecar JSON file describing the replay :
//!   - `sha256_hex`     — 64-char lowercase hex digest of the replay file
//!   - `event_count`    — number of valid events parsed (NOT lines — malformed-skipped)
//!   - `duration_micros`— ts_micros of the last event (0 if empty)
//!   - `schema_version` — `SCHEMA_VERSION`, bumped on `ReplayEventKind` changes
//!
//! § SECURITY POSTURE
//!
//! This is an *integrity* check, not authentication.  An attacker with
//! write-access to BOTH replay + manifest can re-sign their tamper.  For
//! authenticated replay, layer Ed25519 (cssl-crypto-stub upgrade path).

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::replayer::Replayer;

/// Schema version embedded in `ReplayManifest`.  Bump when `ReplayEventKind`
/// gains a variant or `ReplayEvent` gains a field.
pub const SCHEMA_VERSION: u32 = 1;

/// Sidecar manifest describing a replay file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayManifest {
    /// Lowercase hex SHA-256 digest of the replay file contents.
    pub sha256_hex: String,
    /// Count of valid (non-malformed, non-empty) events in the replay.
    pub event_count: u64,
    /// `ts_micros` of the final event (0 if empty).
    pub duration_micros: u64,
    /// Schema version this manifest was written under.
    pub schema_version: u32,
}

/// Compute the SHA-256 digest of `path` as a 64-char lowercase hex string.
pub fn sha256_file(path: impl AsRef<Path>) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize_hex())
}

/// Write a manifest sidecar for `replay_path` to `manifest_path`.
pub fn write_manifest(
    replay_path: impl AsRef<Path>,
    manifest_path: impl AsRef<Path>,
) -> io::Result<()> {
    let sha256_hex = sha256_file(replay_path.as_ref())?;
    let r = Replayer::from_path(replay_path.as_ref())?;
    let event_count = r.len() as u64;
    let duration_micros = r.events().last().map_or(0, |ev| ev.ts_micros);
    let manifest = ReplayManifest {
        sha256_hex,
        event_count,
        duration_micros,
        schema_version: SCHEMA_VERSION,
    };
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut out = File::create(manifest_path)?;
    out.write_all(json.as_bytes())?;
    out.flush()?;
    Ok(())
}

/// Verify that `manifest_path` matches `replay_path`'s current digest.
/// Returns `Ok(true)` on match, `Ok(false)` on mismatch, `Err` on I/O error.
pub fn verify_manifest(
    replay_path: impl AsRef<Path>,
    manifest_path: impl AsRef<Path>,
) -> io::Result<bool> {
    let mut s = String::new();
    File::open(manifest_path)?.read_to_string(&mut s)?;
    let manifest: ReplayManifest = serde_json::from_str(&s)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let live = sha256_file(replay_path)?;
    Ok(manifest.sha256_hex == live && manifest.schema_version == SCHEMA_VERSION)
}

// ─────────────────────────────────────────────────────────────────
// SHA-256 core (FIPS 180-4 · public-domain algorithm)
// ─────────────────────────────────────────────────────────────────

const K: [u32; 64] = [
    0x428a_2f98, 0x7137_4491, 0xb5c0_fbcf, 0xe9b5_dba5, 0x3956_c25b, 0x59f1_11f1, 0x923f_82a4,
    0xab1c_5ed5, 0xd807_aa98, 0x1283_5b01, 0x2431_85be, 0x550c_7dc3, 0x72be_5d74, 0x80de_b1fe,
    0x9bdc_06a7, 0xc19b_f174, 0xe49b_69c1, 0xefbe_4786, 0x0fc1_9dc6, 0x240c_a1cc, 0x2de9_2c6f,
    0x4a74_84aa, 0x5cb0_a9dc, 0x76f9_88da, 0x983e_5152, 0xa831_c66d, 0xb003_27c8, 0xbf59_7fc7,
    0xc6e0_0bf3, 0xd5a7_9147, 0x06ca_6351, 0x1429_2967, 0x27b7_0a85, 0x2e1b_2138, 0x4d2c_6dfc,
    0x5338_0d13, 0x650a_7354, 0x766a_0abb, 0x81c2_c92e, 0x9272_2c85, 0xa2bf_e8a1, 0xa81a_664b,
    0xc24b_8b70, 0xc76c_51a3, 0xd192_e819, 0xd699_0624, 0xf40e_3585, 0x106a_a070, 0x19a4_c116,
    0x1e37_6c08, 0x2748_774c, 0x34b0_bcb5, 0x391c_0cb3, 0x4ed8_aa4a, 0x5b9c_ca4f, 0x682e_6ff3,
    0x748f_82ee, 0x78a5_636f, 0x84c8_7814, 0x8cc7_0208, 0x90be_fffa, 0xa450_6ceb, 0xbef9_a3f7,
    0xc671_78f2,
];

const H0: [u32; 8] = [
    0x6a09_e667, 0xbb67_ae85, 0x3c6e_f372, 0xa54f_f53a, 0x510e_527f, 0x9b05_688c, 0x1f83_d9ab,
    0x5be0_cd19,
];

struct Sha256 {
    state: [u32; 8],
    buf: [u8; 64],
    buf_len: usize,
    total_bits: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: H0,
            buf: [0u8; 64],
            buf_len: 0,
            total_bits: 0,
        }
    }

    fn update(&mut self, mut data: &[u8]) {
        self.total_bits = self.total_bits.wrapping_add((data.len() as u64) * 8);
        if self.buf_len > 0 {
            let need = 64 - self.buf_len;
            let take = need.min(data.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            data = &data[take..];
            if self.buf_len == 64 {
                let block = self.buf;
                Self::compress(&mut self.state, &block);
                self.buf_len = 0;
            }
        }
        while data.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[..64]);
            Self::compress(&mut self.state, &block);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }

    fn finalize_hex(mut self) -> String {
        let bits = self.total_bits;
        // pad : 0x80 then zeros then 8-byte big-endian bit-length.
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;
        if self.buf_len > 56 {
            for i in self.buf_len..64 {
                self.buf[i] = 0;
            }
            let block = self.buf;
            Self::compress(&mut self.state, &block);
            self.buf_len = 0;
        }
        for i in self.buf_len..56 {
            self.buf[i] = 0;
        }
        self.buf[56..64].copy_from_slice(&bits.to_be_bytes());
        let block = self.buf;
        Self::compress(&mut self.state, &block);
        let mut out = String::with_capacity(64);
        for word in self.state {
            for b in word.to_be_bytes() {
                use std::fmt::Write as _;
                let _ = write!(out, "{b:02x}");
            }
        }
        out
    }

    #[allow(clippy::many_single_char_names)]
    fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 =
                w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 =
                w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut a = state[0];
        let mut b = state[1];
        let mut c = state[2];
        let mut d = state[3];
        let mut e = state[4];
        let mut f = state[5];
        let mut g = state[6];
        let mut h = state[7];
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{ReplayEvent, ReplayEventKind};
    use crate::recorder::Recorder;
    use std::path::PathBuf;

    fn temp_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("cssl-host-replay-int-{tag}-{pid}-{nanos}.jsonl"));
        p
    }

    /// FIPS 180-4 NIST test vectors — empty + "abc" + 56-char string.
    /// (Sanity check that the hand-rolled SHA-256 matches the standard.)
    #[test]
    fn sha256_known_vectors() {
        let mut h = Sha256::new();
        h.update(b"");
        assert_eq!(
            h.finalize_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let mut h = Sha256::new();
        h.update(b"abc");
        assert_eq!(
            h.finalize_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let mut h = Sha256::new();
        h.update(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq");
        assert_eq!(
            h.finalize_hex(),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    fn write_replay(path: &Path) {
        let mut r = Recorder::new(path).expect("open");
        r.append(ReplayEventKind::KeyDown(1)).unwrap();
        r.append(ReplayEventKind::KeyUp(1)).unwrap();
        r.flush().unwrap();
    }

    #[test]
    fn write_then_verify_passes() {
        let r = temp_path("write-verify-replay");
        let m = temp_path("write-verify-manifest");
        write_replay(&r);
        write_manifest(&r, &m).expect("write manifest");
        let ok = verify_manifest(&r, &m).expect("verify");
        assert!(ok, "freshly-written manifest must verify");
        // Sanity : manifest contents reasonable.
        let body = std::fs::read_to_string(&m).expect("read manifest");
        let parsed: ReplayManifest = serde_json::from_str(&body).expect("parse");
        assert_eq!(parsed.event_count, 2);
        assert_eq!(parsed.schema_version, SCHEMA_VERSION);
        assert_eq!(parsed.sha256_hex.len(), 64);
        let _ = std::fs::remove_file(&r);
        let _ = std::fs::remove_file(&m);
    }

    #[test]
    fn tamper_fails_verify() {
        let r = temp_path("tamper-replay");
        let m = temp_path("tamper-manifest");
        write_replay(&r);
        write_manifest(&r, &m).expect("write manifest");
        // Tamper : append an extra event AFTER manifest is written.
        {
            let mut rec = Recorder::new(&r).expect("reopen");
            rec.append(ReplayEventKind::KeyDown(99)).unwrap();
            rec.flush().unwrap();
        }
        let ok = verify_manifest(&r, &m).expect("verify");
        assert!(!ok, "post-write tamper must fail verification");
        let _ = std::fs::remove_file(&r);
        let _ = std::fs::remove_file(&m);
    }

    #[test]
    fn empty_replay_manifest() {
        let r = temp_path("empty-replay");
        let m = temp_path("empty-manifest");
        std::fs::write(&r, "").expect("write empty");
        write_manifest(&r, &m).expect("write manifest");
        let body = std::fs::read_to_string(&m).expect("read manifest");
        let parsed: ReplayManifest = serde_json::from_str(&body).expect("parse");
        assert_eq!(parsed.event_count, 0);
        assert_eq!(parsed.duration_micros, 0);
        // Empty-file SHA-256 = e3b0...b855
        assert_eq!(
            parsed.sha256_hex,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        // And it round-trip-verifies.
        assert!(verify_manifest(&r, &m).expect("verify"));
        // Suppress the unused-import warning for ReplayEvent since this test
        // path doesn't construct one directly.
        let _ = std::any::type_name::<ReplayEvent>();
        let _ = std::fs::remove_file(&r);
        let _ = std::fs::remove_file(&m);
    }
}
