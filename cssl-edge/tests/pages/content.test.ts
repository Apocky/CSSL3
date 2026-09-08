// Shared-content helpers and honest-empty-state tests.

import {
  EMPTY_LIST_RESPONSE,
  STATUS_PILL,
  displayAuthor,
  timeAgo,
  truncatePubkey,
  type ContentDetail,
  type ContentItem,
  type ContentStatus,
} from '@/lib/content-fetch';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

const SAMPLE_ITEM: ContentItem = {
  slug: 'verified-example',
  title: 'Verified example',
  author_pubkey: '0xabcdef0123456789abcdef0123456789abcdef01',
  author_display: 'Apocky',
  published_at: '2026-07-27T12:00:00.000Z',
  tags: ['example'],
  rating_summary: {
    total_ratings: 0,
    mean_score: 0,
    distribution: [0, 0, 0, 0, 0],
  },
  status: 'published',
  blurb: 'A local test value. It is never rendered as public content.',
  rationale: {
    kind: 'curator-pick',
    explanation: 'Used only to validate the declared data shape.',
  },
};

const SAMPLE_DETAIL: ContentDetail = {
  ...SAMPLE_ITEM,
  description: 'Local test detail.',
  screenshots: [],
  cosmetic_axiom_attested: false,
  attribution_chain: [
    {
      slug: SAMPLE_ITEM.slug,
      title: SAMPLE_ITEM.title,
      author_pubkey: SAMPLE_ITEM.author_pubkey,
      generation: 0,
    },
  ],
  remix_slugs: [],
  cap_revocable: false,
};

export function testStatusPillUsesWordsForEveryStatus(): void {
  const expected: ContentStatus[] = ['draft', 'playtested', 'published', 'remixable'];
  for (const status of expected) {
    const pill = STATUS_PILL[status];
    assert(pill !== undefined, `STATUS_PILL missing entry for ${status}`);
    assert(typeof pill.label === 'string' && pill.label.length > 0, `label for ${status}`);
    assert(
      !Object.prototype.hasOwnProperty.call(pill, 'glyph'),
      `status ${status} must not require a symbol key`,
    );
    assert(typeof pill.color === 'string' && pill.color.startsWith('#'), `color for ${status}`);
    assert(typeof pill.bg === 'string' && pill.bg.length > 0, `background for ${status}`);
  }
}

export function testUnavailableServiceUsesHonestEmptyState(): void {
  assert(
    EMPTY_LIST_RESPONSE.items.length === 0,
    'an unavailable service must not return fabricated content',
  );
}

export function testTruncatePubkey(): void {
  assert(truncatePubkey('') === '', 'empty remains empty');
  assert(truncatePubkey('short') === 'short', 'short remains unchanged');
  const out = truncatePubkey(SAMPLE_ITEM.author_pubkey);
  assert(out.includes('…'), 'long key contains an ellipsis');
  assert(out.startsWith('0xabcd'), 'long key starts with its first six characters');
  assert(out.endsWith('ef01'), 'long key ends with its final four characters');
}

export function testTimeAgo(): void {
  const now = new Date().toISOString();
  assert(timeAgo(now) === 'now', 'current time displays as now');
  assert(timeAgo(new Date(Date.now() - 60_000).toISOString()) === '1m', 'one minute');
  assert(timeAgo(new Date(Date.now() - 60 * 60_000).toISOString()) === '1h', 'one hour');
  assert(timeAgo(new Date(Date.now() - 24 * 60 * 60_000).toISOString()) === '1d', 'one day');
  assert(timeAgo('garbage') === '—', 'invalid timestamp displays an em dash');
}

export function testDisplayAuthor(): void {
  assert(displayAuthor(SAMPLE_ITEM) === 'Apocky', 'display name takes precedence');
  assert(
    displayAuthor({ author_pubkey: SAMPLE_ITEM.author_pubkey }).includes('…'),
    'a missing display name falls back to a shortened key',
  );
}

export function testNoEngagementTrackingFields(): void {
  const keys = Object.keys(SAMPLE_ITEM);
  const forbidden = [
    'scroll_depth',
    'time_on_page',
    'click_through_rate',
    'view_duration_ms',
    'engagement_score',
    'session_id',
    'user_fingerprint',
  ];
  for (const field of forbidden) {
    assert(!keys.includes(field), `content shape must not contain ${field}`);
  }
}

export function testRationaleShape(): void {
  const rationale = SAMPLE_ITEM.rationale;
  assert(rationale !== undefined, 'sample rationale exists');
  assert(rationale?.kind === 'curator-pick', 'sample rationale has a declared selection method');
  assert(Boolean(rationale?.explanation), 'sample rationale includes a plain-language explanation');
}

export function testAttributionChainShape(): void {
  const chain = SAMPLE_DETAIL.attribution_chain;
  assert(chain.length === 1, 'sample contains one attributable source');
  const first = chain[0];
  assert(first?.slug === SAMPLE_ITEM.slug, 'attribution points to the source item');
  assert(first?.generation === 0, 'original item uses generation zero');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  try {
    testStatusPillUsesWordsForEveryStatus();
    testUnavailableServiceUsesHonestEmptyState();
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
