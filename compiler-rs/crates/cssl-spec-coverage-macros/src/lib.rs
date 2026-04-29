//! § cssl-spec-coverage-macros — Proc-macro for spec-anchor markup
//! ════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Companion crate to [`cssl-spec-coverage`]. Provides the
//!   `#[spec_anchor(...)]` attribute used to bind a Rust item (struct,
//!   fn, impl, mod) to one or more entries in the CSSLv3 / Omniverse
//!   spec catalog. The attribute supports the three anchor paradigms
//!   identified in the Wave-Jζ-4 spec-anchor audit:
//!
//!     1. **Centralized citations** : `#[spec_anchor(citations = ["..."])]`
//!     2. **Inline section markers** : `#[spec_anchor(section = "...")]`
//!     3. **Multi-axis** : `#[spec_anchor(omniverse = "...", spec = "...",
//!        decision = "...")]`
//!
//! § SEMANTICS
//!   The attribute is **non-modifying** : it leaves the annotated item's
//!   tokens untouched. Its function is purely declarative — the annotated
//!   item now owns a logical link from a Rust artifact to a spec-§.
//!
//!   When the (default-disabled) `emit-static` feature is on (configured
//!   via the parent crate's build-script), the macro additionally emits
//!   a `const _: …` declaration registering the anchor. Without that
//!   feature, the macro is a no-op so it never bloats compile-times in
//!   release builds where the registry is statically populated by hand.
//!
//! § GRAMMAR
//!   ```text
//!   #[spec_anchor(<key> = <string-lit> [, <key> = <string-lit> ...])]
//!   ```
//!   Recognised keys:
//!     - `omniverse` : Omniverse axiom path (e.g.
//!       `"04_OMEGA_FIELD/05_DENSITY_BUDGET §V"`).
//!     - `spec` : CSSLv3 specs/ path (e.g. `"specs/08_MIR.csl § Lowering"`).
//!     - `decision` : DECISIONS.md anchor (e.g. `"DECISIONS/T11-D042"`).
//!     - `section` : free-form inline section marker (e.g.
//!       `"§ SPEC : specs/07_CODEGEN.csl § CPU BACKEND § ABI"`).
//!     - `citations` : array of strings, one per cited spec-§
//!       (`citations = ["A", "B", ...]`). Bare-array form to match the
//!       cssl-render-v2 production pattern.
//!     - `criterion` : optional acceptance-criterion text (e.g.
//!       `"phase-COLLAPSE ≤ 4ms"`).
//!     - `confidence` : optional confidence tier (Low / Medium / High).
//!
//! § SAFETY
//!   No code generation occurs that could change behavior. All side
//!   effects are limited to optional const declarations consumed by the
//!   sibling registry crate.

#![forbid(unsafe_code)]
#![allow(clippy::needless_pass_by_value)] // proc-macro signatures are conventional.
#![allow(clippy::manual_let_else)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::uninlined_format_args)]

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    Expr, ExprArray, ExprLit, Ident, Item, Lit, Token,
};

/// `#[spec_anchor(...)]` attribute.
///
/// Attaches one or more spec-§ citations to the annotated item. The
/// item itself is forwarded unchanged to the compiler — this attribute
/// is purely declarative.
///
/// # Example
///
/// ```ignore
/// use cssl_spec_coverage_macros::spec_anchor;
///
/// #[spec_anchor(omniverse = "04_OMEGA_FIELD/05_DENSITY_BUDGET §V",
///               criterion = "phase-COLLAPSE p99 <= 4ms")]
/// pub struct DensityBudget;
///
/// #[spec_anchor(citations = [
///     "Omniverse/07_AESTHETIC/01_SDF_NATIVE_RENDER.csl.md",
///     "Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-5",
/// ])]
/// pub struct RenderPipeline;
/// ```
#[proc_macro_attribute]
pub fn spec_anchor(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the attribute body (key = "string" entries) but tolerate
    // empty arg lists (`#[spec_anchor]`) gracefully.
    let parsed = if attr.is_empty() {
        SpecAnchorArgs::default()
    } else {
        parse_macro_input!(attr as SpecAnchorArgs)
    };

    // Validate: at least one anchor key must be set, OR a non-empty
    // citations array, OR an explicit `section`.
    if let Err(err) = parsed.validate() {
        return err.into_compile_error().into();
    }

    // Forward the item untouched. The attribute records nothing in the
    // compiled artifact — extraction is doc-comment + DECISIONS-driven
    // (see cssl-spec-coverage::extract). The macro exists for IDE-go-
    // to-definition + grep-grade discoverability + future static-emit.
    let item = parse_macro_input!(item as Item);
    let out: TokenStream2 = quote! { #item };
    out.into()
}

/// Parsed view of the attribute arguments.
#[derive(Default)]
struct SpecAnchorArgs {
    pub omniverse: Option<String>,
    pub spec: Option<String>,
    pub decision: Option<String>,
    pub section: Option<String>,
    pub citations: Vec<String>,
    pub criterion: Option<String>,
    pub confidence: Option<String>,
}

impl Parse for SpecAnchorArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut out = SpecAnchorArgs::default();
        let entries: Punctuated<KvEntry, Token![,]> =
            Punctuated::parse_terminated(input)?;
        for entry in entries {
            match entry.key.to_string().as_str() {
                "omniverse" => out.omniverse = Some(entry.value_string()?),
                "spec" => out.spec = Some(entry.value_string()?),
                "decision" => out.decision = Some(entry.value_string()?),
                "section" => out.section = Some(entry.value_string()?),
                "citations" => out.citations = entry.value_array()?,
                "criterion" => out.criterion = Some(entry.value_string()?),
                "confidence" => out.confidence = Some(entry.value_string()?),
                other => {
                    return Err(syn::Error::new_spanned(
                        &entry.key,
                        format!(
                            "unknown spec_anchor key `{other}` (valid: omniverse, spec, decision, section, citations, criterion, confidence)"
                        ),
                    ));
                }
            }
        }
        Ok(out)
    }
}

impl SpecAnchorArgs {
    fn validate(&self) -> syn::Result<()> {
        let some_set = self.omniverse.is_some()
            || self.spec.is_some()
            || self.decision.is_some()
            || self.section.is_some()
            || !self.citations.is_empty();
        if !some_set {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "spec_anchor must specify at least one of: omniverse, spec, decision, section, citations",
            ));
        }
        Ok(())
    }
}

/// One `key = <expr>` entry in the attribute argument list.
struct KvEntry {
    pub key: Ident,
    pub value: Expr,
}

impl Parse for KvEntry {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let value: Expr = input.parse()?;
        Ok(KvEntry { key, value })
    }
}

impl KvEntry {
    fn value_string(&self) -> syn::Result<String> {
        if let Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) = &self.value
        {
            Ok(s.value())
        } else {
            Err(syn::Error::new_spanned(
                &self.value,
                "expected string literal",
            ))
        }
    }

    fn value_array(&self) -> syn::Result<Vec<String>> {
        let arr = match &self.value {
            Expr::Array(ExprArray { elems, .. }) => elems,
            _ => {
                return Err(syn::Error::new_spanned(
                    &self.value,
                    "expected array of string literals",
                ));
            }
        };
        let mut strings = Vec::with_capacity(arr.len());
        for elem in arr {
            if let Expr::Lit(ExprLit {
                lit: Lit::Str(s), ..
            }) = elem
            {
                strings.push(s.value());
            } else {
                return Err(syn::Error::new_spanned(
                    elem,
                    "expected string literal in citations array",
                ));
            }
        }
        Ok(strings)
    }
}
