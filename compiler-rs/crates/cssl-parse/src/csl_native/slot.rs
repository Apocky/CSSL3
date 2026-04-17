//! CSLv3-native slot-template recognizer.
//!
//! § SPEC : `CSLv3/specs/13_GRAMMAR_SELF.csl` § SLOT-TEMPLATE :
//!   `[EVIDENCE?] [MODAL?] [DET?] SUBJECT [RELATION] OBJECT [GATE?] [SCOPE?]`
//!
//! § STAGE-0 SCOPE
//!   For T3.2, slot-template decomposition is a recognition-only stub. The parser walks a
//!   best-effort slot sequence and records the span-bounds of each slot; the elaborator in
//!   `cssl-hir` is responsible for the semantic mapping and enforcement. This keeps the CST
//!   light and the grammar authoritative : spec-delta `specs/02_IR.csl` § HIR contents is
//!   queued (per SESSION_1_HANDOFF) for full slot-template elaboration pass listing.

use cssl_ast::Span;
use cssl_lex::{EvidenceMark, ModalOp, Token, TokenKind};

/// Slot-template components recovered from a token span. Each is `Some` only if the
/// corresponding slot was present in the input; elaborator fills defaults.
#[derive(Debug, Clone, Default)]
pub struct SlotTemplate {
    /// Evidence marker slot (`✓ ◐ ○ ✗ ⊘ △ ▽ ‼`).
    pub evidence: Option<(EvidenceMark, Span)>,
    /// Modal slot (`W! R! M? N! I> Q? P> D>`).
    pub modal: Option<(ModalOp, Span)>,
    /// Span covering the subject + relation + object slots (the core triple).
    pub core_span: Option<Span>,
}

/// Best-effort recognition of the optional evidence + modal slots at a line start.
/// Returns the recovered slot-template and a slice index advancement count.
#[must_use]
pub fn recognize_prefix(tokens: &[Token]) -> (SlotTemplate, usize) {
    let mut st = SlotTemplate::default();
    let mut idx = 0;
    if let Some(t) = tokens.get(idx) {
        if let TokenKind::Evidence(mark) = t.kind {
            st.evidence = Some((mark, t.span));
            idx += 1;
        }
    }
    if let Some(t) = tokens.get(idx) {
        if let TokenKind::Modal(m) = t.kind {
            st.modal = Some((m, t.span));
            idx += 1;
        }
    }
    (st, idx)
}

#[cfg(test)]
mod tests {
    use super::recognize_prefix;
    use cssl_ast::{SourceId, Span};
    use cssl_lex::{EvidenceMark, ModalOp, Token, TokenKind};

    fn tok(kind: TokenKind, start: u32, end: u32) -> Token {
        Token::new(kind, Span::new(SourceId::first(), start, end))
    }

    #[test]
    fn no_prefix_slots() {
        let toks = vec![tok(TokenKind::Ident, 0, 3)];
        let (st, adv) = recognize_prefix(&toks);
        assert!(st.evidence.is_none());
        assert!(st.modal.is_none());
        assert_eq!(adv, 0);
    }

    #[test]
    fn evidence_only() {
        let toks = vec![
            tok(TokenKind::Evidence(EvidenceMark::Confirmed), 0, 1),
            tok(TokenKind::Ident, 1, 5),
        ];
        let (st, adv) = recognize_prefix(&toks);
        assert!(st.evidence.is_some());
        assert!(st.modal.is_none());
        assert_eq!(adv, 1);
    }

    #[test]
    fn evidence_then_modal() {
        let toks = vec![
            tok(TokenKind::Evidence(EvidenceMark::Confirmed), 0, 1),
            tok(TokenKind::Modal(ModalOp::Must), 1, 3),
            tok(TokenKind::Ident, 3, 7),
        ];
        let (st, adv) = recognize_prefix(&toks);
        assert!(st.evidence.is_some());
        assert!(st.modal.is_some());
        assert_eq!(adv, 2);
    }

    #[test]
    fn modal_only_without_evidence() {
        let toks = vec![tok(TokenKind::Modal(ModalOp::Insight), 0, 2)];
        let (st, adv) = recognize_prefix(&toks);
        assert!(st.evidence.is_none());
        assert!(st.modal.is_some());
        assert_eq!(adv, 1);
    }
}
