// cssl-edge · components/CodeBlock.tsx
// Fenced-code block with simple CSSL syntax highlighting (manual span coloring).
// Zero dependencies on syntax-highlight libs. Tokenizer recognizes:
//   keywords · type-keywords · CSL3 glyphs · numbers · strings · comments
// "lang" prop selects coloring rules · 'cssl' | 'rust' | 'bash' | 'plain'.

import type { ReactNode } from 'react';

const CSSL_KEYWORDS = new Set([
  'module', 'fn', 'let', 'mut', 'extern', 'struct', 'enum', 'if', 'else',
  'match', 'return', 'use', 'pub', 'self', 'loop', 'while', 'for', 'in',
  'true', 'false', 'as', 'break', 'continue', 'const',
]);

const CSSL_TYPE_KEYWORDS = new Set([
  'i8', 'i16', 'i32', 'i64', 'i128', 'u8', 'u16', 'u32', 'u64', 'u128',
  'f32', 'f64', 'bool', 'char', 'str', 'String', 'Vec', 'Option', 'Result',
]);

const RUST_KEYWORDS = new Set([
  ...CSSL_KEYWORDS,
  'impl', 'trait', 'where', 'type', 'mod', 'crate', 'super', 'static',
  'unsafe', 'async', 'await', 'move', 'ref', 'dyn', 'box',
]);

const GLYPH_RE = /[§¬→≤≥⊑⊔∀∃∈⊆⇒∴∵⟨⟩⌈⌉⟦⟧«»⟪⟫✓◐○✗⊘△▽‼W!R!I>Q?M?N!⊗·∞]/;

interface CodeBlockProps {
  /** The code source. Newlines preserved as-is. */
  children: string;
  /** Language for highlighting. Defaults to 'plain'. */
  lang?: 'cssl' | 'rust' | 'bash' | 'plain';
  /** Optional caption rendered above the block. */
  caption?: string;
}

const COLORS = {
  keyword: '#c084fc',
  typeKw: '#7dd3fc',
  glyph: '#fbbf24',
  number: '#f472b6',
  string: '#a7f3d0',
  comment: '#5a5a6a',
  ident: '#cdd6e4',
  punct: '#7a7a8c',
};

/**
 * Tokenize one logical "word" + emit colored span. Preserves whitespace
 * exactly; only word/number/glyph runs are colored.
 */
function highlightLine(line: string, kwSet: Set<string>): ReactNode[] {
  const out: ReactNode[] = [];
  let i = 0;
  let key = 0;
  // line-comment: emit rest as comment
  const commentIdx = line.indexOf('//');
  let codePart = line;
  let commentPart = '';
  if (commentIdx >= 0) {
    codePart = line.slice(0, commentIdx);
    commentPart = line.slice(commentIdx);
  }
  while (i < codePart.length) {
    const c = codePart[i] ?? '';
    // whitespace passthrough
    if (c === ' ' || c === '\t') {
      let ws = '';
      while (i < codePart.length && (codePart[i] === ' ' || codePart[i] === '\t')) {
        ws += codePart[i];
        i++;
      }
      out.push(ws);
      continue;
    }
    // string literal
    if (c === '"') {
      let s = c;
      i++;
      while (i < codePart.length && codePart[i] !== '"') {
        const cur = codePart[i] ?? '';
        const nxt = codePart[i + 1] ?? '';
        if (cur === '\\' && i + 1 < codePart.length) {
          s += cur + nxt;
          i += 2;
          continue;
        }
        s += cur;
        i++;
      }
      if (i < codePart.length) {
        s += codePart[i] ?? '';
        i++;
      }
      out.push(<span key={key++} style={{ color: COLORS.string }}>{s}</span>);
      continue;
    }
    // number
    if (/[0-9]/.test(c)) {
      let n = '';
      while (i < codePart.length && /[0-9._a-fxA-FX]/.test(codePart[i] ?? '')) {
        n += codePart[i];
        i++;
      }
      out.push(<span key={key++} style={{ color: COLORS.number }}>{n}</span>);
      continue;
    }
    // identifier / keyword
    if (/[A-Za-z_]/.test(c)) {
      let id = '';
      while (i < codePart.length && /[A-Za-z0-9_]/.test(codePart[i] ?? '')) {
        id += codePart[i];
        i++;
      }
      if (kwSet.has(id)) {
        out.push(<span key={key++} style={{ color: COLORS.keyword }}>{id}</span>);
      } else if (CSSL_TYPE_KEYWORDS.has(id)) {
        out.push(<span key={key++} style={{ color: COLORS.typeKw }}>{id}</span>);
      } else {
        out.push(<span key={key++} style={{ color: COLORS.ident }}>{id}</span>);
      }
      continue;
    }
    // glyph (CSL3 + status)
    if (GLYPH_RE.test(c)) {
      out.push(<span key={key++} style={{ color: COLORS.glyph }}>{c}</span>);
      i++;
      continue;
    }
    // punctuation passthrough
    out.push(<span key={key++} style={{ color: COLORS.punct }}>{c}</span>);
    i++;
  }
  if (commentPart !== '') {
    out.push(<span key={key++} style={{ color: COLORS.comment }}>{commentPart}</span>);
  }
  return out;
}

const CodeBlock = ({ children, lang = 'plain', caption }: CodeBlockProps) => {
  const kwSet = lang === 'cssl' ? CSSL_KEYWORDS : lang === 'rust' ? RUST_KEYWORDS : new Set<string>();
  const lines = children.replace(/\n$/, '').split('\n');
  return (
    <div style={{ margin: '1.1rem 0' }}>
      {caption !== undefined && caption !== '' ? (
        <div style={{ fontSize: '0.7rem', color: '#7a7a8c', letterSpacing: '0.08em', marginBottom: '0.3rem', textTransform: 'uppercase' }}>
          § {caption}
        </div>
      ) : null}
      <pre
        style={{
          background: 'rgba(15, 15, 25, 0.7)',
          border: '1px solid #1f1f2a',
          borderRadius: 6,
          padding: '0.95rem 1.1rem',
          fontSize: '0.8rem',
          lineHeight: 1.55,
          overflowX: 'auto',
          margin: 0,
          fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
        }}
      >
        <code>
          {lines.map((line, idx) => (
            <div key={idx}>{highlightLine(line, kwSet)}</div>
          ))}
        </code>
      </pre>
    </div>
  );
};

export default CodeBlock;
