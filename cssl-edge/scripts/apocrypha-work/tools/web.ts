/**
 * Web access for Apocrypha: `web_search` and `web_fetch`.
 *
 * WHY THESE EXIST. Apocrypha had eleven tools -- file operations, a shell, and the dials -- and no
 * way to look anything up. Its MCP hub carries fs, aegis and palworld-doctor, none of which reach
 * the network. A local model whose weights were frozen months ago and which cannot read a page is
 * guessing about the present BY CONSTRUCTION, and it cannot tell that it is guessing.
 *
 * The concrete case that prompted this: asked which small coding model was best "as of today",
 * I answered by searching. Apocrypha could not have. It would have answered confidently from
 * whatever was true when its weights were baked, with no signal that anything had changed.
 *
 * It does have `run_command`, so in principle it could curl. In practice that means it invents an
 * ad-hoc scraping pipeline per question, with no size cap, no timeout, no redirect policy and no
 * protection against reaching back into the operator's own machine. Doing it once, properly, is
 * the difference between a capability and a hazard.
 *
 * WHAT MAKES THIS SAFE, and each of these is load-bearing:
 *
 *   SSRF IS THE REAL RISK, not the fetching. A tool that takes a URL from a model and fetches it
 *   can be pointed at 127.0.0.1, at 169.254.169.254, or at anything on the LAN. This host runs an
 *   unauthenticated engine on 19128, a room on 19123 and a work app on 19130; "fetch this URL" is
 *   otherwise a way to read them. So: http(s) only, and the resolved address is checked against
 *   loopback, link-local, and every private range BEFORE the request and AGAIN after each
 *   redirect, because a public hostname can redirect to a private one.
 *
 *   BOUNDED. A byte cap, a timeout, and a redirect limit. An unbounded fetch is a way to fill the
 *   context with one page, or to hang the turn.
 *
 *   HONEST FAILURE. Every failure returns a REASON the model can act on -- refused, too large,
 *   timed out, blocked as private -- never an empty string. The same rule the engine client
 *   follows: silence must say which kind of silence it is, or the model treats a dead tool and an
 *   empty answer as the same fact.
 *
 * SEARCH NEEDS AN API KEY, and that is a measured finding rather than a preference. Every keyless
 * endpoint was tried on 2026-09-20 and every one now blocks automated requests:
 *     html.duckduckgo.com   14 KB, zero result markup (challenge page)
 *     lite.duckduckgo.com   HTTP 202, challenge
 *     mojeek.com            341 bytes for a real query
 *     searx.be              HTTP 200, 11 KB, zero result markup
 *     four other SearXNG instances   403, 429, 429, connection refused
 * So `web_search` uses a provider key when one is configured, tries the keyless endpoint only as
 * a last resort, and when neither works it says EXACTLY what to set rather than returning an
 * empty list. An empty list would be read as "the web has nothing on this", which is a lie.
 *
 * Set any ONE of these in the work environment and search starts working:
 *     BRAVE_SEARCH_API_KEY    api.search.brave.com, generous free tier
 *     SERPER_API_KEY          serper.dev
 *     TAVILY_API_KEY          tavily.com, built for agents
 */
import { lookup } from 'node:dns/promises';
import { isIP } from 'node:net';

import type { ToolDefinition } from '../types';
import type { ToolResult } from './files';

/** A page larger than this is being read for the wrong reason. */
const MAX_BYTES = 512 * 1024;
/** Characters of extracted text handed back. Beyond this it is not reading, it is flooding. */
const MAX_TEXT = 24_000;
const MAX_REDIRECTS = 4;
const DEFAULT_TIMEOUT_MS = 20_000;

export const WEB_TOOLS: ToolDefinition[] = [
  {
    name: 'web_search',
    risk: 'read',
    description:
      'Search the web and get titles, URLs and snippets. Use this whenever the answer depends on '
      + 'anything current -- releases, versions, prices, news, benchmarks, or "as of today". Your '
      + 'training data has a cutoff and this is the only way to see past it. Prefer searching over '
      + 'answering from memory when the question is about the present.',
    parameters: {
      type: 'object',
      properties: {
        query: { type: 'string', description: 'What to search for.' },
        max_results: { type: 'integer', minimum: 1, maximum: 15, description: 'Default 8.' },
      },
      required: ['query'],
    },
  },
  {
    name: 'web_fetch',
    risk: 'read',
    description:
      'Fetch one http(s) page and return its readable text. Use it on a URL from web_search to '
      + 'read the source rather than trusting a snippet. Private and loopback addresses are '
      + 'refused, so this cannot reach services on this machine or the local network.',
    parameters: {
      type: 'object',
      properties: {
        url: { type: 'string', description: 'Absolute http:// or https:// URL.' },
        max_chars: { type: 'integer', minimum: 500, maximum: 24000, description: 'Default 24000.' },
      },
      required: ['url'],
    },
  },
];

export function ownsWebTool(name: string): boolean {
  return name === 'web_search' || name === 'web_fetch';
}

/** Private, loopback, link-local and carrier-grade ranges. Checked on the RESOLVED address. */
function isPrivateAddress(ip: string): boolean {
  if (isIP(ip) === 6) {
    const v = ip.toLowerCase();
    if (v === '::1' || v === '::') return true;
    if (v.startsWith('fe80') || v.startsWith('fc') || v.startsWith('fd')) return true;
    // ::ffff:a.b.c.d — an IPv4 address wearing an IPv6 coat, and an easy way past a naive check.
    const mapped = v.match(/^::ffff:(\d+\.\d+\.\d+\.\d+)$/);
    return mapped && mapped[1] ? isPrivateAddress(mapped[1]) : false;
  }
  const p = ip.split('.').map(Number);
  if (p.length !== 4 || p.some((n) => Number.isNaN(n))) return true;   // unparseable: refuse
  const a = p[0] as number;
  const b = p[1] as number;
  if (a === 127 || a === 0 || a === 10) return true;
  if (a === 169 && b === 254) return true;               // link-local, incl. cloud metadata
  if (a === 172 && b >= 16 && b <= 31) return true;
  if (a === 192 && b === 168) return true;
  if (a === 100 && b >= 64 && b <= 127) return true;     // carrier-grade NAT
  return false;
}

/** '' when the URL is allowed; otherwise why it is refused. */
async function refuse(raw: string): Promise<string> {
  let u: URL;
  try {
    u = new URL(raw);
  } catch {
    return `not a valid URL: ${raw.slice(0, 120)}`;
  }
  if (u.protocol !== 'http:' && u.protocol !== 'https:') {
    return `only http and https are allowed, not ${u.protocol}`;
  }
  const host = u.hostname.replace(/^\[|\]$/g, '');
  if (isIP(host) && isPrivateAddress(host)) {
    return `refused: ${host} is a private or loopback address`;
  }
  if (!isIP(host)) {
    if (!host.includes('.') || host.toLowerCase() === 'localhost') {
      return `refused: ${host} is not a public hostname`;
    }
    try {
      const resolved = await lookup(host, { all: true });
      if (resolved.length === 0) return `refused: ${host} did not resolve`;
      const bad = resolved.find((r) => isPrivateAddress(r.address));
      if (bad) return `refused: ${host} resolves to the private address ${bad.address}`;
    } catch (err) {
      return `refused: ${host} did not resolve (${(err as Error).message})`;
    }
  }
  return '';
}

/** Fetch with our own redirect loop, so every hop is re-checked against the private ranges. */
async function guardedFetch(url: string, signal: AbortSignal, accept: string) {
  let current = url;
  for (let hop = 0; hop <= MAX_REDIRECTS; hop += 1) {
    const why = await refuse(current);
    if (why) return { error: hop === 0 ? why : `${why} (after a redirect from ${url})` };
    const res = await fetch(current, {
      redirect: 'manual',
      signal,
      headers: { accept, 'user-agent': 'Apocrypha-Work/1.0 (+local agent)' },
    });
    if (res.status >= 300 && res.status < 400) {
      const next = res.headers.get('location');
      if (!next) return { error: `redirect with no location, status ${res.status}` };
      current = new URL(next, current).toString();
      continue;
    }
    return { res, finalUrl: current };
  }
  return { error: `more than ${MAX_REDIRECTS} redirects starting at ${url}` };
}

async function readCapped(res: Response): Promise<{ body: string; truncated: boolean }> {
  const reader = res.body?.getReader();
  if (!reader) return { body: await res.text(), truncated: false };
  const chunks: Uint8Array[] = [];
  let total = 0;
  let truncated = false;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    if (value) {
      total += value.byteLength;
      if (total > MAX_BYTES) {
        chunks.push(value.slice(0, Math.max(0, value.byteLength - (total - MAX_BYTES))));
        truncated = true;
        await reader.cancel();
        break;
      }
      chunks.push(value);
    }
  }
  return { body: Buffer.concat(chunks.map((c) => Buffer.from(c))).toString('utf8'), truncated };
}

/** Strip a page to something worth reading. Not a parser; a de-noiser. */
export function toText(html: string): string {
  return html
    .replace(/<script[\s\S]*?<\/script>/gi, ' ')
    .replace(/<style[\s\S]*?<\/style>/gi, ' ')
    .replace(/<noscript[\s\S]*?<\/noscript>/gi, ' ')
    .replace(/<svg[\s\S]*?<\/svg>/gi, ' ')
    .replace(/<!--[\s\S]*?-->/g, ' ')
    .replace(/<\/(p|div|h[1-6]|li|tr|section|article|br)>/gi, '\n')
    .replace(/<[^>]+>/g, ' ')
    .replace(/&nbsp;/gi, ' ').replace(/&amp;/gi, '&').replace(/&lt;/gi, '<')
    .replace(/&gt;/gi, '>').replace(/&quot;/gi, '"').replace(/&#39;/gi, "'")
    .replace(/[ \t ]+/g, ' ')
    .replace(/\n\s*\n\s*\n+/g, '\n\n')
    .trim();
}

export interface SearchHit { title: string; url: string; snippet: string }

function formatHits(source: string, query: string, hits: SearchHit[]): ToolResult {
  const lines = hits.map((h, i) => `${i + 1}. ${h.title}\n   ${h.url}\n   ${h.snippet}`);
  return {
    summary: `web_search (${source}): ${hits.length} result(s) for ${query.slice(0, 60)}`,
    content: `${lines.join('\n\n')}\n\nUse web_fetch on a URL to read the source rather than trusting a snippet.`,
  };
}

async function asJson(url: string, init: RequestInit): Promise<any> {
  const res = await fetch(url, init);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

/** Keyed providers, tried in order. Each returns [] rather than throwing on a well-formed miss. */
const PROVIDERS: {
  name: string;
  env: string;
  run: (q: string, max: number, key: string, signal: AbortSignal) => Promise<SearchHit[]>;
}[] = [
  {
    name: 'brave',
    env: 'BRAVE_SEARCH_API_KEY',
    run: async (q, max, key, signal) => {
      const d = await asJson(
        `https://api.search.brave.com/res/v1/web/search?q=${encodeURIComponent(q)}&count=${max}`,
        { signal, headers: { accept: 'application/json', 'x-subscription-token': key } });
      return (d?.web?.results ?? []).slice(0, max).map((r: any) => ({
        title: String(r.title ?? ''), url: String(r.url ?? ''),
        snippet: toText(String(r.description ?? '')).slice(0, 300),
      })).filter((h: SearchHit) => h.url);
    },
  },
  {
    name: 'serper',
    env: 'SERPER_API_KEY',
    run: async (q, max, key, signal) => {
      const d = await asJson('https://google.serper.dev/search', {
        method: 'POST', signal,
        headers: { 'content-type': 'application/json', 'X-API-KEY': key },
        body: JSON.stringify({ q, num: max }),
      });
      return (d?.organic ?? []).slice(0, max).map((r: any) => ({
        title: String(r.title ?? ''), url: String(r.link ?? ''),
        snippet: String(r.snippet ?? '').slice(0, 300),
      })).filter((h: SearchHit) => h.url);
    },
  },
  {
    name: 'tavily',
    env: 'TAVILY_API_KEY',
    run: async (q, max, key, signal) => {
      const d = await asJson('https://api.tavily.com/search', {
        method: 'POST', signal,
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ api_key: key, query: q, max_results: max }),
      });
      return (d?.results ?? []).slice(0, max).map((r: any) => ({
        title: String(r.title ?? ''), url: String(r.url ?? ''),
        snippet: String(r.content ?? '').slice(0, 300),
      })).filter((h: SearchHit) => h.url);
    },
  },
];

/** Parse DuckDuckGo's HTML result page. Zero hits is reported, never returned as success. */
export function parseDuckDuckGo(html: string, max: number): SearchHit[] {
  const out: SearchHit[] = [];
  const blocks = html.split(/class="result__body"/i).slice(1);
  for (const block of blocks) {
    const link = block.match(/href="([^"]+)"[^>]*class="result__a"/i)
      || block.match(/class="result__a"[^>]*href="([^"]+)"/i);
    if (!link || !link[1]) continue;
    let url = link[1].replace(/&amp;/g, '&');
    // DuckDuckGo wraps results in /l/?uddg=<encoded>. Unwrap so the model gets the real URL.
    const wrapped = url.match(/[?&]uddg=([^&]+)/);
    if (wrapped && wrapped[1]) {
      try { url = decodeURIComponent(wrapped[1]); } catch { /* keep the wrapper URL */ }
    }
    if (!/^https?:\/\//i.test(url)) continue;
    const titleRaw = block.match(/class="result__a"[^>]*>([\s\S]*?)<\/a>/i);
    const snipRaw = block.match(/class="result__snippet"[^>]*>([\s\S]*?)<\/a>/i)
      || block.match(/class="result__snippet"[^>]*>([\s\S]*?)<\/div>/i);
    out.push({
      title: titleRaw && titleRaw[1] ? toText(titleRaw[1]).slice(0, 180) : '(no title)',
      url,
      snippet: snipRaw && snipRaw[1] ? toText(snipRaw[1]).slice(0, 300) : '',
    });
    if (out.length >= max) break;
  }
  return out;
}

export async function runWebTool(
  name: string,
  args: Record<string, unknown>,
  signal: AbortSignal,
): Promise<ToolResult> {
  if (name === 'web_search') {
    const query = String(args.query ?? '').trim();
    if (!query) return { summary: 'web_search: no query', content: 'A query is required.' };
    const max = Math.min(15, Math.max(1, Number(args.max_results) || 8));
    const both = AbortSignal.any([signal, AbortSignal.timeout(DEFAULT_TIMEOUT_MS)]);
    const tried: string[] = [];

    for (const provider of PROVIDERS) {
      const key = process.env[provider.env];
      if (!key) continue;
      try {
        const hits = await provider.run(query, max, key, both);
        if (hits.length > 0) return formatHits(provider.name, query, hits);
        tried.push(`${provider.name}: 0 results`);
      } catch (err) {
        tried.push(`${provider.name}: ${(err as Error).message.slice(0, 90)}`);
      }
    }

    // Last resort. Recorded as blocked on 2026-09-20; kept because it costs one request and
    // these endpoints come back.
    try {
      const got = await guardedFetch(
        `https://html.duckduckgo.com/html/?q=${encodeURIComponent(query)}`, both, 'text/html');
      if (!('error' in got) && got.res?.ok) {
        const { body } = await readCapped(got.res);
        const hits = parseDuckDuckGo(body, max);
        if (hits.length > 0) return formatHits('duckduckgo', query, hits);
        tried.push('duckduckgo: 0 results parsed (challenge page)');
      } else {
        tried.push(`duckduckgo: ${('error' in got && got.error) || got.res?.status}`);
      }
    } catch (err) {
      tried.push(`duckduckgo: ${(err as Error).message.slice(0, 90)}`);
    }

    const configured = PROVIDERS.filter((p) => process.env[p.env]).map((p) => p.name);
    return {
      summary: 'web_search: no search backend available',
      content:
        'The search did NOT run. This is a missing capability, not an empty result -- do not '
        + 'conclude anything about what is on the web.\n\n'
        + `Attempts: ${tried.join(' | ') || 'none'}\n`
        + `Provider keys configured: ${configured.length ? configured.join(', ') : 'NONE'}\n\n`
        + 'To enable search, set ONE of these in the work environment and restart:\n'
        + PROVIDERS.map((p) => `  ${p.env}   (${p.name})`).join('\n')
        + '\n\nweb_fetch still works: if you already know a URL, read it directly.',
    };
  }

  if (name === 'web_fetch') {
    const raw = String(args.url ?? '').trim();
    if (!raw) return { summary: 'web_fetch: no url', content: 'A url is required.' };
    const cap = Math.min(MAX_TEXT, Math.max(500, Number(args.max_chars) || MAX_TEXT));
    const timer = AbortSignal.timeout(DEFAULT_TIMEOUT_MS);
    const both = AbortSignal.any([signal, timer]);
    try {
      const got = await guardedFetch(raw, both, 'text/html,text/plain,application/json');
      if ('error' in got || !got.res) {
        return { summary: 'web_fetch refused', content: String(('error' in got && got.error) || 'no response') };
      }
      const { res, finalUrl } = got;
      if (!res.ok) {
        return { summary: `web_fetch HTTP ${res.status}`, content: `${finalUrl} answered ${res.status} ${res.statusText}.` };
      }
      const { body, truncated } = await readCapped(res);
      const type = res.headers.get('content-type') || '';
      const text = /json|text\/plain/i.test(type) ? body : toText(body);
      const clipped = text.length > cap;
      const note = [
        truncated ? `the download was capped at ${MAX_BYTES} bytes` : '',
        clipped ? `the text was clipped to ${cap} characters` : '',
      ].filter(Boolean).join('; ');
      return {
        summary: `web_fetch: ${finalUrl.slice(0, 90)} (${text.length} chars)`,
        content: `${finalUrl}\n\n${text.slice(0, cap)}${note ? `\n\n[${note}]` : ''}`,
        truncated: truncated || clipped,
      };
    } catch (err) {
      const e = err as Error;
      const why = e.name === 'TimeoutError' || e.name === 'AbortError'
        ? `no answer within ${DEFAULT_TIMEOUT_MS / 1000}s`
        : e.message;
      return { summary: 'web_fetch failed', content: `${raw} could not be fetched: ${why}` };
    }
  }

  return { summary: `unknown web tool ${name}`, content: `No such tool: ${name}` };
}
