#![forbid(unsafe_code)]
#![doc = "cssl-toy-cli — minimal S-expression frontend for the foundation crates.\n\n\
Grammar (recursive S-exp) :\n\
  term     := atom | list\n\
  atom     := identifier | `()`\n\
  list     := `(` head args `)`\n\
  head     := `lam` <ident> <grade> body\n\
            | `let` <ident> <grade> value body\n\
            | `app` fn arg\n\
            | `op` <label>\n\
  grade    := `L` | `A` | `W`     (linear / affine / unrestricted-ω)\n\
  identifier = `[A-Za-z_][A-Za-z0-9_-]*`\n\n\
Examples :\n\
  `()`                              → unit\n\
  `(lam x L x)`                     → linear identity\n\
  `(app (lam x W x) (op io))`       → effectful application\n\
  `(let x W (op io) (op state))`    → sequenced effects\n\n\
This is NOT the production parser (that's a separate Logos/Chumsky frontend) — it \
exists to give the foundation crates an end-to-end driver."]

use cssl_elab::{Grade, Term};

/// Parse error.
#[derive(Debug, PartialEq, Eq)]
pub struct ParseError {
    pub msg: String,
    pub at: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse error at offset {} : {}", self.at, self.msg)
    }
}

impl std::error::Error for ParseError {}

/// Parse a complete S-expression source into a `Term`.
pub fn parse(src: &str) -> Result<Term, ParseError> {
    let tokens = tokenize(src)?;
    let mut p = Parser { tokens, pos: 0 };
    let t = p.parse_term()?;
    if p.pos != p.tokens.len() {
        return Err(ParseError {
            msg: "trailing tokens after term".into(),
            at: p.tokens[p.pos].at,
        });
    }
    Ok(t)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    LParen,
    RParen,
    Ident(String),
}

#[derive(Clone, Debug)]
struct Token {
    tok: Tok,
    at: usize,
}

fn tokenize(src: &str) -> Result<Vec<Token>, ParseError> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b' ' | b'\t' | b'\n' | b'\r' => { i += 1; }
            b'(' => { out.push(Token { tok: Tok::LParen, at: i }); i += 1; }
            b')' => { out.push(Token { tok: Tok::RParen, at: i }); i += 1; }
            b';' => { while i < bytes.len() && bytes[i] != b'\n' { i += 1; } }
            _ if is_ident_start(c) => {
                let start = i;
                while i < bytes.len() && is_ident_cont(bytes[i]) { i += 1; }
                let s = std::str::from_utf8(&bytes[start..i])
                    .map_err(|_| ParseError { msg: "non-utf8 ident".into(), at: start })?;
                out.push(Token { tok: Tok::Ident(s.into()), at: start });
            }
            _ => return Err(ParseError { msg: format!("unexpected byte 0x{c:02x}"), at: i }),
        }
    }
    Ok(out)
}

fn is_ident_start(c: u8) -> bool { c.is_ascii_alphabetic() || c == b'_' }
fn is_ident_cont(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> { self.tokens.get(self.pos) }

    fn bump(&mut self) -> Result<Token, ParseError> {
        let t = self.tokens
            .get(self.pos)
            .ok_or_else(|| ParseError { msg: "unexpected end of input".into(), at: usize::MAX })?
            .clone();
        self.pos += 1;
        Ok(t)
    }

    fn expect_ident(&mut self) -> Result<(String, usize), ParseError> {
        let t = self.bump()?;
        match t.tok {
            Tok::Ident(s) => Ok((s, t.at)),
            other => Err(ParseError { msg: format!("expected ident, got {other:?}"), at: t.at }),
        }
    }

    fn expect(&mut self, want: Tok) -> Result<(), ParseError> {
        let t = self.bump()?;
        if t.tok == want { Ok(()) } else {
            Err(ParseError { msg: format!("expected {want:?}, got {:?}", t.tok), at: t.at })
        }
    }

    fn parse_grade(&mut self) -> Result<Grade, ParseError> {
        let (s, at) = self.expect_ident()?;
        match s.as_str() {
            "L" => Ok(Grade::Linear),
            "A" => Ok(Grade::Affine),
            "W" | "ω" => Ok(Grade::Unrestricted),
            _ => Err(ParseError { msg: format!("unknown grade `{s}` (use L | A | W)"), at }),
        }
    }

    fn parse_term(&mut self) -> Result<Term, ParseError> {
        let t = self.peek()
            .ok_or_else(|| ParseError { msg: "expected term".into(), at: usize::MAX })?
            .clone();
        match &t.tok {
            Tok::LParen => self.parse_list(),
            Tok::Ident(_) => {
                let (name, _) = self.expect_ident()?;
                Ok(Term::Var(name))
            }
            Tok::RParen => Err(ParseError { msg: "unexpected `)`".into(), at: t.at }),
        }
    }

    fn parse_list(&mut self) -> Result<Term, ParseError> {
        self.expect(Tok::LParen)?;
        // `()` is unit.
        if let Some(t) = self.peek() {
            if matches!(t.tok, Tok::RParen) {
                self.expect(Tok::RParen)?;
                return Ok(Term::Unit);
            }
        }
        let (head, head_at) = self.expect_ident()?;
        let term = match head.as_str() {
            "lam" => {
                let (param, _) = self.expect_ident()?;
                let grade = self.parse_grade()?;
                let body = self.parse_term()?;
                Term::Lam { param, grade, body: Box::new(body) }
            }
            "let" => {
                let (name, _) = self.expect_ident()?;
                let grade = self.parse_grade()?;
                let value = self.parse_term()?;
                let body = self.parse_term()?;
                Term::Let { name, grade, value: Box::new(value), body: Box::new(body) }
            }
            "app" => {
                let f = self.parse_term()?;
                let x = self.parse_term()?;
                Term::App(Box::new(f), Box::new(x))
            }
            "op" => {
                let (label, _) = self.expect_ident()?;
                Term::Op(label)
            }
            other => return Err(ParseError {
                msg: format!("unknown head `{other}` (expect lam | let | app | op)"),
                at: head_at,
            }),
        };
        self.expect(Tok::RParen)?;
        Ok(term)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_elab::elaborate;

    #[test]
    fn parse_unit() {
        assert_eq!(parse("()").unwrap(), Term::Unit);
    }

    #[test]
    fn parse_var() {
        assert_eq!(parse("x").unwrap(), Term::Var("x".into()));
    }

    #[test]
    fn parse_linear_identity() {
        let t = parse("(lam x L x)").unwrap();
        assert!(matches!(t, Term::Lam { grade: Grade::Linear, .. }));
        elaborate(&t).expect("parsed identity must elaborate");
    }

    #[test]
    fn parse_application_of_op() {
        let t = parse("(app (lam x W x) (op io))").unwrap();
        let e = elaborate(&t).unwrap();
        assert!(e.effects.contains("io"));
    }

    #[test]
    fn parse_let_unrestricted() {
        let t = parse("(let x W (op io) (op state))").unwrap();
        let e = elaborate(&t).unwrap();
        assert!(e.effects.contains("io"));
        assert!(e.effects.contains("state"));
    }

    #[test]
    fn parse_rejects_unknown_head() {
        let err = parse("(foo x)").unwrap_err();
        assert!(err.msg.contains("unknown head"));
    }

    #[test]
    fn parse_rejects_unknown_grade() {
        let err = parse("(lam x Z x)").unwrap_err();
        assert!(err.msg.contains("unknown grade"));
    }

    #[test]
    fn parse_rejects_trailing_tokens() {
        let err = parse("() x").unwrap_err();
        assert!(err.msg.contains("trailing"));
    }

    #[test]
    fn parse_strips_line_comments() {
        let t = parse("; this is a comment\n(lam x W x) ; trailing\n").unwrap();
        assert!(matches!(t, Term::Lam { .. }));
    }

    #[test]
    fn parse_rejects_unmatched_close_paren() {
        let err = parse(")").unwrap_err();
        assert!(err.msg.contains("unexpected `)`"));
    }
}
