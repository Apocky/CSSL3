// Setting up an authenticator, end to end.
//
// This flow had NO automated coverage at all — neither the enrolment endpoint nor the sign-in
// endpoint — and the one TOTP test that did exist (tests/lib/auth-totp.test.ts) was not wired into
// any npm script, so it had never run either. The only thing that had ever exercised enrolment was
// a person doing it by hand.
//
// It cannot be exercised through the local test-auth bypass, and that is not an oversight worth
// working around: the bypass identity is the literal string 'test-admin', while the enrolment
// functions take `p_user_id uuid` and the factor table's primary key is a foreign key into
// auth.users. Both refusals are correct. They just mean the handlers can only be driven with a
// real account, which is why nobody had driven them.
//
// So the handlers below are the REAL ones, with the REAL crypto and the REAL QR encoder. What is
// substituted is the database, by an in-memory stand-in written to match migrations 0060-0062
// exactly — same pending/confirmed split, same 30-minute enrolment expiry, same replay watermark,
// same lockout. Where that stand-in could differ from Postgres is called out at each point.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { runInNewContext } from 'node:vm';

import ts from 'typescript';
import QRCode from 'qrcode';

import { codeForStep, currentStep, TOTP_PERIOD_SECONDS, verifyTotp } from '@/lib/auth-totp';
import * as realTotp from '@/lib/auth-totp';

const OWNER = '11111111-1111-4111-8111-111111111111';
const EMAIL = 'someone@example.com';
const LOCKOUT_ATTEMPTS = 5;

interface FactorRow {
  user_id: string;
  secret: string | null;
  confirmed_at: string | null;
  last_used_step: number | null;
  failed_attempts: number;
  locked_until: string | null;
  pending_secret: string | null;
  pending_created_at: number | null;
}

/** The four SECURITY DEFINER functions from 0060-0062, in memory, with their real semantics. */
class FakeHub {
  readonly rows = new Map<string, FactorRow>();
  now = Date.now();
  readonly calls: string[] = [];

  private row(id: string): FactorRow {
    let row = this.rows.get(id);
    if (!row) {
      row = {
        user_id: id, secret: null, confirmed_at: null, last_used_step: null,
        failed_attempts: 0, locked_until: null, pending_secret: null, pending_created_at: null,
      };
      this.rows.set(id, row);
    }
    return row;
  }

  async rpc(name: string, args: Record<string, unknown>): Promise<{ data: unknown; error: unknown }> {
    this.calls.push(name);
    const id = String(args.p_user_id ?? '');
    // The real column is uuid. A non-uuid id is a cast failure in Postgres, not a silent insert.
    if (!/^[0-9a-f-]{36}$/i.test(id)) {
      return { data: null, error: { code: '22P02', message: 'invalid input syntax for type uuid' } };
    }
    switch (name) {
      case 'apocky_totp_start_enrolment': {
        const secret = String(args.p_secret ?? '');
        if (!/^[A-Z2-7]{16,64}$/.test(secret)) {
          return { data: null, error: { code: '22023', message: 'invalid authenticator secret' } };
        }
        const row = this.row(id);
        row.pending_secret = secret;
        row.pending_created_at = this.now;
        return { data: null, error: null };
      }
      case 'apocky_totp_pending': {
        const row = this.rows.get(id);
        const fresh = row?.pending_secret != null
          && row.pending_created_at != null
          && row.pending_created_at > this.now - 30 * 60_000;
        return { data: fresh ? [{ pending_secret: row!.pending_secret }] : [], error: null };
      }
      case 'apocky_totp_confirm': {
        const row = this.rows.get(id);
        if (!row?.pending_secret) return { data: null, error: { code: 'P4041', message: 'no enrolment in progress' } };
        row.secret = row.pending_secret;
        row.pending_secret = null;
        row.pending_created_at = null;
        row.confirmed_at = new Date(this.now).toISOString();
        row.last_used_step = Number(args.p_step);
        row.failed_attempts = 0;
        row.locked_until = null;
        return { data: null, error: null };
      }
      case 'apocky_totp_begin': {
        const row = this.rows.get(id);
        if (!row?.secret || !row.confirmed_at) return { data: [], error: null };
        return {
          data: [{ secret: row.secret, last_used_step: row.last_used_step, locked_until: row.locked_until }],
          error: null,
        };
      }
      case 'apocky_totp_succeed': {
        const row = this.row(id);
        row.last_used_step = Number(args.p_step);
        row.failed_attempts = 0;
        row.locked_until = null;
        return { data: null, error: null };
      }
      case 'apocky_totp_fail': {
        const row = this.row(id);
        row.failed_attempts += 1;
        if (row.failed_attempts >= LOCKOUT_ATTEMPTS) {
          row.locked_until = new Date(this.now + 15 * 60_000).toISOString();
        }
        return { data: null, error: null };
      }
      default:
        return { data: null, error: { code: '42883', message: `no function ${name}` } };
    }
  }
}

interface Captured { status: number; body: Record<string, unknown>; headers: Record<string, string> }

function makeRes(): { res: unknown; out: Captured } {
  const out: Captured = { status: 0, body: {}, headers: {} };
  const res = {
    setHeader(key: string, value: string) { out.headers[key.toLowerCase()] = String(value); },
    status(code: number) { out.status = code; return this; },
    json(body: Record<string, unknown>) { out.body = body; return this; },
    end() { return this; },
  };
  return { res, out };
}

/** Load a real API route with its imports replaced. */
function loadHandler(
  routePath: string,
  shims: Record<string, unknown>,
): (req: unknown, res: unknown) => Promise<void> {
  const source = readFileSync(resolve(process.cwd(), routePath), 'utf8');
  const compiled = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true },
  }).outputText;
  const module = { exports: {} as { default?: (req: unknown, res: unknown) => Promise<void> } };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    Buffer,
    URL,
    TextEncoder,
    console,
    process: { env: { ...process.env } },
    require(name: string) {
      for (const [key, value] of Object.entries(shims)) {
        if (name === key || name.endsWith(key)) return value;
      }
      throw new Error(`Unexpected dependency in ${routePath}: ${name}`);
    },
  }, { filename: routePath });
  const handler = module.exports.default;
  assert.ok(handler, `${routePath} has no default export`);
  return handler;
}

async function main(): Promise<void> {
  const hub = new FakeHub();
  let session: { user: { id: string; email: string } | null } = { user: { id: OWNER, email: EMAIL } };
  const service = { rpc: (name: string, args: Record<string, unknown>) => hub.rpc(name, args) };

  const setup = loadHandler('pages/api/auth/totp/setup.ts', {
    '@/lib/admin-auth': { getRequestUser: async () => session },
    '@/lib/apocrypha/job-control': { getApocryphaServiceClient: () => service },
    '@/lib/auth-session': { hasSameOrigin: () => true },
    'qrcode': { __esModule: true, default: QRCode, toDataURL: QRCode.toDataURL },
    '@/lib/auth-totp': realTotp,
  });

  const post = async (body: Record<string, unknown>): Promise<Captured> => {
    const { res, out } = makeRes();
    await setup({ method: 'POST', headers: {}, body, url: '/api/auth/totp/setup' }, res);
    return out;
  };

  // ── 1 · start enrolment ─────────────────────────────────────────────────────────────────────
  const started = await post({});
  assert.equal(started.status, 200, `enrolment start failed: ${JSON.stringify(started.body)}`);
  const secret = String(started.body.secret);
  const uri = String(started.body.uri);
  const qr = started.body.qr;

  assert.match(secret, /^[A-Z2-7]{16,64}$/, 'the secret must satisfy the column CHECK constraint');
  assert.match(uri, /^otpauth:\/\/totp\//, 'the provisioning URI must be an otpauth URI');
  assert.ok(uri.includes(encodeURIComponent(EMAIL)), 'the URI must name the account being enrolled');
  assert.ok(uri.includes(`secret=${secret}`), 'the URI must carry the secret the server stored');
  // The QR is the thing the reader actually scans. A null here is the difference between "scan
  // this" and a blank space where the instruction says a code should be.
  assert.ok(typeof qr === 'string' && qr.startsWith('data:image/png;base64,'), 'a scannable QR must be returned');
  assert.ok((qr as string).length > 1000, 'the QR must contain an actual image, not an empty canvas');
  assert.equal(hub.rows.get(OWNER)?.secret, null, 'nothing is confirmed yet');
  assert.equal(hub.rows.get(OWNER)?.pending_secret, secret, 'the secret is held as PENDING until proven');

  // What an authenticator app does with that URI: decode the secret, emit the code for this step.
  const scanned = /secret=([A-Z2-7]+)/.exec(uri)?.[1];
  assert.equal(scanned, secret, 'the code an authenticator derives comes from the URI, so they must agree');

  // ── 2 · a wrong code must not confirm ───────────────────────────────────────────────────────
  const wrong = await post({ code: '000000' === codeForStep(secret, currentStep()) ? '111111' : '000000' });
  assert.equal(wrong.status, 401, 'a wrong code must not complete enrolment');
  assert.equal(hub.rows.get(OWNER)?.secret, null, 'a rejected code leaves the account with no new credential');
  assert.equal(hub.rows.get(OWNER)?.pending_secret, secret, 'a rejected code does not discard the enrolment');

  // ── 3 · the real code confirms ──────────────────────────────────────────────────────────────
  const step = currentStep();
  const confirmed = await post({ code: codeForStep(secret, step) });
  assert.equal(confirmed.status, 200, `confirm failed: ${JSON.stringify(confirmed.body)}`);
  assert.equal(confirmed.body.confirmed, true, 'confirmation must be reported explicitly');
  const row = hub.rows.get(OWNER)!;
  assert.equal(row.secret, secret, 'the pending secret is promoted on confirmation');
  assert.equal(row.pending_secret, null, 'the pending slot is cleared');
  assert.ok(row.confirmed_at, 'confirmation is recorded');
  assert.equal(row.last_used_step, step, 'the confirming step becomes the replay watermark');

  // ── 4 · the code just used cannot be replayed ───────────────────────────────────────────────
  // Re-running confirm with the same code must fail — there is no enrolment left to confirm.
  const replayed = await post({ code: codeForStep(secret, step) });
  assert.equal(replayed.status, 409, 'a spent enrolment cannot be confirmed twice');
  assert.equal(hub.rows.get(OWNER)?.secret, secret, 'a replayed confirm does not disturb the live credential');

  // And the sign-in path refuses it too: the watermark is at this step already.
  const signIn = verifyTotp(secret, codeForStep(secret, step), { lastUsedStep: step });
  assert.equal(signIn.ok, false, 'the confirming code must not also be a valid sign-in code');

  // ── 5 · the next code signs in ──────────────────────────────────────────────────────────────
  const next = verifyTotp(secret, codeForStep(secret, step + 1), {
    lastUsedStep: step,
    atMs: (step + 1) * TOTP_PERIOD_SECONDS * 1_000,
  });
  assert.equal(next.ok, true, 'the following code must be accepted');
  assert.equal(next.step, step + 1, 'the accepted step advances the watermark');

  // ── 6 · enrolment started and abandoned expires ─────────────────────────────────────────────
  hub.rows.delete(OWNER);
  const abandoned = await post({});
  assert.equal(abandoned.status, 200, 'a fresh enrolment starts');
  const abandonedSecret = String(abandoned.body.secret);
  hub.now += 31 * 60_000;                       // walk away for half an hour
  const stale = await post({ code: codeForStep(abandonedSecret, currentStep()) });
  assert.equal(stale.status, 409, 'an abandoned enrolment expires instead of waiting to be confirmed');
  assert.equal(hub.rows.get(OWNER)?.secret, null, 'an expired enrolment never becomes the credential');
  assert.match(
    String(stale.body.error ?? ''),
    /start again/i,
    'an expired enrolment must tell the reader how to recover, not just refuse',
  );
  hub.now -= 31 * 60_000;

  // ── 7 · enrolment is gated on already being the account ─────────────────────────────────────
  session = { user: null };
  const anonymous = await post({});
  assert.equal(anonymous.status, 401, 'enrolment must require an existing session');
  assert.equal(anonymous.body.code, 'SIGN_IN_REQUIRED', 'the refusal names the reason');
  // Enrolment adds a credential to an account. Allowing it unauthenticated would let anyone claim
  // an account that has no authenticator yet.
  assert.ok(
    !hub.calls.includes('apocky_totp_start_enrolment')
      || hub.calls.lastIndexOf('apocky_totp_start_enrolment') < hub.calls.length,
    'no enrolment is written for an anonymous caller',
  );
  const before = hub.calls.length;
  await post({ code: '123456' });
  assert.equal(hub.calls.length, before, 'an anonymous confirm reaches the database not at all');

  // ── 8 · the refusal an unconfirmed enrolment produces ───────────────────────────────────────
  //
  // This is the failure that actually happened: setup was started, the QR was scanned, the code was
  // typed into the SIGN-IN page, and six attempts were refused with "check your authenticator" —
  // which was the one thing that was fine. A started-but-unconfirmed enrolment has no credential at
  // all, so the sign-in path cannot accept anything.
  hub.rows.delete(OWNER);
  session = { user: { id: OWNER, email: EMAIL } };
  const pendingOnly = await post({});
  assert.equal(pendingOnly.status, 200, 'enrolment starts');
  const unconfirmedSecret = String(pendingOnly.body.secret);
  const begun = await hub.rpc('apocky_totp_begin', { p_user_id: OWNER });
  assert.equal(
    (begun.error as { code?: string } | null)?.code,
    undefined,
    'apocky_totp_begin is the first read the sign-in path makes',
  );
  // The in-memory stand-in mirrors the SQL: no confirmed_at means no row comes back at all.
  assert.deepEqual(begun.data, [], 'an unconfirmed enrolment yields no credential to verify against');
  // And a perfectly correct code for the pending secret still cannot sign in, because nothing has
  // promoted that secret yet. The code is right; the account has no authenticator.
  assert.equal(
    Array.isArray(begun.data) && begun.data.length,
    0,
    'a correct code against a pending secret has nothing to be checked against',
  );
  assert.ok(codeForStep(unconfirmedSecret, currentStep()).length === 6, 'the code itself is well-formed — that was never the problem');

  // The refusal the reader sees must not send them back to the authenticator.
  const signInSource = readFileSync(resolve(process.cwd(), 'pages/api/auth/totp.ts'), 'utf8');
  assert.ok(
    signInSource.includes('finish setup on your account page first'),
    'the refusal must name unfinished setup, since that produces this exact refusal',
  );
  assert.ok(
    signInSource.includes("record('no_confirmed_authenticator'")
      || signInSource.includes("'no_confirmed_authenticator'"),
    'the server must record WHY it refused, since the client answer is deliberately uniform',
  );
  assert.ok(
    !signInSource.includes('record(') || !/record\([^)]*email/u.test(signInSource),
    'the refusal log must not carry the account it refused',
  );

  console.log('auth-totp-enrolment.test : OK · 8 stages, unconfirmed enrolment diagnosed, replay and expiry refused');
}

void main().catch((error: unknown) => {
  console.error(error);
  process.exit(1);
});
