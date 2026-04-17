//! Surface mode detection per `specs/16_DUAL_SURFACE.csl` § MODE-DETECTION.
//!
//! § DETECTION ORDER (first match wins)
//!   1. File extension :
//!        `.cssl-csl`  → `Surface::CslNative`
//!        `.cssl-rust` → `Surface::RustHybrid`
//!        `.cssl`      → fall through to step 2
//!   2. Pragma on first non-comment, non-blank line :
//!        `#![surface = "csl"]`  → `Surface::CslNative`
//!        `#![surface = "rust"]` → `Surface::RustHybrid`
//!   3. First-non-comment-line heuristic :
//!        starts with `§` + glyph              → `Surface::CslNative`
//!        starts with `module | use | fn | struct | enum | pub` → `Surface::RustHybrid`
//!   4. Default : `Surface::RustHybrid` (per §§ 16 "ambiguous : default Rust-hybrid,
//!      warn + recommend explicit") — call-site is expected to surface a `Warning`.
//!
//! § NON-GOAL
//!   Mixed-mode within a single file is forbidden (§§ 16) and produces a diagnostic
//!   at lex time; this module does not validate that — parser layer handles it.

use cssl_ast::Surface;

/// Outcome of a mode-detection pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detection {
    /// The surface the file should be lexed as.
    pub surface: Surface,
    /// Reason the surface was chosen (used for diagnostic framing).
    pub reason: Reason,
}

/// Why a particular surface was selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// File extension matched `.cssl-csl` or `.cssl-rust`.
    Extension,
    /// A `#![surface = "…"]` pragma fixed the surface.
    Pragma,
    /// First non-comment line matched a surface heuristic (`§` vs Rust keyword).
    FirstLine,
    /// Nothing matched; default `RustHybrid` chosen. Call-site should warn.
    Default,
}

/// Detect the surface of a source file from its filename + contents.
///
/// `filename` is the (logical) path — only its extension matters. `contents` is the full
/// UTF-8 text. The function never reads the filesystem.
#[must_use]
pub fn detect(filename: &str, contents: &str) -> Detection {
    if filename.ends_with(".cssl-csl") {
        return Detection {
            surface: Surface::CslNative,
            reason: Reason::Extension,
        };
    }
    if filename.ends_with(".cssl-rust") {
        return Detection {
            surface: Surface::RustHybrid,
            reason: Reason::Extension,
        };
    }
    if let Some(surface) = detect_pragma(contents) {
        return Detection {
            surface,
            reason: Reason::Pragma,
        };
    }
    if let Some(surface) = detect_first_line(contents) {
        return Detection {
            surface,
            reason: Reason::FirstLine,
        };
    }
    Detection {
        surface: Surface::RustHybrid,
        reason: Reason::Default,
    }
}

/// Scan leading comment / blank lines for a `#![surface = "…"]` pragma.
///
/// Recognises both ASCII `"csl"` / `"rust"` and the full spec forms.
/// Returns `None` if no pragma is found in the first ~5 non-blank lines.
fn detect_pragma(contents: &str) -> Option<Surface> {
    for (i, line) in contents.lines().enumerate() {
        if i > 8 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment_line(trimmed) {
            continue;
        }
        // pragma must be one of the first couple of real lines
        if trimmed.starts_with("#![surface") {
            if trimmed.contains("\"csl\"") || trimmed.contains("\"csl-native\"") {
                return Some(Surface::CslNative);
            }
            if trimmed.contains("\"rust\"") || trimmed.contains("\"rust-hybrid\"") {
                return Some(Surface::RustHybrid);
            }
        } else {
            // non-pragma non-comment : stop looking for pragma
            break;
        }
    }
    None
}

/// Scan the first non-blank, non-comment line for a surface signal.
fn detect_first_line(contents: &str) -> Option<Surface> {
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment_line(trimmed) {
            continue;
        }
        // CSLv3-native starts with § or §§ followed by something
        if trimmed.starts_with('§') {
            return Some(Surface::CslNative);
        }
        // Rust-hybrid item-level keywords (see `specs/09_SYNTAX.csl` RUST-HYBRID SURFACE)
        for kw in RUST_HYBRID_ITEM_KEYWORDS {
            if has_prefix_word(trimmed, kw) {
                return Some(Surface::RustHybrid);
            }
        }
        // first real line found but didn't match either heuristic
        return None;
    }
    None
}

/// Keywords that unambiguously identify Rust-hybrid at item level.
const RUST_HYBRID_ITEM_KEYWORDS: &[&str] = &[
    "module",
    "use",
    "fn",
    "struct",
    "enum",
    "pub",
    "interface",
    "impl",
    "type",
    "const",
    "effect",
    "handler",
];

/// Is `line` a recognized comment line (for surface-agnostic skipping)?
fn is_comment_line(line: &str) -> bool {
    line.starts_with("//")
        || line.starts_with("/*")
        || line.starts_with('#') && !line.starts_with("#!")
}

/// `true` iff `s` starts with the exact word `word` followed by non-identifier-continuation
/// (or EOL). Avoids matching `functional` when looking for `fn`.
fn has_prefix_word(s: &str, word: &str) -> bool {
    if !s.starts_with(word) {
        return false;
    }
    match s.as_bytes().get(word.len()) {
        None => true,
        Some(&b) => !b.is_ascii_alphanumeric() && b != b'_',
    }
}

#[cfg(test)]
mod tests {
    use super::{detect, Reason};
    use cssl_ast::Surface;

    // ─── extension-driven ────────────────────────────────────────────────────

    #[test]
    fn ext_cssl_csl_wins() {
        let d = detect("foo.cssl-csl", "fn bar() {}");
        assert_eq!(d.surface, Surface::CslNative);
        assert_eq!(d.reason, Reason::Extension);
    }

    #[test]
    fn ext_cssl_rust_wins() {
        let d = detect("foo.cssl-rust", "§ prose");
        assert_eq!(d.surface, Surface::RustHybrid);
        assert_eq!(d.reason, Reason::Extension);
    }

    #[test]
    fn ext_cssl_neutral_falls_through() {
        let d = detect("foo.cssl", "fn f() {}");
        // no explicit extension, heuristic kicks in → RustHybrid via keyword
        assert_eq!(d.surface, Surface::RustHybrid);
        assert_eq!(d.reason, Reason::FirstLine);
    }

    // ─── pragma-driven ───────────────────────────────────────────────────────

    #[test]
    fn pragma_csl_wins() {
        let d = detect("any.cssl", "#![surface = \"csl\"]\nfn f() {}");
        assert_eq!(d.surface, Surface::CslNative);
        assert_eq!(d.reason, Reason::Pragma);
    }

    #[test]
    fn pragma_rust_wins() {
        let d = detect("any.cssl", "#![surface = \"rust\"]\n§ prose");
        assert_eq!(d.surface, Surface::RustHybrid);
        assert_eq!(d.reason, Reason::Pragma);
    }

    #[test]
    fn pragma_full_form_csl_native() {
        let d = detect("any.cssl", "#![surface = \"csl-native\"]\nfoo");
        assert_eq!(d.surface, Surface::CslNative);
        assert_eq!(d.reason, Reason::Pragma);
    }

    #[test]
    fn pragma_through_leading_comments() {
        let src = "// banner comment\n// copyright\n\n#![surface = \"csl\"]\nfoo";
        let d = detect("any.cssl", src);
        assert_eq!(d.surface, Surface::CslNative);
        assert_eq!(d.reason, Reason::Pragma);
    }

    // ─── first-line heuristic ────────────────────────────────────────────────

    #[test]
    fn section_glyph_chooses_csl_native() {
        let d = detect("any.cssl", "§ prose\n  more");
        assert_eq!(d.surface, Surface::CslNative);
        assert_eq!(d.reason, Reason::FirstLine);
    }

    #[test]
    fn fn_keyword_chooses_rust_hybrid() {
        let d = detect("any.cssl", "fn foo() {}\n");
        assert_eq!(d.surface, Surface::RustHybrid);
        assert_eq!(d.reason, Reason::FirstLine);
    }

    #[test]
    fn module_keyword_chooses_rust_hybrid() {
        let d = detect("any.cssl", "module com.apocky.foo");
        assert_eq!(d.surface, Surface::RustHybrid);
        assert_eq!(d.reason, Reason::FirstLine);
    }

    #[test]
    fn use_statement_chooses_rust_hybrid() {
        let d = detect("any.cssl", "use std::math::vec3\n");
        assert_eq!(d.surface, Surface::RustHybrid);
        assert_eq!(d.reason, Reason::FirstLine);
    }

    #[test]
    fn prefix_word_avoids_partial_match() {
        // `functional` should NOT trigger `fn` keyword detection
        let d = detect("any.cssl", "functional_tests_go_here();\n");
        assert_eq!(d.surface, Surface::RustHybrid);
        assert_eq!(d.reason, Reason::Default);
    }

    // ─── default fallback ────────────────────────────────────────────────────

    #[test]
    fn empty_file_defaults_to_rust_hybrid() {
        let d = detect("any.cssl", "");
        assert_eq!(d.surface, Surface::RustHybrid);
        assert_eq!(d.reason, Reason::Default);
    }

    #[test]
    fn all_comments_default_to_rust_hybrid() {
        let d = detect("any.cssl", "// just a comment\n// another\n");
        assert_eq!(d.surface, Surface::RustHybrid);
        assert_eq!(d.reason, Reason::Default);
    }

    #[test]
    fn ambiguous_expression_line_defaults_rust_hybrid() {
        let d = detect("any.cssl", "some_expression(1 + 2);\n");
        assert_eq!(d.surface, Surface::RustHybrid);
        assert_eq!(d.reason, Reason::Default);
    }

    // ─── leading whitespace tolerance ────────────────────────────────────────

    #[test]
    fn leading_whitespace_before_first_line() {
        let d = detect("any.cssl", "\n\n   fn foo() {}\n");
        assert_eq!(d.surface, Surface::RustHybrid);
        assert_eq!(d.reason, Reason::FirstLine);
    }

    #[test]
    fn leading_whitespace_before_section_glyph() {
        let d = detect("any.cssl", "\n  § prose\n");
        assert_eq!(d.surface, Surface::CslNative);
        assert_eq!(d.reason, Reason::FirstLine);
    }
}
