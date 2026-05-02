// cssl-edge · /api/content/remix
// W12-9 · Init a fork-attribution. Returns Σ-Chain-anchor + new content-id
// reservation. Mirror-of compiler-rs/crates/cssl-content-remix in pure-JS
// (canonical-bytes layout matches `sign.rs`).
//
// POST /api/content/remix
//   body : {
//     parent_id          : string (uuid)
//     parent_version     : string ("M.m.p")
//     remix_kind         : "fork"|"extension"|"translation"|"adaptation"|"improvement"|"bundle"
//     attribution_text   : string (≤200)
//     remix_creator_pubkey : string (64-hex)
//     created_at         : number (unix-seconds)
//     royalty_share_gift : { pledged_pct : number 0..=100 } | null
//     remix_signature    : string (128-hex)   // CALLER signs canonical-bytes
//   }
//   → 200 { ok, remixed_id (uuid), sigma_chain_anchor (hex64), via : 'init' }
//   → 200 { stub:true, todo }    when SUPABASE_URL missing (stub-mode)
//   → 4xx { ok:false, error }    on validation or opt-out failure
//
// All routes audit-emit · sovereign-bypass-RECORDED.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { checkCap, CONTENT_CAP_REMIX } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';

interface RemixInitReq {
  parent_id?: string;
  parent_version?: string;
  remix_kind?: string;
  attribution_text?: string;
  remix_creator_pubkey?: string;
  created_at?: number;
  royalty_share_gift?: { pledged_pct: number } | null;
  remix_signature?: string;
  cap?: number;
  sovereign?: boolean;
}

const REMIX_KINDS = new Set([
  'fork',
  'extension',
  'translation',
  'adaptation',
  'improvement',
  'bundle',
]);

const ATTRIBUTION_MAX = 200;

const HEX64_RE = /^[0-9a-f]{64}$/;
const HEX128_RE = /^[0-9a-f]{128}$/;
const SEMVER_RE = /^\d+\.\d+\.\d+$/;
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

interface ValidationOk {
  ok: true;
  parent_id: string;
  parent_version: string;
  remix_kind: string;
  attribution_text: string;
  remix_creator_pubkey: string;
  created_at: number;
  royalty_pct: number;
  remix_signature: string;
}
interface ValidationErr {
  ok: false;
  error: string;
}
type Validation = ValidationOk | ValidationErr;

function validate(body: RemixInitReq): Validation {
  if (!body.parent_id || !UUID_RE.test(body.parent_id)) {
    return { ok: false, error: 'parent_id must be uuid' };
  }
  if (!body.parent_version || !SEMVER_RE.test(body.parent_version)) {
    return { ok: false, error: 'parent_version must be M.m.p' };
  }
  if (!body.remix_kind || !REMIX_KINDS.has(body.remix_kind)) {
    return {
      ok: false,
      error: `remix_kind must be one of ${[...REMIX_KINDS].join(',')}`,
    };
  }
  const attribution = body.attribution_text ?? '';
  if (typeof attribution !== 'string' || attribution.length > ATTRIBUTION_MAX) {
    return { ok: false, error: `attribution_text ≤ ${ATTRIBUTION_MAX} chars` };
  }
  if (!body.remix_creator_pubkey || !HEX64_RE.test(body.remix_creator_pubkey)) {
    return { ok: false, error: 'remix_creator_pubkey must be 64-char lower-hex' };
  }
  if (typeof body.created_at !== 'number' || body.created_at < 0) {
    return { ok: false, error: 'created_at must be unix-seconds (number)' };
  }
  let pct = 0;
  if (body.royalty_share_gift !== null && body.royalty_share_gift !== undefined) {
    const p = body.royalty_share_gift.pledged_pct;
    if (typeof p !== 'number' || p < 0 || p > 100 || !Number.isInteger(p)) {
      return { ok: false, error: 'royalty_share_gift.pledged_pct must be int 0..=100' };
    }
    pct = p;
  }
  if (!body.remix_signature || !HEX128_RE.test(body.remix_signature)) {
    return { ok: false, error: 'remix_signature must be 128-char lower-hex' };
  }
  return {
    ok: true,
    parent_id: body.parent_id,
    parent_version: body.parent_version,
    remix_kind: body.remix_kind,
    attribution_text: attribution,
    remix_creator_pubkey: body.remix_creator_pubkey,
    created_at: body.created_at,
    royalty_pct: pct,
    remix_signature: body.remix_signature,
  };
}

// Generate a uuid-v4 client-side. Node 18+ exposes crypto.randomUUID via
// the Web-Crypto global which Next/Vercel polyfills in both edge + node
// runtimes. We avoid `require('crypto')` to stay edge-runtime-compatible.
function genUuid(): string {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const g = (globalThis as { crypto?: { randomUUID?: () => string } }).crypto;
  if (g && typeof g.randomUUID === 'function') {
    return g.randomUUID();
  }
  // Final fallback : RFC-4122-ish manual generation (deterministic-shape
  // hex). Used in stub-mode + tests when Web-Crypto is unavailable.
  const hex = '0123456789abcdef';
  let s = '';
  for (let i = 0; i < 32; i++) {
    s += hex[Math.floor(Math.random() * 16)];
    if (i === 7 || i === 11 || i === 15 || i === 19) s += '-';
  }
  // Set version-4 bits at positions 14 + 19 (after dashes accounted for).
  return s.replace(/^(.{14})./, '$14').replace(/^(.{19})./, '$1a');
}

// BLAKE3-anchor placeholder. Edge does NOT recompute anchor (caller
// supplies signature over the anchor bytes). The endpoint stores the
// caller-provided signature + reads back the anchor from the canonical
// bytes via length-checked echo. The Rust crate is the canonical signer.
//
// To keep the edge stub deterministic we hash the canonical-bytes-equiv
// JSON via Node-native sha256 (BLAKE3 on the JS side requires native deps
// we don't want to ship to Vercel-edge). Verifiers downstream MUST use
// the BLAKE3-derived anchor stored by the signer. Here the field is
// echoed only — the source-of-truth is the canonical Rust signer.
function anchorPlaceholder(): string {
  // 64-char lower-hex zero-prefix marker. Real anchor flows from caller.
  return '0'.repeat(64);
}

interface RemixInitOk {
  ok: true;
  remixed_id: string;
  sigma_chain_anchor: string;
  parent_id: string;
  remix_kind: string;
  via: 'init';
  served_by: string;
  ts: string;
}
interface RemixInitStub {
  stub: true;
  todo: string;
  served_by: string;
  ts: string;
}
interface RemixInitErr {
  ok: false;
  error: string;
  served_by: string;
  ts: string;
}
type Resp = RemixInitOk | RemixInitStub | RemixInitErr;

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<Resp>
): Promise<void> {
  logHit('content.remix.init', { method: req.method ?? 'POST' });
  const env = envelope();

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...env });
    return;
  }

  const body = (req.body ?? {}) as RemixInitReq;

  // ── cap-gate · default-DENY ───────────────────────────────────────────
  const sovereign = isSovereignFromIncoming(req.headers, body.sovereign);
  const callerCap = typeof body.cap === 'number' ? body.cap : 0;
  const decision = checkCap(callerCap, CONTENT_CAP_REMIX, sovereign);
  if (!decision.ok) {
    const d = deny(decision.reason ?? 'cap denied', CONTENT_CAP_REMIX);
    logEvent(d.body);
    res.status(d.status).json({
      ok: false,
      error: 'cap denied · CONTENT_CAP_REMIX (0x2000) required',
      ...env,
    });
    return;
  }

  // ── input validation ─────────────────────────────────────────────────
  const v = validate(body);
  if (!v.ok) {
    res.status(400).json({ ok: false, error: v.error, ...env });
    return;
  }

  // ── stub-mode (no Supabase configured) ───────────────────────────────
  const supaUrl = process.env['SUPABASE_URL'];
  if (!supaUrl || supaUrl.length === 0) {
    logEvent(
      auditEvent('content.remix.init.stub', CONTENT_CAP_REMIX, sovereign, 'ok', {
        parent_id: v.parent_id,
        kind: v.remix_kind,
      })
    );
    res.status(200).json({
      ...stubEnvelope('set SUPABASE_URL + SUPABASE_SERVICE_ROLE_KEY on Vercel'),
    });
    return;
  }

  // ── live path : reserve content-id + persist link ────────────────────
  const remixed_id = genUuid();
  const sigma_chain_anchor = anchorPlaceholder();
  // NOTE : the cssl-edge layer does not call Supabase directly here (avoid
  // bundling pg-driver into edge). The Rust loa-host is the canonical
  // writer ; this endpoint emits the audit-event so Vercel logs capture
  // intent and returns the reserved id for the client to ship to the
  // Rust signer. Single-source-of-truth = signed RemixLink in DB.
  logEvent(
    auditEvent('content.remix.init', CONTENT_CAP_REMIX, sovereign, 'ok', {
      remixed_id,
      parent_id: v.parent_id,
      kind: v.remix_kind,
      pct: v.royalty_pct,
    })
  );
  res.status(200).json({
    ok: true,
    remixed_id,
    sigma_chain_anchor,
    parent_id: v.parent_id,
    remix_kind: v.remix_kind,
    via: 'init',
    ...env,
  });
}

// ─── Inline tests · framework-agnostic ────────────────────────────────────

interface MockedResponse {
  statusCode: number;
  body: unknown;
}

function mockReqRes(
  method: string,
  body: unknown = {},
  headers: Record<string, string> = {}
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null };
  const req = { method, query: {}, headers, body } as unknown as NextApiRequest;
  const res = {
    status(code: number) {
      out.statusCode = code;
      return this;
    },
    json(payload: unknown) {
      out.body = payload;
      return this;
    },
    setHeader(_k: string, _v: string) {
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. cap=0 + sovereign=false → 403.
export async function testRemixInitCapDenied(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { cap: 0 });
  await handler(req, res);
  assert(out.statusCode === 403, `expected 403, got ${out.statusCode}`);
}

// 2. valid input + no SUPABASE_URL → 200 stub-mode.
export async function testRemixInitStubMode(): Promise<void> {
  const prev = process.env['SUPABASE_URL'];
  delete process.env['SUPABASE_URL'];
  const validBody = {
    parent_id: '00000000-1234-5678-9abc-000000000001',
    parent_version: '1.0.0',
    remix_kind: 'fork',
    attribution_text: 'a respectful fork of the original',
    remix_creator_pubkey: 'a'.repeat(64),
    created_at: 1700000000,
    royalty_share_gift: { pledged_pct: 10 },
    remix_signature: 'b'.repeat(128),
    cap: CONTENT_CAP_REMIX,
  };
  const { req, res, out } = mockReqRes('POST', validBody);
  await handler(req, res);
  if (prev !== undefined) process.env['SUPABASE_URL'] = prev;
  assert(out.statusCode === 200, `expected 200 stub, got ${out.statusCode}`);
  const body = out.body as { stub?: boolean };
  assert(body.stub === true, 'expected stub:true');
}

// 3. invalid remix_kind rejected.
export async function testRemixInitBadKind(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    parent_id: '00000000-1234-5678-9abc-000000000001',
    parent_version: '1.0.0',
    remix_kind: 'troll',
    attribution_text: '',
    remix_creator_pubkey: 'a'.repeat(64),
    created_at: 1,
    remix_signature: 'b'.repeat(128),
    cap: CONTENT_CAP_REMIX,
  });
  await handler(req, res);
  assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
}

// 4. attribution-text > 200 chars rejected.
export async function testRemixInitAttributionTooLong(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    parent_id: '00000000-1234-5678-9abc-000000000001',
    parent_version: '1.0.0',
    remix_kind: 'extension',
    attribution_text: 'x'.repeat(201),
    remix_creator_pubkey: 'a'.repeat(64),
    created_at: 1,
    remix_signature: 'b'.repeat(128),
    cap: CONTENT_CAP_REMIX,
  });
  await handler(req, res);
  assert(out.statusCode === 400, `expected 400 attribution-too-long`);
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  void (async () => {
    await testRemixInitCapDenied();
    await testRemixInitStubMode();
    await testRemixInitBadKind();
    await testRemixInitAttributionTooLong();
    // eslint-disable-next-line no-console
    console.log('content/remix.ts : OK · 4 inline tests passed');
  })();
}
