// What happens after a sign-in succeeds.
//
// The failure mode this guards is quiet: a status check that is slow, broken, or lying pushes a
// reader who ALREADY has an authenticator at a screen offering to replace it, or worse, blocks a
// sign-in that has already succeeded. The session is real by the time this code runs; the offer is
// a courtesy and must behave like one.

import assert from 'node:assert/strict';

import { authenticatorSetupHref, continueAfterSignIn, fetchEnrolmentStatus } from '@/lib/auth-continue';

function respond(body: unknown, status = 200): typeof fetch {
  return (async () => new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  })) as unknown as typeof fetch;
}

async function destinationFor(fetchImpl: typeof fetch, alreadyOffered = false): Promise<string> {
  let went = '';
  await continueAfterSignIn('/apocrypha', {
    fetchImpl,
    navigate: (url) => { went = url; },
    alreadyOffered,
  });
  return went;
}

async function main(): Promise<void> {
  // ── an account with no authenticator is offered one, as the last step of signing in ──────────
  assert.equal(
    await destinationFor(respond({ ok: true, enrolled: false })),
    '/login?next=%2Fapocrypha&setup=authenticator',
    'an account with no authenticator is offered one before it reaches its destination',
  );

  // ── an account that has one is never asked again ─────────────────────────────────────────────
  assert.equal(
    await destinationFor(respond({ ok: true, enrolled: true })),
    '/apocrypha',
    'an enrolled account goes straight through',
  );
  assert.equal(
    await destinationFor(respond({ ok: true, enrolled: true, locked: true })),
    '/apocrypha',
    'a locked-out authenticator is still an authenticator; do not offer to replace it',
  );

  // ── a broken check must never block or misdirect a sign-in that already succeeded ────────────
  for (const [label, impl] of [
    ['a 502 from the status route', respond({ ok: false, code: 'ENROLMENT_STATUS_UNAVAILABLE' }, 502)],
    ['a 401 from the status route', respond({ ok: false }, 401)],
    ['a malformed payload', respond({ ok: true })],
    ['a non-boolean enrolled field', respond({ ok: true, enrolled: 'no' })],
    ['garbage instead of JSON', (async () => new Response('<html>', { status: 200 })) as unknown as typeof fetch],
    ['the network throwing', (async () => { throw new Error('offline'); }) as unknown as typeof fetch],
  ] as Array<[string, typeof fetch]>) {
    assert.equal(
      await destinationFor(impl),
      '/apocrypha',
      `${label} must send the reader where they asked to go, not to an enrolment screen`,
    );
  }

  // ── a hung check must not strand a completed sign-in ─────────────────────────────────────────
  const hung = (async () => new Promise<Response>(() => undefined)) as unknown as typeof fetch;
  const started = Date.now();
  const status = await fetchEnrolmentStatus(hung, 60);
  assert.equal(status.known, false, 'a hung check resolves as unknown rather than pending forever');
  assert.ok(Date.now() - started < 3_000, 'a hung check is abandoned at its deadline');

  // ── the offer is made once per sign-in, not in a loop ────────────────────────────────────────
  assert.equal(
    await destinationFor(respond({ ok: true, enrolled: false }), true),
    '/apocrypha',
    'declining the offer and completing again must not present it a second time',
  );

  // ── the destination survives being carried through the offer ─────────────────────────────────
  assert.equal(
    authenticatorSetupHref('/account#authenticator'),
    '/login?next=%2Faccount%23authenticator&setup=authenticator',
    'the place the reader was going is preserved through the enrolment step, fragment included',
  );

  console.log('auth-continue.test : OK · offer once, never block, never nag an enrolled account');
}

void main().catch((error: unknown) => {
  console.error(error);
  process.exit(1);
});
