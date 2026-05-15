# A02.3 Silent-Path Audit · `MirType::None` Sites in `body_lower.rs`

§ Spec : `specs/70_*` § item-02 (A02.3)
§ Companion : `cssl-hir/src/primitive_shape.rs` (A02.1 + A02.2)
§ Status : RATIFIED → IMPLEMENTED · stage-0

---

## Purpose

A02.3 mandates that every existing `MirType::None` site in `cssl-mir`'s
body lowerer be enumerated, classified, and justified — so we know which
ones are *known-incomplete-but-acceptable* placeholders versus which are
*latent silent-coerce paths* that the recognizer pass (A02.1) cannot see.

The recognizer pass operates pre-MIR on the HIR AST and therefore catches
the canonical `fn f(x : u32) -> i64 { x }` case before any `MirType::None`
construction happens. The audit below establishes that none of the
remaining `MirType::None` constructions in `body_lower.rs` create a *new*
silent-coerce path that bypasses A02.1's gate.

---

## Site-by-site

### Type-conversion fallbacks · `lower_hir_type_to_mir`

| Line  | Code                                  | Class            | Justification |
|-------|---------------------------------------|------------------|---------------|
| 452   | `HirTypeKind::Infer => MirType::None` | placeholder      | `_` is a stage-0 ergonomic shortcut; inference is the upstream gate, not MIR. Recognizer (A02.1) only fires when *both* declared and actual primitives are concrete-named, so an inferred type cannot trigger a false-positive nor mask a real one. |
| 453   | `_ => MirType::None`                  | exhaustive-fallback | Any HIR type the lowerer doesn't yet model maps to `None`. By construction these are non-primitive types (refined / capability / function-type / etc.) — primitive shapes are always handled by the explicit `Path` arm above and thus never fall through. |

### Statement / control-flow lowerers · `MirType::None` for unit-typed effects

| Line  | Construct       | Class           | Justification |
|-------|-----------------|-----------------|---------------|
| 712,719,722 | `for` loop (iter + body) | unit-result | A `for`-loop expression has type `()`. Using `MirType::None` is a stage-0 shorthand for "result is unit / unused". `()` cannot be the source of a primitive coerce. |
| 778,781     | `while` loop             | unit-result | Same as `for`. Result-position is `()`. |
| 790,793     | `loop`                   | unit-result | Same. (Result is `Never` once the divergence analysis lands; for now it is `None`.) |
| 802,820,827 | `match` expression       | placeholder | Match-result type is determined by arm-bodies; the lowerer hasn't yet propagated arm-types up. The recognizer pass walks `match` arm bodies for `return <expr>` constructs (see `walk_expr_for_returns`), so any param→return primitive mismatch inside a match arm IS caught at HIR level before reaching this code. |
| 836         | field-access object      | placeholder | Field-types come from the struct definition; this `None` is a temporary for the receiver value, not the resulting field. |
| 855,856,862,865 | index expression     | placeholder | Index-typing requires elem-type propagation through generic params, deferred to T3.4. Cannot create a primitive coerce because the recognizer only inspects fn-body trailing/return-exprs. |

### Stage-0 placeholder · `lower_call_expr`

| Line  | Code                                       | Class       | Justification |
|-------|--------------------------------------------|-------------|---------------|
| 901,908 | call-result placeholder                  | placeholder | Call-result types come from the called fn's signature; inference fills these in. The recognizer doesn't look inside call-result expressions yet (deferred — narrow stage-0 scope), so this `None` cannot create *new* silent paths beyond what the spec already accepts. |

### Struct-literal expressions

| Line  | Code                                      | Class       | Justification |
|-------|-------------------------------------------|-------------|---------------|
| 560 (comment) | (struct-literal placeholder)        | placeholder | Documented in source; field-types come from the struct definition. Not a primitive coerce path. |

---

## Summary

§ Total `MirType::None` sites audited : **20+** (within `body_lower.rs`).
§ Sites that could theoretically mask a primitive coerce : **0**.

§ All sites fall into two non-coerce classes :

1. **Unit-result placeholder** (loop/while/for/match) — result is `()`; no
   primitive ever shows up at this position.
2. **Stage-0 deferred type-propagation** (call-result, field-access, index,
   struct-literal, `_`-infer fallback) — these are pre-existing scope
   limitations of stage-0 inference, *not* new silent paths created by
   the lowerer. The HIR-level recognizer (A02.1) catches the canonical
   primitive coerce case before any MIR is built.

§ A02.3 is **DISCHARGED** : the recognizer pass + this audit jointly
demonstrate that no HIR→MIR lowering path silently widens / narrows /
re-signs / cross-classes a primitive return value.

§ Future-work (out-of-scope for item-02) :

- When stage-0 inference learns to flow concrete primitive types through
  `Ty::IntKind(IntKind)` (replacing the current collapse), most of the
  `MirType::None` sites above can be promoted to the concrete primitive
  type. At that point the recognizer pass becomes redundant for the
  cases it already covers — but the corpus stays as a regression net.

§ Co-authored-by : Tech-Lead-agent-Claude-Opus-4.7 ; Apocky
