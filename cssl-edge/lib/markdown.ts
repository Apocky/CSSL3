// cssl-edge · lib/markdown.ts
// Minimal markdown → HTML transformer. ZERO npm-deps · runs at SSG-time.
// Supports a controlled subset : headings (h1-h3) · paragraphs · ul/ol ·
// inline-code · code-blocks · bold · italic · links · §-glyph-aware classes.
//
// NOT a fully-spec-compliant CommonMark parser — by design. Just enough to
// render the devblog posts authored as plain markdown strings.
//
// Output is HTML-string. Caller injects via dangerouslySetInnerHTML.
// All inputs are STATIC (build-time devblog posts) so XSS surface is bounded
// to authored content; we still escape angle-brackets in code-blocks.

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function escapeAttr(s: string): string {
  return escapeHtml(s).replace(/'/g, '&#39;');
}

interface RenderState {
  out: string[];
  inList: 'ul' | 'ol' | null;
  inCode: boolean;
  codeLang: string;
  codeBuf: string[];
}

function flushList(st: RenderState): void {
  if (st.inList !== null) {
    st.out.push(`</${st.inList}>`);
    st.inList = null;
  }
}

function renderInline(s: string): string {
  // Order matters : code first (greedy), then bold, then italic, then links.
  let out = escapeHtml(s);
  // `inline code`
  out = out.replace(/`([^`]+)`/g, (_m, c) => `<code class="md-code">${c}</code>`);
  // **bold**
  out = out.replace(/\*\*([^*]+)\*\*/g, (_m, t) => `<strong>${t}</strong>`);
  // *italic* — keep simple; we don't author with mid-word emphasis
  out = out.replace(/(?<![A-Za-z0-9])\*([^*\n]+)\*(?![A-Za-z0-9])/g, (_m, t) => `<em>${t}</em>`);
  // [text](href) — http(s) only
  out = out.replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, (_m, text, href) => {
    return `<a href="${escapeAttr(href as string)}" target="_blank" rel="noopener noreferrer">${text}</a>`;
  });
  return out;
}

export function markdownToHtml(input: string): string {
  const lines = input.split('\n');
  const st: RenderState = { out: [], inList: null, inCode: false, codeLang: '', codeBuf: [] };

  for (const rawLine of lines) {
    const line = rawLine.replace(/\r$/, '');

    // ── code-fence open/close ────────────────────────────────────────────
    if (line.startsWith('```')) {
      if (st.inCode) {
        // close
        const langClass = st.codeLang.length > 0 ? ` class="md-code-block lang-${escapeAttr(st.codeLang)}"` : ' class="md-code-block"';
        st.out.push(`<pre${langClass}><code>${escapeHtml(st.codeBuf.join('\n'))}</code></pre>`);
        st.inCode = false;
        st.codeLang = '';
        st.codeBuf = [];
      } else {
        flushList(st);
        st.inCode = true;
        st.codeLang = line.slice(3).trim();
      }
      continue;
    }
    if (st.inCode) {
      st.codeBuf.push(line);
      continue;
    }

    // ── headings ─────────────────────────────────────────────────────────
    if (line.startsWith('### ')) {
      flushList(st);
      st.out.push(`<h3 class="md-h3">${renderInline(line.slice(4))}</h3>`);
      continue;
    }
    if (line.startsWith('## ')) {
      flushList(st);
      st.out.push(`<h2 class="md-h2">${renderInline(line.slice(3))}</h2>`);
      continue;
    }
    if (line.startsWith('# ')) {
      flushList(st);
      st.out.push(`<h1 class="md-h1">${renderInline(line.slice(2))}</h1>`);
      continue;
    }

    // ── lists ────────────────────────────────────────────────────────────
    const ulMatch = line.match(/^- (.+)$/);
    if (ulMatch !== null) {
      if (st.inList !== 'ul') {
        flushList(st);
        st.out.push('<ul class="md-ul">');
        st.inList = 'ul';
      }
      st.out.push(`<li>${renderInline(ulMatch[1] as string)}</li>`);
      continue;
    }
    const olMatch = line.match(/^\d+\. (.+)$/);
    if (olMatch !== null) {
      if (st.inList !== 'ol') {
        flushList(st);
        st.out.push('<ol class="md-ol">');
        st.inList = 'ol';
      }
      st.out.push(`<li>${renderInline(olMatch[1] as string)}</li>`);
      continue;
    }

    // ── blank line · paragraph break ─────────────────────────────────────
    if (line.trim().length === 0) {
      flushList(st);
      continue;
    }

    // ── default · paragraph ──────────────────────────────────────────────
    flushList(st);
    st.out.push(`<p class="md-p">${renderInline(line)}</p>`);
  }

  flushList(st);
  if (st.inCode) {
    // Unclosed fence — flush so we don't drop content
    st.out.push(`<pre class="md-code-block"><code>${escapeHtml(st.codeBuf.join('\n'))}</code></pre>`);
  }
  return st.out.join('\n');
}

// ─── inline tests · framework-agnostic ───────────────────────────────────

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

export function testMarkdownH1(): void {
  const html = markdownToHtml('# Hello\n');
  assert(html.includes('<h1 class="md-h1">Hello</h1>'), 'h1 must render');
}

export function testMarkdownList(): void {
  const html = markdownToHtml('- one\n- two\n');
  assert(html.includes('<ul class="md-ul">'), 'ul opens');
  assert(html.includes('<li>one</li>'), 'first li');
  assert(html.includes('<li>two</li>'), 'second li');
  assert(html.includes('</ul>'), 'ul closes');
}

export function testMarkdownCodeBlock(): void {
  const html = markdownToHtml('```ts\nconst x = 1;\n```\n');
  assert(html.includes('<pre class="md-code-block lang-ts">'), 'lang-class');
  assert(html.includes('const x = 1;'), 'body retained');
}

export function testMarkdownHtmlEscape(): void {
  const html = markdownToHtml('# <script>alert(1)</script>\n');
  assert(html.includes('&lt;script&gt;'), 'angle-bracket escape');
  assert(!html.includes('<script>'), 'no raw script tag');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  testMarkdownH1();
  testMarkdownList();
  testMarkdownCodeBlock();
  testMarkdownHtmlEscape();
  // eslint-disable-next-line no-console
  console.log('markdown.ts : OK · 4 inline tests passed');
}
