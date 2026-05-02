//! § tokenize — BLAKE3-derived deterministic tokenizer.
#![allow(clippy::module_name_repetitions)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub vocab_id: u32,
    pub byte_start: u32,
    pub byte_end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenOwned {
    pub vocab_id: u32,
    pub text: String,
}

pub struct Tokenizer<'a> {
    text: &'a str,
}

impl<'a> Tokenizer<'a> {
    pub fn new(text: &'a str) -> Self {
        Self { text }
    }

    pub fn iter(&self) -> TokenIter<'a> {
        TokenIter::new(self.text)
    }

    pub fn collect_owned(&self) -> Vec<TokenOwned> {
        self.iter()
            .map(|t| TokenOwned {
                vocab_id: t.vocab_id,
                text: self.text[t.byte_start as usize..t.byte_end as usize].to_string(),
            })
            .collect()
    }
}

pub struct TokenIter<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> TokenIter<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            pos: 0,
        }
    }
}

impl<'a> Iterator for TokenIter<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        // Skip non-token bytes (anything not ASCII-alphanumeric or '-' or '_').
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if is_token_byte(b) {
                break;
            }
            self.pos += 1;
        }
        if self.pos >= self.bytes.len() {
            return None;
        }
        let start = self.pos;
        while self.pos < self.bytes.len() && is_token_byte(self.bytes[self.pos]) {
            self.pos += 1;
        }
        let end = self.pos;
        // Lowercase the token bytes for vocab-id derivation.
        let mut buf = [0u8; 64];
        let n = (end - start).min(64);
        for i in 0..n {
            buf[i] = self.bytes[start + i].to_ascii_lowercase();
        }
        let vocab_id = vocab_id_for_bytes(&buf[..n]);
        Some(Token {
            vocab_id,
            byte_start: start as u32,
            byte_end: end as u32,
        })
    }
}

fn is_token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// Public: vocab-id for a known &str (lowercased). Used by codebook builder.
pub fn vocab_id_for(text: &str) -> u32 {
    let mut buf = [0u8; 64];
    let bytes = text.as_bytes();
    let n = bytes.len().min(64);
    for i in 0..n {
        buf[i] = bytes[i].to_ascii_lowercase();
    }
    vocab_id_for_bytes(&buf[..n])
}

fn vocab_id_for_bytes(bytes: &[u8]) -> u32 {
    let mut h = blake3::Hasher::new();
    h.update(b"intent-vocab-v1");
    h.update(bytes);
    let d: [u8; 32] = h.finalize().into();
    let v = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
    v % crate::TEXT_TOKEN_VOCAB
}
