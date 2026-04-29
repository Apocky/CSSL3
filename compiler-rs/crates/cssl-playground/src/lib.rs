//! CSSLv3 browser playground — WASM-bindgen wrapper.
//!
//! § PIPELINE EXPOSED
//!   source-string → lex → parse (CST) → HIR-lower → type-check → JSON result
//!
//! § WASM TARGETS
//!   wasm32-unknown-unknown   (wasm-pack / bundler)
//!   wasm32-wasi              (wasmtime / node; not primary)
//!
//! § WHY NO CODEGEN
//!   cranelift-jit requires mmap + executable-memory pages — impossible in the
//!   WASM sandbox.  The entire parse→HIR chain has no such deps.
//!   See compiler-rs/crates/cssl-playground/WASM_BLOCKERS.md for the full tally.
//!
//! § SPEC
//!   Playground is a developer tool, not a runtime.  It surfaces diagnostics,
//!   item counts, and type-check errors.  Evaluation / execution is out-of-scope
//!   until a `cssl-eval` interpreter crate lands.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

use wasm_bindgen::prelude::*;

use cssl_ast::{source::Surface, Diagnostic, SourceFile, SourceId};
use cssl_hir::{check_module, lower_module, HirItem, Interner};

// ── panic hook ─────────────────────────────────────────────────────────────

/// Install `console_error_panic_hook` so Rust panics surface in the browser
/// dev-tools console rather than disappearing silently.
#[wasm_bindgen(start)]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

// ── public WASM API ────────────────────────────────────────────────────────

/// Run the parse → HIR → type-check pipeline on `source` and return a JSON string.
///
/// `surface_hint`:
///   - `"rust"` → Rust-hybrid surface
///   - `"csl"`  → CSLv3-native surface
///   - anything else → auto-detect
///
/// Returned JSON shape:
/// ```json
/// {
///   "ok": bool,
///   "token_count": usize,
///   "diagnostics": [{ "severity": "error"|"warning"|"note"|"help",
///                      "message": str,
///                      "span": { "start": u32, "end": u32 } | null,
///                      "notes": [str] }],
///   "item_count": usize,
///   "items": [{ "kind": str, "name": str | null }],
///   "stages": { "lex": bool, "parse": bool, "hir": bool, "typecheck": bool }
/// }
/// ```
#[wasm_bindgen]
pub fn compile(source: String, surface_hint: &str) -> String {
    let surface = match surface_hint {
        "csl" => Surface::CslNative,
        "rust" => Surface::RustHybrid,
        _ => Surface::Auto,
    };

    let src = SourceFile::new(SourceId::first(), "playground.cssl", source, surface);

    // ── lex ────────────────────────────────────────────────────────────────
    let tokens = cssl_lex::lex(&src);
    let token_count = tokens.len();

    // ── parse ──────────────────────────────────────────────────────────────
    let (cst_module, parse_bag) = cssl_parse::parse(&src, &tokens);
    let parse_ok = !parse_bag.has_errors();

    // ── HIR lower + resolve ────────────────────────────────────────────────
    let (hir_module, interner, hir_bag) = lower_module(&src, &cst_module);
    let hir_ok = !hir_bag.has_errors();

    // ── type-check ─────────────────────────────────────────────────────────
    let (_type_map, tc_diags) = check_module(&hir_module, &interner);
    let tc_ok = tc_diags.iter().all(|d| !d.severity.is_error());

    // ── collect all diagnostics ────────────────────────────────────────────
    let mut diags_json: Vec<serde_json::Value> = Vec::new();
    for d in parse_bag
        .iter()
        .chain(hir_bag.iter())
        .chain(tc_diags.iter())
    {
        diags_json.push(diag_to_json(d, &src));
    }

    // ── summarise HIR items ────────────────────────────────────────────────
    let items_json: Vec<serde_json::Value> = hir_module
        .items
        .iter()
        .map(|item| item_to_json(item, &interner))
        .collect();

    let result = serde_json::json!({
        "ok": parse_ok && hir_ok && tc_ok,
        "token_count": token_count,
        "diagnostics": diags_json,
        "item_count": items_json.len(),
        "items": items_json,
        "stages": {
            "lex": true,
            "parse": parse_ok,
            "hir": hir_ok,
            "typecheck": tc_ok,
        }
    });

    result.to_string()
}

/// Return the playground's WASM build version string.
#[wasm_bindgen]
pub fn playground_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ── helpers ────────────────────────────────────────────────────────────────

fn diag_to_json(d: &Diagnostic, src: &SourceFile) -> serde_json::Value {
    let span_val = d.span.map(|sp| {
        let loc = src.position_of(sp.start);
        serde_json::json!({
            "start": sp.start,
            "end": sp.end,
            "line": loc.line.get(),
            "col": loc.column.get(),
        })
    });
    let note_strs: Vec<&str> = d.notes.iter().map(|n| n.message.as_str()).collect();
    serde_json::json!({
        "severity": d.severity.label(),
        "message": d.message,
        "span": span_val,
        "notes": note_strs,
    })
}

fn item_to_json(item: &HirItem, interner: &Interner) -> serde_json::Value {
    let kind = item_kind_name(item);
    let name = item.name().map(|sym| interner.resolve(sym));
    serde_json::json!({ "kind": kind, "name": name })
}

fn item_kind_name(item: &HirItem) -> &'static str {
    match item {
        HirItem::Fn(_) => "fn",
        HirItem::Struct(_) => "struct",
        HirItem::Enum(_) => "enum",
        HirItem::Interface(_) => "interface",
        HirItem::Impl(_) => "impl",
        HirItem::Effect(_) => "effect",
        HirItem::Handler(_) => "handler",
        HirItem::TypeAlias(_) => "type",
        HirItem::Use(_) => "use",
        HirItem::Const(_) => "const",
        HirItem::Module(_) => "module",
    }
}

// ── host tests (cargo test --lib on native, not WASM target) ───────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> serde_json::Value {
        let json_str = compile(src.to_string(), "auto");
        serde_json::from_str(&json_str).expect("compile() must return valid JSON")
    }

    #[test]
    fn empty_source_no_crash() {
        let v = run("");
        assert!(v["stages"]["lex"].as_bool().unwrap());
    }

    #[test]
    fn simple_fn_parses() {
        let src = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let v = run(src);
        assert!(
            v["ok"].as_bool().unwrap(),
            "diagnostics: {}",
            v["diagnostics"]
        );
        assert_eq!(v["item_count"].as_u64().unwrap(), 1);
        assert_eq!(v["items"][0]["kind"].as_str().unwrap(), "fn");
        assert_eq!(v["items"][0]["name"].as_str().unwrap(), "add");
    }

    #[test]
    fn struct_decl_parses() {
        let src = "struct Point { x: f32, y: f32 }";
        let v = run(src);
        assert!(
            v["ok"].as_bool().unwrap(),
            "diagnostics: {}",
            v["diagnostics"]
        );
        assert_eq!(v["items"][0]["kind"].as_str().unwrap(), "struct");
        assert_eq!(v["items"][0]["name"].as_str().unwrap(), "Point");
    }

    #[test]
    fn syntax_error_produces_diagnostic() {
        let src = "fn broken(";
        let v = run(src);
        assert!(!v["ok"].as_bool().unwrap());
        assert!(
            v["diagnostics"]
                .as_array()
                .map(|a| !a.is_empty())
                .unwrap_or(false),
            "expected at least one diagnostic for syntax error"
        );
    }

    #[test]
    fn json_always_valid() {
        for src in &["", "!!!@@@###", "fn", "struct {}", "42"] {
            let json_str = compile(src.to_string(), "auto");
            serde_json::from_str::<serde_json::Value>(&json_str)
                .unwrap_or_else(|e| panic!("invalid JSON for input {src:?}: {e}"));
        }
    }

    #[test]
    fn surface_hints_accepted() {
        let src = "fn f() {}";
        for hint in &["auto", "rust", "csl", "unknown", ""] {
            let json_str = compile(src.to_string(), hint);
            serde_json::from_str::<serde_json::Value>(&json_str)
                .unwrap_or_else(|e| panic!("surface={hint:?} produced invalid JSON: {e}"));
        }
    }
}
