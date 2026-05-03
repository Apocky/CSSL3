// cssl-edge/lib/mneme/csl.ts
// MNEME — minimal CSLv3 validator for memory records.
//
// Spec : ../../specs/02_GRAMMAR.csl + 03_MORPH.csl + 14_CSSLv3_BRIDGE.csl §S3-PARSER-REUSE
//
// PURPOSE
// ════════════════════════════════════════════════════════════════════════
// Memories store their canonical form as a CSLv3 string. We need to:
//   (a) reject malformed extractions before they pollute the index, and
//   (b) extract the head-morpheme topic_key for supersession.
//
// At v1 this is a hand-rolled validator that handles the SUBSET of CSLv3
// used by memory records (which is far smaller than full CSLv3). The full
// reference parser lives at parser/ in this repo and ships as parser.exe;
// at v1.1 we'll shell out to `parser cssllint --json` when local-deploys
// support it. See specs/14_CSSLv3_BRIDGE.csl § S3-PARSER-REUSE.
//
// ACCEPTED FORMS
// ────────────────────────────────────────────────────────────────────────
// fact         <subject-path> ⊗ <object-path>             [evidence?]
//              <subject-path> = <value>                   [evidence?]
//              <subject-path>.<attr> = <value>            [evidence?]
// instruction  flow.<name> - { <step-1> → <step-2> → ... }
//              proc.<name> ⊗ <step-list>
// event        <subject-path> @<scope> [evidence] ['t<YYYY-MM-DD>]
//              <verb>.<obj> @<scope> ['t<YYYY-MM-DD>]
// task         <subject-path> ⊗ status.<state> [I> next-step]
//
// Rules :
//   • subject-path : [a-z][a-zA-Z0-9_-]* ('.' [a-z][a-zA-Z0-9_-]*)*
//   • compound operators allowed : . + - ⊗ @
//   • evidence glyphs allowed at any position: ✓ ◐ ○ ✗ ⊘ △ ▽ ‼
//   • modal glyphs allowed (rare in memories) : W! R! M? N! I> Q?
//   • temporal literal :  't YYYY-MM-DD  (avyayibhava-with-suffix)
//   • numeric literal  :  digits, optional decimal, optional unit (no whitespace)
//
// REJECTED FORMS (anti-pattern detection) :
//   • English connective tokens : "the", "is", "was", "are", "and" outside +
//   • Trailing punctuation that is not a glyph (.,!? at end)
//   • Compound operator with whitespace on both sides except → ←
//   • Empty paths or unbalanced braces / brackets

import type { MemoryType } from './types';

// ── Diagnostic shape ───────────────────────────────────────────────────
export interface CslDiag {
    code: string;
    msg:  string;
    pos?: number;
}

export interface CslValidateResult {
    ok:        boolean;
    diags:     CslDiag[];
    topic_key: string | null;          // extracted head morpheme path
    head_op:   '.' | '+' | '-' | '⊗' | '@' | null;
}

// ── Glyph + token tables ───────────────────────────────────────────────
const COMPOUND_OPS = new Set(['.', '+', '-', '⊗', '@']);
const EVIDENCE     = new Set(['✓', '◐', '○', '✗', '⊘', '△', '▽', '‼']);
const MODALS       = new Set(['W!', 'R!', 'M?', 'N!', 'I>', 'Q?', 'P>', 'D>']);
const FLOW_OPS     = new Set(['→', '->', '←', '<-', '↔', '<->', '⇒', '=>', '|>', '~>']);

// English connective words that must NOT appear in a CSL memory (anti-pattern).
const ENGLISH_NOISE = new Set([
    'the', 'is', 'was', 'are', 'were', 'be', 'been', 'being',
    'a', 'an', 'of', 'on', 'in', 'to', 'for', 'with', 'by',
    'and', 'or', 'but', 'if', 'then', 'than',
    'has', 'have', 'had',
]);

// ── Path validator ─────────────────────────────────────────────────────
const PATH_SEGMENT = /^[a-z][a-zA-Z0-9_-]*$/;

function isValidPathSegment(s: string): boolean {
    return PATH_SEGMENT.test(s);
}

function isValidPath(s: string): boolean {
    if (s.length === 0) return false;
    const parts = s.split('.');
    return parts.every(isValidPathSegment);
}

// Extract the head morpheme path from a CSL string.
// The "head" is the leftmost dotted-path before the first non-dot compound operator.
// Examples :
//   "user.pref.pkg-mgr ⊗ pnpm"        → "user.pref.pkg-mgr"
//   "deploy.chaos-tarot @prod 't2026-04-30" → "deploy.chaos-tarot"
//   "flow.compact-context - {...}"    → "flow.compact-context"
//   "user.theme = dark"               → "user.theme"
export function extractTopicKey(csl: string): string | null {
    const trimmed = csl.trim();
    // Walk forward collecting [a-z][a-zA-Z0-9_-]* and '.' chars.
    let i = 0;
    let head = '';
    while (i < trimmed.length) {
        const c = trimmed[i]!;
        if ((c >= 'a' && c <= 'z') ||
            (c >= 'A' && c <= 'Z') ||
            (c >= '0' && c <= '9') ||
            c === '_' || c === '-' || c === '.') {
            head += c;
            i++;
        } else {
            break;
        }
    }
    head = head.replace(/\.+$/, '');     // strip trailing dots if any
    if (head.length === 0) return null;
    if (!isValidPath(head)) return null;
    return head;
}

// ── Tokeniser (lightweight) ────────────────────────────────────────────
// Splits on whitespace and on compound-op boundaries while preserving the ops.
function tokenise(csl: string): string[] {
    const out: string[] = [];
    const n = csl.length;
    let buf = '';
    const flush = () => { if (buf.length > 0) { out.push(buf); buf = ''; } };

    for (let i = 0; i < n; i++) {
        const c = csl[i]!;
        // Whitespace
        if (c === ' ' || c === '\t' || c === '\n') {
            flush();
            continue;
        }
        // Multichar flow ops first
        if (i + 1 < n) {
            const di = csl.slice(i, i + 2);
            if (di === '->' || di === '<-' || di === '=>' || di === '|>' || di === '~>') {
                flush(); out.push(di); i++; continue;
            }
            // Modal markers are 2-char (W! R! M? N! I> Q? P> D>)
            if (MODALS.has(di)) { flush(); out.push(di); i++; continue; }
        }
        if (i + 2 < n && csl.slice(i, i + 3) === '<->') {
            flush(); out.push('<->'); i += 2; continue;
        }
        // Single-char compound op
        if (COMPOUND_OPS.has(c)) {
            // '-' embedded in a kebab path stays in buf if surrounded by alnum.
            // We disambiguate: if buf currently ends in alnum and next char is alnum,
            // treat '-' as part of the path token; else treat as compound op.
            if (c === '-') {
                const prev = buf.length > 0 ? buf[buf.length - 1] : '';
                const next = i + 1 < n ? csl[i + 1] : '';
                const prevAlnum = !!prev && /[a-zA-Z0-9_]/.test(prev);
                const nextAlnum = !!next && /[a-zA-Z0-9_]/.test(next);
                if (prevAlnum && nextAlnum) { buf += c; continue; }
            }
            // '.' embedded in a path keeps in buf (like dotted segments).
            if (c === '.') {
                const prev = buf.length > 0 ? buf[buf.length - 1] : '';
                const next = i + 1 < n ? csl[i + 1] : '';
                const prevAlnum = !!prev && /[a-zA-Z0-9_-]/.test(prev);
                const nextAlnum = !!next && /[a-zA-Z0-9_]/.test(next);
                if (prevAlnum && nextAlnum) { buf += c; continue; }
            }
            flush(); out.push(c); continue;
        }
        // Evidence glyph stands alone
        if (EVIDENCE.has(c)) { flush(); out.push(c); continue; }
        // Flow arrows (single-char unicode forms)
        if (c === '→' || c === '←' || c === '↔' || c === '⇒') {
            flush(); out.push(c); continue;
        }
        // Otherwise add to current token buffer
        buf += c;
    }
    flush();
    return out;
}

// ── Validator ──────────────────────────────────────────────────────────
export function validateCsl(csl: string, _expectedType?: MemoryType): CslValidateResult {
    const diags: CslDiag[] = [];
    const trimmed = csl.trim();

    if (trimmed.length === 0) {
        return { ok: false, diags: [{ code: 'CSL-E-EMPTY', msg: 'empty CSL string' }],
                 topic_key: null, head_op: null };
    }
    if (trimmed.length > 4096) {
        diags.push({ code: 'CSL-E-LEN', msg: 'CSL string exceeds 4096 chars' });
    }

    // 1. Balanced brackets / braces
    let depthBrace = 0, depthBrack = 0, depthParen = 0;
    for (const c of trimmed) {
        if (c === '{') depthBrace++;
        else if (c === '}') depthBrace--;
        else if (c === '[') depthBrack++;
        else if (c === ']') depthBrack--;
        else if (c === '(') depthParen++;
        else if (c === ')') depthParen--;
        if (depthBrace < 0 || depthBrack < 0 || depthParen < 0) {
            diags.push({ code: 'CSL-E-BRACKET', msg: 'unbalanced bracket/brace' });
            break;
        }
    }
    if (depthBrace !== 0 || depthBrack !== 0 || depthParen !== 0) {
        diags.push({ code: 'CSL-E-BRACKET', msg: 'unclosed bracket/brace at end' });
    }

    // 2. Tokenise + scan for English noise + structure checks
    const tokens = tokenise(trimmed);
    if (tokens.length === 0) {
        diags.push({ code: 'CSL-E-EMPTY', msg: 'no tokens after tokenisation' });
    }

    // First token must be a valid path or a known prefix glyph (§).
    const first = tokens[0] ?? '';
    let head_op: CslValidateResult['head_op'] = null;

    // Find head op = first compound operator that follows a valid path token.
    let topic_key: string | null = null;
    if (first === '§' || first === 'def' || first === 'fn') {
        // Unusual: spec block embedded as memory. Discouraged but not invalid.
        diags.push({ code: 'CSL-W-SPECBLOCK', msg: 'memory CSL begins with §/def/fn — discouraged' });
    } else if (isValidPath(first)) {
        topic_key = first;
        // Scan for next non-whitespace, non-evidence token to determine head_op.
        for (let i = 1; i < tokens.length; i++) {
            const t = tokens[i]!;
            if (EVIDENCE.has(t)) continue;
            if (MODALS.has(t)) continue;
            if (COMPOUND_OPS.has(t)) {
                head_op = t as CslValidateResult['head_op'];
                break;
            }
            // Plain word after path — treat as object/value (implicit '⊗' or '=' allowed).
            if (t === '=' || t === ':' || t === '::') {
                head_op = '.';   // identity-binding, treat as tatpurusha for topic-key
                break;
            }
            // Any other token after the path means we've exited the head.
            break;
        }
    } else {
        diags.push({
            code: 'CSL-E-PATH',
            msg: `first token "${first}" is not a valid morpheme path`,
        });
    }

    // 3. English-noise scan (anti-pattern)
    for (const t of tokens) {
        if (ENGLISH_NOISE.has(t.toLowerCase())) {
            diags.push({
                code: 'CSL-W-EN-NOISE',
                msg: `English filler word "${t}" — paraphrase, do not store as CSL`,
            });
            break;     // one warning per record is enough
        }
    }

    // 4. Trailing-punctuation anti-pattern
    if (/[.,!?]$/.test(trimmed) && !trimmed.endsWith('!') /* W!/N! */) {
        // Allow exclamation/question if it's part of a modal at end.
        const last2 = trimmed.slice(-2);
        if (!MODALS.has(last2)) {
            diags.push({ code: 'CSL-W-PUNCT', msg: 'trailing English punctuation' });
        }
    }

    // 5. No isolated single-char words other than known glyphs
    for (const t of tokens) {
        if (t.length === 1 && !COMPOUND_OPS.has(t) && !EVIDENCE.has(t) &&
            !'§=:→←↔⇒'.includes(t) && !/[a-zA-Z0-9]/.test(t)) {
            diags.push({ code: 'CSL-W-GLYPH', msg: `unknown single-char token "${t}"` });
            break;
        }
    }

    const errors = diags.filter(d => d.code.startsWith('CSL-E-'));
    return {
        ok: errors.length === 0,
        diags,
        topic_key,
        head_op,
    };
}

// ── Helpers used elsewhere in lib/mneme ────────────────────────────────

// Compose the embedding text from CSL + paraphrase + search queries.
// Spec : 44_MNEME_PIPELINES.csl § INGEST § STAGE-8.
export function composeEmbeddingText(
    csl: string,
    paraphrase: string,
    queries: string[],
): string {
    const parts: string[] = [csl, paraphrase];
    for (const q of queries) {
        const t = q.trim();
        if (t.length > 0) parts.push(t);
    }
    return parts.join('\n');
}

// Compose an FTS or-query from a list of terms, escaping each.
export function composeTsQuery(terms: string[]): string {
    return terms
        .map(t => t.replace(/[^a-zA-Z0-9_-]+/g, ' ').trim())
        .filter(t => t.length > 0)
        .map(t => t.replace(/\s+/g, ' & '))
        .join(' | ');
}
