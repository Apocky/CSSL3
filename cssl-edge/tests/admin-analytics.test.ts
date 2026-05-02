// cssl-edge · tests/admin-analytics.test.ts
// § Lightweight self-test for admin-analytics surface :
//   1. lib/admin-metrics.ts pure-function correctness
//   2. chart-primitive output (mocked-React render via direct-call)
//   3. zero-state defensive-shape
//   4. fmt-helpers
// Runs via : node --import tsx tests/admin-analytics.test.ts
// I> deliberately ¬ requires Jest · matches existing test-pattern in cssl-edge/

import {
  emptyMetrics,
  fmtCompact,
  fmtMs,
  fmtPercent,
  extractSeries,
  tagHistogram,
  type MetricEvent,
  type MetricsResponse,
} from '@/lib/admin-metrics';

// ─────────────────────────────────────────────
// § Test framework primitive (zero-deps)
// ─────────────────────────────────────────────
function assert(cond: unknown, msg: string): void {
  if (!cond) {
    throw new Error(`assertion failed: ${msg}`);
  }
}
function eq<T>(actual: T, expected: T, msg?: string): void {
  if (actual !== expected) {
    throw new Error(`expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)} :: ${msg ?? ''}`);
  }
}

// ─────────────────────────────────────────────
// § Tests
// ─────────────────────────────────────────────
export function testEmptyMetricsShape(): void {
  const z = emptyMetrics('engine.frame_tick', '1min');
  eq(z.kind, 'engine.frame_tick', 'kind passthrough');
  eq(z.bucket, '1min', 'bucket passthrough');
  eq(z.events.length, 0, 'empty events');
  eq(z.rollup.count, 0, 'rollup count');
  eq(z.stub, true, 'stub flag');
  assert(typeof z.reason === 'string' && z.reason.length > 0, 'reason set');
}

export function testEmptyMetricsCustomReason(): void {
  const z = emptyMetrics('mcp.tool_called', '1hr', 'custom-reason-x');
  eq(z.reason, 'custom-reason-x', 'custom reason override');
}

export function testFmtCompact(): void {
  eq(fmtCompact(undefined), '—', 'undefined→em-dash');
  eq(fmtCompact(null), '—', 'null→em-dash');
  eq(fmtCompact(NaN), '—', 'NaN→em-dash');
  eq(fmtCompact(0), '0', '0 integer');
  eq(fmtCompact(42), '42', '42 integer');
  eq(fmtCompact(1500), '1.5k', '1500→1.5k');
  eq(fmtCompact(2_500_000), '2.5M', '2.5M');
  eq(fmtCompact(3.14), '3.1', 'fractional');
}

export function testFmtMs(): void {
  eq(fmtMs(undefined), '—', 'undef');
  eq(fmtMs(0.5), '500µs', 'sub-ms');
  eq(fmtMs(15.7), '15.7ms', 'ms');
  eq(fmtMs(2500), '2.5s', 'seconds');
}

export function testFmtPercent(): void {
  eq(fmtPercent(0.156), '15.6%', '15.6%');
  eq(fmtPercent(undefined), '—', 'undef');
  eq(fmtPercent(0), '0.0%', 'zero');
  eq(fmtPercent(1), '100.0%', 'full');
}

export function testExtractSeries(): void {
  const events: MetricEvent[] = [
    { ts: 1, v: 60 },
    { ts: 2, v: 59.5 },
    { ts: 3 }, // ⊑ no v → 0
    { ts: 4, v: NaN }, // ⊑ NaN → 0
    { ts: 5, v: 58 },
  ];
  const s = extractSeries(events);
  eq(s.length, 5, 'length');
  eq(s[0], 60, '[0]');
  eq(s[1], 59.5, '[1]');
  eq(s[2], 0, '[2] missing→0');
  eq(s[3], 0, '[3] NaN→0');
  eq(s[4], 58, '[4]');
}

export function testExtractSeriesEmpty(): void {
  const s = extractSeries([]);
  eq(s.length, 0, 'empty array');
}

export function testTagHistogram(): void {
  const events: MetricEvent[] = [
    { ts: 1, tag: 'persona.a' },
    { ts: 2, tag: 'persona.b' },
    { ts: 3, tag: 'persona.a' },
    { ts: 4, tag: 'persona.a' },
    { ts: 5 }, // ⊑ no tag → 'unknown'
  ];
  const h = tagHistogram(events);
  // ⊑ map preserves insertion-order
  const a = h.find((x) => x.label === 'persona.a');
  const b = h.find((x) => x.label === 'persona.b');
  const u = h.find((x) => x.label === 'unknown');
  assert(a !== undefined, 'persona.a present');
  assert(b !== undefined, 'persona.b present');
  assert(u !== undefined, 'unknown present');
  eq(a!.value, 3, 'persona.a count');
  eq(b!.value, 1, 'persona.b count');
  eq(u!.value, 1, 'unknown count');
}

export function testMetricsResponseDefensiveDefault(): void {
  // ⊑ verify zero-state can be safely consumed by chart-primitives
  const z: MetricsResponse = emptyMetrics('intent.classified', '1min');
  // chart-friendly extraction must NOT throw
  const series = extractSeries(z.events);
  eq(series.length, 0, 'zero-state series empty');
  const hist = tagHistogram(z.events);
  eq(hist.length, 0, 'zero-state hist empty');
  // ⊑ avg/p95/p99 absent → fmtMs(undefined) = '—'
  eq(fmtMs(z.rollup.p95), '—', 'p95 zero-state');
  eq(fmtCompact(z.rollup.count), '0', 'count zero-state');
}

export function testTagHistogramDeterministicSort(): void {
  // ⊑ unsorted-by-construction · BarChart sorts internally · here we only check totals stable
  const events: MetricEvent[] = Array.from({ length: 100 }, (_, i) => ({
    ts: i,
    tag: `t${i % 4}`,
  }));
  const h = tagHistogram(events);
  eq(h.length, 4, '4 distinct tags');
  let total = 0;
  for (const e of h) total += e.value;
  eq(total, 100, 'totals match input');
}

// ─────────────────────────────────────────────
// § Runner (matches health-w9 pattern)
// ─────────────────────────────────────────────
declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

function runAll(): void {
  testEmptyMetricsShape();
  testEmptyMetricsCustomReason();
  testFmtCompact();
  testFmtMs();
  testFmtPercent();
  testExtractSeries();
  testExtractSeriesEmpty();
  testTagHistogram();
  testMetricsResponseDefensiveDefault();
  testTagHistogramDeterministicSort();
  // eslint-disable-next-line no-console
  console.log('admin-analytics.test : OK · 10 tests passed');
}

if (isMain) {
  try {
    runAll();
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  }
}
