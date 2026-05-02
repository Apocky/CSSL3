// cssl-edge · lib/content-fetch.ts
// W12-6 · UGC-Discover-Browse · client-side fetch helpers + types
//
// Consumes sibling W12-5 publish-API (when wired). Stub-mode-aware:
// every fetch helper distinguishes 404/network-error from real errors,
// so the UI can degrade gracefully.
//
// Sovereignty:
//   - NO engagement-tracking fields collected
//   - NO scroll-depth, time-on-page, or conversion-funnel hooks
//   - Author-revocability glyph data exposed in every shape
//   - "why am I seeing this?" → returns rationale_kind for UI tooltip

export type ContentStatus = 'draft' | 'playtested' | 'published' | 'remixable';

export interface ContentRatingSummary {
  /** Aggregate rating count [0, ∞). NEVER per-user · privacy-default. */
  total_ratings: number;
  /** Mean score [0, 5]. Stub-default = 0. */
  mean_score: number;
  /** Distribution buckets count[1..5]. */
  distribution: ReadonlyArray<number>; // [c1, c2, c3, c4, c5]
}

export interface ContentItem {
  /** URL-safe slug · used in /content/[slug]. */
  slug: string;
  /** Display title authored by creator. */
  title: string;
  /** Author public-key (revocable-cap tag). Display as truncated hex. */
  author_pubkey: string;
  /** Author display-name (optional · falls back to truncated pubkey). */
  author_display?: string;
  /** ISO-8601 published timestamp. */
  published_at: string;
  /** Tag list · UGC-applied + curator-approved. */
  tags: ReadonlyArray<string>;
  /** Aggregate rating summary. */
  rating_summary: ContentRatingSummary;
  /** Lifecycle status. */
  status: ContentStatus;
  /** Short blurb (≤140 chars · feed-card display). */
  blurb: string;
  /** Optional thumbnail URL · stub-mode → undefined. */
  thumbnail_url?: string;
  /** Why-am-I-seeing-this rationale for trending feed. */
  rationale?: ContentRationale;
}

export interface ContentRationale {
  kind: 'kan-bias' | 'curator-pick' | 'subscribed' | 'tagged-by-you' | 'new' | 'remix-of-yours';
  /** Human-readable explanation (CSLv3-tinged). */
  explanation: string;
  /** KAN-axis name when kind === 'kan-bias'. */
  kan_axis?: string;
}

export interface ContentDetail extends ContentItem {
  /** Full description · markdown-flavoured but rendered as plain text in stub. */
  description: string;
  /** Screenshot URL list · stub-mode → []. */
  screenshots: ReadonlyArray<string>;
  /** Install button target (deep-link or download URL). */
  install_url?: string;
  /** Σ-mask attestation : cosmetic-axiom-compliance flag. */
  cosmetic_axiom_attested: boolean;
  /** Remix attribution chain (oldest → newest). */
  attribution_chain: ReadonlyArray<AttributionLink>;
  /** List of remixes (immediate children · slug pointers). */
  remix_slugs: ReadonlyArray<string>;
  /** Σ-mask : revocability-status flag. */
  cap_revocable: boolean;
}

export interface AttributionLink {
  slug: string;
  title: string;
  author_pubkey: string;
  /** Generation index · 0 = original, N = N-th remix. */
  generation: number;
}

export interface ContentListResponse {
  items: ReadonlyArray<ContentItem>;
  /** Cursor for next-page (undefined if no more). */
  next_cursor?: string;
  /** Total count if backend supports it (stub: undefined). */
  total?: number;
}

export interface ContentFetchResult<T> {
  data: T | null;
  /** True when API returned 404 (stub-mode trigger). */
  stub_mode: boolean;
  /** Network/parse error messaging (UI may render). */
  error?: string;
}

const API_TIMEOUT_MS = 8000;

/**
 * Generic fetch wrapper · 404 → stub_mode=true (NOT error).
 * Distinguishes "endpoint not yet wired" from "real failure".
 */
async function fetchWithStub<T>(
  url: string,
  init?: RequestInit,
): Promise<ContentFetchResult<T>> {
  let controller: AbortController | undefined;
  let timeoutId: ReturnType<typeof setTimeout> | undefined;
  try {
    if (typeof AbortController !== 'undefined') {
      controller = new AbortController();
      timeoutId = setTimeout(() => controller?.abort(), API_TIMEOUT_MS);
    }
    const res = await fetch(url, {
      ...init,
      signal: controller?.signal,
      headers: { Accept: 'application/json', ...(init?.headers ?? {}) },
    });
    if (timeoutId !== undefined) clearTimeout(timeoutId);
    if (res.status === 404) {
      return { data: null, stub_mode: true };
    }
    if (!res.ok) {
      return { data: null, stub_mode: false, error: `http ${res.status}` };
    }
    const json = (await res.json()) as T;
    return { data: json, stub_mode: false };
  } catch (err) {
    if (timeoutId !== undefined) clearTimeout(timeoutId);
    const msg = err instanceof Error ? err.message : 'unknown';
    // Network errors are treated as stub-mode (publish-API may be offline)
    return { data: null, stub_mode: true, error: msg };
  }
}

export type Bucket = 'featured' | 'trending' | 'new' | 'tagged';

export async function fetchContentList(
  bucket: Bucket,
  limit = 24,
  cursor?: string,
): Promise<ContentFetchResult<ContentListResponse>> {
  const params = new URLSearchParams({ bucket, limit: String(limit) });
  if (cursor) params.set('cursor', cursor);
  return fetchWithStub<ContentListResponse>(`/api/content/list?${params.toString()}`);
}

export async function fetchContentDetail(
  slug: string,
): Promise<ContentFetchResult<ContentDetail>> {
  return fetchWithStub<ContentDetail>(`/api/content/detail/${encodeURIComponent(slug)}`);
}

export async function fetchContentSearch(
  query: string,
  tags?: ReadonlyArray<string>,
): Promise<ContentFetchResult<ContentListResponse>> {
  const params = new URLSearchParams({ q: query });
  if (tags && tags.length > 0) params.set('tags', tags.join(','));
  return fetchWithStub<ContentListResponse>(`/api/content/search?${params.toString()}`);
}

export async function fetchSubscribed(
  userCap: string,
): Promise<ContentFetchResult<ContentListResponse>> {
  const params = new URLSearchParams({ user_cap: userCap });
  return fetchWithStub<ContentListResponse>(`/api/content/subscribed?${params.toString()}`);
}

/**
 * Sovereign-unsubscribe · POSTs to revoke endpoint.
 * Stub-mode aware. Returns true if revoked, false if API not wired.
 */
export async function unsubscribe(slug: string): Promise<boolean> {
  const result = await fetchWithStub<{ ok: boolean }>(
    `/api/content/unsubscribe`,
    { method: 'POST', body: JSON.stringify({ slug }) },
  );
  return result.data?.ok === true;
}

/**
 * Status pill display · maps lifecycle → glyph + color.
 * Sawyer-style : LUT keyed on enum, no string-cmp at render-time.
 */
export const STATUS_PILL: Record<
  ContentStatus,
  { glyph: string; label: string; color: string; bg: string }
> = {
  draft: { glyph: '○', label: 'Draft', color: '#9aa0a6', bg: 'rgba(154,160,166,0.08)' },
  playtested: { glyph: '◐', label: 'Playtested', color: '#fbbf24', bg: 'rgba(251,191,36,0.1)' },
  published: { glyph: '✓', label: 'Published', color: '#34d399', bg: 'rgba(52,211,153,0.1)' },
  remixable: { glyph: '⊔', label: 'Remixable', color: '#7dd3fc', bg: 'rgba(125,211,252,0.1)' },
};

/**
 * Truncate pubkey for compact display · "0xab12...cdef".
 * Pre-allocated string-template for Sawyer-friendly rendering.
 */
export function truncatePubkey(pubkey: string): string {
  if (!pubkey || pubkey.length <= 10) return pubkey;
  return `${pubkey.slice(0, 6)}…${pubkey.slice(-4)}`;
}

/**
 * Author display fallback · prefers display_name when set, else truncated pubkey.
 */
export function displayAuthor(item: Pick<ContentItem, 'author_pubkey' | 'author_display'>): string {
  return item.author_display && item.author_display.length > 0
    ? item.author_display
    : truncatePubkey(item.author_pubkey);
}

/**
 * Format ISO timestamp → "3d ago" / "2h ago" / "now".
 * No external date-lib · 0 deps.
 */
export function timeAgo(iso: string): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return '—';
  const delta = Date.now() - then;
  const sec = Math.max(0, Math.floor(delta / 1000));
  if (sec < 60) return 'now';
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h`;
  const day = Math.floor(hr / 24);
  if (day < 30) return `${day}d`;
  const mo = Math.floor(day / 30);
  if (mo < 12) return `${mo}mo`;
  const yr = Math.floor(mo / 12);
  return `${yr}y`;
}

/**
 * Static stub-data · rendered when publish-API returns 404.
 * Demonstrates expected shape so UI never breaks.
 */
export const STUB_ITEMS: ReadonlyArray<ContentItem> = [
  {
    slug: 'stub-zero-state-example',
    title: '⟨ publish-pipeline not yet wired ⟩',
    author_pubkey: '0x0000000000000000000000000000000000000000',
    author_display: 'sibling-W12-5',
    published_at: new Date().toISOString(),
    tags: ['stub', 'placeholder', 'sovereign'],
    rating_summary: { total_ratings: 0, mean_score: 0, distribution: [0, 0, 0, 0, 0] },
    status: 'draft',
    blurb: 'When sibling W12-5 lands the publish-API, real items will appear here. This is a graceful zero-state placeholder.',
    rationale: {
      kind: 'curator-pick',
      explanation: 'Stub-mode placeholder · API endpoint not yet returning 200',
    },
  },
];

export const STUB_LIST_RESPONSE: ContentListResponse = {
  items: STUB_ITEMS,
};

export const STUB_DETAIL: ContentDetail = {
  ...STUB_ITEMS[0],
  description: 'This page surfaces a single content-package detail when the publish-API returns it. In stub-mode (API 404), this placeholder demonstrates the expected layout.',
  screenshots: [],
  cosmetic_axiom_attested: true,
  attribution_chain: [
    {
      slug: STUB_ITEMS[0].slug,
      title: STUB_ITEMS[0].title,
      author_pubkey: STUB_ITEMS[0].author_pubkey,
      generation: 0,
    },
  ],
  remix_slugs: [],
  cap_revocable: true,
};

/**
 * Build-time-validable : ensures STUB_ITEMS shape conforms to ContentItem.
 * Test harness imports + invokes this.
 */
export function validateStubShape(): void {
  for (const item of STUB_ITEMS) {
    if (!item.slug || !item.title || !item.author_pubkey) {
      throw new Error(`STUB_ITEMS shape violation : ${JSON.stringify(item)}`);
    }
    if (!Array.isArray(item.tags)) {
      throw new Error(`tags must be array : ${item.slug}`);
    }
    if (!item.rating_summary || typeof item.rating_summary.mean_score !== 'number') {
      throw new Error(`rating_summary shape : ${item.slug}`);
    }
  }
}
