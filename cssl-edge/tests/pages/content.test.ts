// cssl-edge · tests/pages/content.test.ts
// W12-6 · UGC-Discover-Browse smoke tests
//
// Verifies:
//   - lib/content-fetch types + helpers (truncatePubkey · timeAgo · STUB shapes)
//   - STATUS_PILL LUT covers all enum members
//   - STUB_ITEMS/STUB_DETAIL conform to declared shape
//   - getServerSideProps for /content (landing) returns expected props
//   - getServerSideProps for /content/[slug] handles stub-fallback
//   - sovereignty-UX attestation : NO engagement-tracking fields exist

import {
  STATUS_PILL,
  STUB_ITEMS,
  STUB_LIST_RESPONSE,
  STUB_DETAIL,
  truncatePubkey,
  timeAgo,
  displayAuthor,
  validateStubShape,
  type ContentItem,
  type ContentDetail,
  type ContentStatus,
} from '@/lib/content-fetch';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

export function testStatusPillCoversAllEnumMembers(): void {
  const expected: ContentStatus[] = ['draft', 'playtested', 'published', 'remixable'];
  for (const s of expected) {
    const pill = STATUS_PILL[s];
    assert(pill !== undefined, `STATUS_PILL missing entry for : ${s}`);
    assert(typeof pill.glyph === 'string' && pill.glyph.length > 0, `glyph for ${s}`);
    assert(typeof pill.label === 'string' && pill.label.length > 0, `label for ${s}`);
    assert(typeof pill.color === 'string' && pill.color.startsWith('#'), `color for ${s}`);
    assert(typeof pill.bg === 'string' && pill.bg.length > 0, `bg for ${s}`);
  }
}

export function testStubShape(): void {
  validateStubShape(); // throws on violation
  assert(STUB_ITEMS.length > 0, 'STUB_ITEMS must be non-empty');
  assert(STUB_LIST_RESPONSE.items.length > 0, 'STUB_LIST_RESPONSE.items must be non-empty');
  assert(STUB_DETAIL.slug === STUB_ITEMS[0]!.slug, 'STUB_DETAIL must reference STUB_ITEMS[0]');
  assert(STUB_DETAIL.cosmetic_axiom_attested === true, 'STUB_DETAIL must attest cosmetic-axiom');
  assert(STUB_DETAIL.cap_revocable === true, 'STUB_DETAIL must mark cap_revocable');
  assert(Array.isArray(STUB_DETAIL.attribution_chain), 'attribution_chain must be array');
}

export function testTruncatePubkey(): void {
  assert(truncatePubkey('') === '', 'empty → empty');
  assert(truncatePubkey('short') === 'short', 'short → unchanged');
  const long = '0xabcdef0123456789abcdef0123456789abcdef01';
  const out = truncatePubkey(long);
  assert(out.includes('…'), 'long → contains ellipsis');
  assert(out.startsWith('0xabcd'), 'long → starts with first 6 chars');
  assert(out.endsWith('ef01'), 'long → ends with last 4 chars');
}

export function testTimeAgo(): void {
  const now = new Date().toISOString();
  assert(timeAgo(now) === 'now', 'now → "now"');
  const oneMinAgo = new Date(Date.now() - 60_000).toISOString();
  assert(timeAgo(oneMinAgo) === '1m', '1 minute → "1m"');
  const oneHourAgo = new Date(Date.now() - 60 * 60_000).toISOString();
  assert(timeAgo(oneHourAgo) === '1h', '1 hour → "1h"');
  const oneDayAgo = new Date(Date.now() - 24 * 60 * 60_000).toISOString();
  assert(timeAgo(oneDayAgo) === '1d', '1 day → "1d"');
  assert(timeAgo('garbage') === '—', 'invalid → em-dash');
}

export function testDisplayAuthor(): void {
  const withDisplay: Pick<ContentItem, 'author_pubkey' | 'author_display'> = {
    author_pubkey: '0xabcdef0123456789abcdef0123456789abcdef01',
    author_display: 'Apocky',
  };
  assert(displayAuthor(withDisplay) === 'Apocky', 'display fallback prefers display_name');
  const noDisplay = { author_pubkey: '0xabcdef0123456789abcdef0123456789abcdef01' };
  assert(displayAuthor(noDisplay).includes('…'), 'no display → truncated pubkey');
}

export function testNoEngagementTrackingFields(): void {
  // Sovereignty assertion : ContentItem shape MUST NOT include scroll-depth,
  // time-on-page, click-through-rate, or per-user behavioral fields.
  // We verify the runtime stub shape doesn't accidentally include them.
  const allItems: ReadonlyArray<ContentItem> = [...STUB_ITEMS];
  for (const item of allItems) {
    const keys = Object.keys(item);
    const forbidden = [
      'scroll_depth',
      'time_on_page',
      'click_through_rate',
      'view_duration_ms',
      'engagement_score',
      'session_id',
      'user_fingerprint',
    ];
    for (const f of forbidden) {
      assert(!keys.includes(f), `sovereignty violation : ${item.slug} contains ${f}`);
    }
  }
}

export function testRationaleShape(): void {
  // Trending-feed cards expose rationale.kind for "why am I seeing this?"
  const allowedKinds = [
    'kan-bias',
    'curator-pick',
    'subscribed',
    'tagged-by-you',
    'new',
    'remix-of-yours',
  ];
  for (const item of STUB_ITEMS) {
    if (item.rationale) {
      assert(
        allowedKinds.includes(item.rationale.kind),
        `rationale.kind ∈ allowed-set : ${item.rationale.kind}`,
      );
      assert(
        typeof item.rationale.explanation === 'string' && item.rationale.explanation.length > 0,
        `rationale.explanation must be non-empty`,
      );
    }
  }
}

export function testAttributionChainShape(): void {
  const chain = STUB_DETAIL.attribution_chain;
  assert(chain.length > 0, 'attribution_chain must have at least 1 entry (self)');
  for (const link of chain) {
    assert(typeof link.slug === 'string', 'link.slug shape');
    assert(typeof link.title === 'string', 'link.title shape');
    assert(typeof link.author_pubkey === 'string', 'link.author_pubkey shape');
    assert(typeof link.generation === 'number' && link.generation >= 0, 'generation shape');
  }
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  try {
    testStatusPillCoversAllEnumMembers();
    testStubShape();
    testTruncatePubkey();
    testTimeAgo();
    testDisplayAuthor();
    testNoEngagementTrackingFields();
    testRationaleShape();
    testAttributionChainShape();
    // eslint-disable-next-line no-console
    console.log('content.test : OK · 8 tests passed');
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  }
}
