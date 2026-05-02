// cssl-edge · tests/engine-status.test.ts
// § Self-test for /engine page lib · zero-state shapes · format helpers · normalizers
// Runs via : node --import tsx tests/engine-status.test.ts
// W14-M live-engine-status-page

import {
  emptyHeartbeat,
  emptyCycles,
  emptyEvents,
  emptyPauseState,
  fmtCompact,
  fmtBytes,
  fmtUptime,
  fmtRelTime,
  fmtRelTimeCoarse,
  isOnline,
  sanitizePubkey,
  type NodeHeartbeat,
} from '@/lib/engine-status';

// ─────────────────────────────────────────────
// § Test framework primitive (zero-deps)
// ─────────────────────────────────────────────
function assert(cond: unknown, msg: string): void {
  if (!cond) throw new Error(`assertion failed: ${msg}`);
}
function eq<T>(actual: T, expected: T, msg?: string): void {
  if (actual !== expected) {
    throw new Error(`expected ${JSON.stringify(expected)} · got ${JSON.stringify(actual)} :: ${msg ?? ''}`);
  }
}

// ─────────────────────────────────────────────
// § Tests · zero-state shapes
// ─────────────────────────────────────────────
function testEmptyHeartbeatShape(): void {
  const z = emptyHeartbeat();
  eq(z.local.online, false, 'local online=false');
  eq(z.cloud.online, false, 'cloud online=false');
  eq(z.local.last_seen, 0, 'local last_seen=0');
  eq(z.local.uptime_secs, 0, 'local uptime=0');
  eq(z.cloud.last_seen, 0, 'cloud last_seen=0');
  eq(z.stub, true, 'stub=true');
  assert(typeof z.reason === 'string' && z.reason.length > 0, 'reason set');
  assert(typeof z.now === 'number' && z.now > 0, 'now epoch-ms');
}

function testEmptyHeartbeatCustomReason(): void {
  const z = emptyHeartbeat('custom-x');
  eq(z.reason, 'custom-x', 'custom reason override');
}

function testEmptyCyclesShape(): void {
  const z = emptyCycles();
  eq(z.self_author, 0, 'self_author=0');
  eq(z.playtest, 0, 'playtest=0');
  eq(z.kan_rollup, 0, 'kan_rollup=0');
  eq(z.mycelium_sync, 0, 'mycelium_sync=0');
  eq(z.sigma_chain_anchors, 0, 'sigma_chain_anchors=0');
  eq(z.bytes_processed, 0, 'bytes_processed=0');
  eq(z.stub, true, 'stub=true');
  assert(z.until > z.since, 'window valid');
  assert(z.until - z.since >= 3_500_000, 'window ~1hr');
}

function testEmptyEventsShape(): void {
  const z = emptyEvents();
  eq(z.events.length, 0, 'no events');
  eq(z.stub, true, 'stub=true');
}

function testEmptyPauseStateShape(): void {
  const z = emptyPauseState();
  eq(z.paused, false, 'paused=false');
  eq(z.stub, true, 'stub=true');
}

// ─────────────────────────────────────────────
// § Tests · format helpers
// ─────────────────────────────────────────────
function testFmtCompactBranches(): void {
  eq(fmtCompact(null), '—', 'null→em-dash');
  eq(fmtCompact(undefined), '—', 'undef→em-dash');
  eq(fmtCompact(NaN), '—', 'NaN→em-dash');
  eq(fmtCompact(0), '0', '0 integer');
  eq(fmtCompact(42), '42', '42 integer');
  eq(fmtCompact(1500), '1.5k', '1500→1.5k');
  eq(fmtCompact(2_500_000), '2.5M', '2.5M');
  eq(fmtCompact(1_500_000_000), '1.5B', '1.5B');
  eq(fmtCompact(3.14), '3.1', 'fractional');
}

function testFmtBytesBranches(): void {
  eq(fmtBytes(null), '—', 'null');
  eq(fmtBytes(undefined), '—', 'undef');
  eq(fmtBytes(NaN), '—', 'NaN');
  eq(fmtBytes(0), '0 B', 'zero');
  eq(fmtBytes(512), '512 B', 'bytes');
  eq(fmtBytes(2048), '2.0 KiB', 'kib');
  eq(fmtBytes(1_500_000), '1.4 MiB', 'mib');
  eq(fmtBytes(2 * 1_073_741_824), '2.0 GiB', 'gib');
}

function testFmtUptimeBranches(): void {
  eq(fmtUptime(null), '—', 'null');
  eq(fmtUptime(undefined), '—', 'undef');
  eq(fmtUptime(-1), '—', 'negative');
  eq(fmtUptime(0), '0s', 'zero');
  eq(fmtUptime(45), '45s', '45s');
  eq(fmtUptime(120), '2m', '2m');
  eq(fmtUptime(3661), '1h 1m', '1h1m');
  eq(fmtUptime(90_000), '1d 1h', '1d1h');
}

function testFmtRelTime(): void {
  const now = 1_000_000_000_000;
  eq(fmtRelTime(null, now), 'never', 'null');
  eq(fmtRelTime(undefined, now), 'never', 'undef');
  eq(fmtRelTime(0, now), 'never', 'zero');
  eq(fmtRelTime(now - 2000, now), 'just-now', 'just-now <5s');
  eq(fmtRelTime(now - 30_000, now), '30s ago', '30s');
  eq(fmtRelTime(now - 600_000, now), '10m ago', '10m');
  eq(fmtRelTime(now - 7_200_000, now), '2h ago', '2h');
  eq(fmtRelTime(now - 2 * 86_400_000, now), '2d ago', '2d');
}

function testFmtRelTimeCoarse(): void {
  const now = 1_000_000_000_000;
  eq(fmtRelTimeCoarse(null, now), 'unknown', 'null');
  eq(fmtRelTimeCoarse(undefined, now), 'unknown', 'undef');
  eq(fmtRelTimeCoarse(0, now), 'unknown', 'zero');
  eq(fmtRelTimeCoarse(now - 30_000, now), 'within the last minute', '<1m');
  eq(fmtRelTimeCoarse(now - 600_000, now), 'within the last hour', '<1h');
  eq(fmtRelTimeCoarse(now - 7_200_000, now), 'within the last day', '<1d');
  eq(fmtRelTimeCoarse(now - 2 * 86_400_000, now), 'within the last week', '<1w');
  eq(fmtRelTimeCoarse(now - 30 * 86_400_000, now), 'over a week ago', '>1w');
}

function testIsOnlineSemantics(): void {
  const now = 1_000_000_000_000;
  const never: NodeHeartbeat = { last_seen: 0, uptime_secs: 0, online: false };
  const fresh: NodeHeartbeat = { last_seen: now - 5_000, uptime_secs: 100, online: true };
  const stale: NodeHeartbeat = { last_seen: now - 60_000, uptime_secs: 100, online: true };
  eq(isOnline(never, now), false, 'never offline');
  eq(isOnline(fresh, now), true, 'fresh online');
  eq(isOnline(stale, now), false, 'stale offline');
}

function testSanitizePubkeySafety(): void {
  // privacy : truncate + strip non-hex (hex-digits a-f are valid)
  eq(sanitizePubkey('abc123def456789'), 'abc123de', 'truncate to 8');
  eq(sanitizePubkey('!@#$%^&*()'), '', 'strip all non-hex');
  eq(sanitizePubkey('xyzghij'), '', 'no hex chars');
  eq(sanitizePubkey('not-hex!@#'), 'e', 'pulls e (valid hex) from "hex"');
  eq(sanitizePubkey('AABBCCDDEEFF1122'), 'AABBCCDD', 'mixed case truncated');
  eq(sanitizePubkey(''), '', 'empty');
  eq(sanitizePubkey('a'), 'a', 'single char');
}

// ─────────────────────────────────────────────
// § Tests · defensive composition
// ─────────────────────────────────────────────
function testZeroStateConsumableByPage(): void {
  // ⊑ verify all four zero-states are safely consumable (no NaN/undef leaks)
  const hb = emptyHeartbeat();
  const cy = emptyCycles();
  const ev = emptyEvents();
  const pa = emptyPauseState();

  // counters can sum across without errors
  const total = cy.self_author + cy.playtest + cy.kan_rollup + cy.mycelium_sync;
  eq(total, 0, 'sum of zero-cycles is 0');

  // bytes-per-cycle would divide-by-zero · UI must guard · here we just ensure inputs zero
  eq(cy.bytes_processed, 0, 'bytes_processed zero');

  // events list-length safe
  eq(ev.events.length, 0, 'events safe');

  // pause-state default safe
  eq(pa.paused, false, 'pause-state default false');

  // heartbeat-online derived
  eq(hb.local.online, false, 'local default offline');
  eq(hb.cloud.online, false, 'cloud default offline');
}

// ─────────────────────────────────────────────
// § Runner
// ─────────────────────────────────────────────
declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

function runAll(): void {
  testEmptyHeartbeatShape();
  testEmptyHeartbeatCustomReason();
  testEmptyCyclesShape();
  testEmptyEventsShape();
  testEmptyPauseStateShape();
  testFmtCompactBranches();
  testFmtBytesBranches();
  testFmtUptimeBranches();
  testFmtRelTime();
  testFmtRelTimeCoarse();
  testIsOnlineSemantics();
  testSanitizePubkeySafety();
  testZeroStateConsumableByPage();
  // eslint-disable-next-line no-console
  console.log('engine-status.test : OK · 13 tests passed');
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

export {};
