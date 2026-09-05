# Apocrypha V2 frontend deployment evidence

The repository configuration is a requested bound, not proof of the deployed
platform's effective execution limit. A release receipt must capture all items
below from the deployed candidate before public chat is called operational.

- Record deployment ID, Git commit, build artifact identity, UTC timestamp, and operator.
- Inspect the deployed `chat.ts` function in Vercel and record its observed effective `maxDuration`.
- Prove the configured 30-second bound was applied; do not infer this from `vercel.json`.
- Run an authenticated public turn through `www.apocky.com` and retain status, elapsed time,
  request ID, conversation reference, transition ID, and StateRoot without private content.
- Verify the frontend aborts the body request at 25 seconds and the browser request at 28 seconds.
- Prove incomplete, non-committed, externally inferred, wrong-expression, or wrong-continuity
  upstream envelopes fail closed and are never rendered as Apocrypha speech.
- Verify `after_event_seq` telemetry polling is monotonic and rejects malformed, decreasing,
  skipped-empty, and unsafe-integer cursor envelopes.
- Confirm all predecessor admin endpoints return authenticated `410 Gone` and make no upstream call.
- Confirm `/api/admin/check` is private, no-store, no-cache, and varies on Authorization and Cookie.
- Record that turn retries lack backend duplicate-commit protection until `/v2/turn` accepts and
  atomically enforces a request idempotency commitment. Frontend request identity alone is not proof.
- Exercise desktop, mobile, keyboard, reduced-motion, loading, success, timeout, malformed-envelope,
  and unauthorized states. Attach screenshots or a signed browser-run artifact.
- Validate `/.well-known/apocky.json` against `/schemas/site-manifest.v1.json`; confirm its public,
  account, owner, private-beta, consent, method, and media-type declarations match deployed routes.
- Confirm `/llms.txt`, `/robots.txt`, `/sitemap.xml`, and the public PWA manifest are reachable and
  advertise no private control route as public or account-only route as generally authorized.
- From a fresh browser profile, prove no Akashic identifier, listener, observer, event, storage write,
  or network request occurs before an explicit telemetry choice. Repeat on login, register, callback,
  and account recovery surfaces; auth routes remain telemetry-blackout even after prior consent.
- Exercise email link, pasted email code, resend, expired/incorrect code, change-email, OAuth cancel,
  callback retry, malicious `next`, safe `next=/chat`, and sign-out. Authentication must never imply
  owner authorization.
- Run the Playwright outcome suite in desktop and mobile Chrome, run the serious/critical Axe gate,
  inspect deterministic 2560x1080, 1440x900, 834x1112, 390x844, and 320x568 renders, and prove no
  horizontal overflow, clipped primary action, hydration error, unexpected console error, or hidden
  focus state.
- Verify every account control either performs its stated durable operation or is visibly disabled
  and labeled unavailable. Local-only profile data must never be described as synced or public.

No release receipt may label chat, voice, telemetry, or diagnostics public and operational from a
source build alone. Deployment and authenticated runtime evidence are separate gates.
