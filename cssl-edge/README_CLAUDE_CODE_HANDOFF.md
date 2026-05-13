# Claude Code Handoff README: cssl-edge

`cssl-edge` is the active browser-facing site and API layer for `apocky.com`.
It is a Next.js Pages Router application deployed to Vercel under the project
`apocky-com`.

This document is intended for Claude Code handoff. It is practical: where to
run the app, which routes matter, how auth works, how Lazarus is wired, what
files contain which functions, and what to verify before saying a fix is done.

## Current Production Snapshot

Active Vercel project:

```text
apocky-com
```

Current production deployment at handoff time:

```text
https://apocky-gecoig9t4-shawn-bakers-projects-cb1c9715.vercel.app
```

Aliases pointing at production:

```text
https://apocky.com
https://www.apocky.com
https://apocky-com.vercel.app
```

Known-good production checks at handoff time:

```text
https://apocky.com                         -> 200
https://www.apocky.com                     -> 200
https://www.apocky.com/?code=shape-only    -> 200
https://www.apocky.com/login               -> 200
https://www.apocky.com/auth/callback       -> 200
https://www.apocky.com/admin/chat          -> 200
https://www.apocky.com/admin/lazarus       -> 200
https://www.apocky.com/admin/tessera-omnimind -> 200
https://www.apocky.com/api/health          -> 200
```

Unauthenticated admin/Lazarus reads intentionally return 401:

```text
/api/admin/bridge?action=status
/api/admin/bridge?action=send
/api/admin/lazarus/health
/api/admin/lazarus/tasks
/api/admin/lazarus/runners
```

## Local Browser Setup

From PowerShell:

```powershell
cd C:\Users\Apocky\source\repos\CSSLv3\cssl-edge
npm install
npm run dev
```

Open:

```text
http://localhost:3000
```

For a clean auth-dev start, this helper exists:

```powershell
.\start-auth-dev.bat
```

For Lazarus local control-plane and runner windows, this helper exists:

```powershell
.\start-lazarus.bat
```

Do not run a long-lived runner against production unless the user explicitly
asks. Do not enable real model calls unless the user explicitly asks.

## Validation Commands

Run these before reporting completion for auth, admin, or Lazarus work:

```powershell
npm run test:auth-redirect
npm run test:health
npm run test:lazarus
npm run test:admin-chat-lib
npm run test:admin-chat-page
npm run test:lazarus-page
npm run test:tessera-bridge
npm run test:tessera-runner
npm run test:tessera-omnimind-page
npm run check
npm run build
```

The broad test command exists, but focused tests are faster while iterating:

```powershell
npm test
```

## Deployment Commands

Deploy production from this directory:

```powershell
vercel --prod
```

After deploy, verify aliases:

```powershell
vercel alias ls
```

Then smoke the public URLs and the Lazarus unauthenticated 401 gates.

Do not redeploy for local-only documentation unless the user wants the docs on
the public site.

## Secret Handling

`.env.local` exists and may contain real secrets. Do not print it, copy it into
chat, or commit it.

Environment variables used by this app:

```text
APOCKY_HUB_SUPABASE_URL
APOCKY_HUB_SUPABASE_ANON_KEY
NEXT_PUBLIC_SUPABASE_URL
NEXT_PUBLIC_SUPABASE_ANON_KEY
SUPABASE_URL
SUPABASE_ANON_KEY
SUPABASE_SERVICE_ROLE_KEY
APOCKY_ADMIN_EMAILS
LAZARUS_RUNNER_TOKEN
LAZARUS_CONTROL_URL
LAZARUS_RUNNER_ID
LAZARUS_RUNNER_LABEL
LAZARUS_ENABLE_MODEL_CALLS
DEEPSEEK_API_KEY
STRIPE_SECRET_KEY
STRIPE_WEBHOOK_SIGNING_SECRET
CRON_SECRET
ANTHROPIC_API_KEY
VOYAGE_API_KEY
CLAUDE_API_KEY
ADMIN_CHAT_MODEL
ADMIN_CHAT_DEEPSEEK_MODEL
ADMIN_CHAT_ANTHROPIC_MODEL
```

At handoff time `LAZARUS_RUNNER_TOKEN` is configured in Vercel production,
preview, development, and local `.env.local`. The value is secret.

## Package And Scripts

Project basics from `package.json`:

```text
name: cssl-edge
Next.js: 14.2.5
React: 18.3.1
TypeScript: 5.5.3
Node: >=18.17.0
Router: Pages Router
```

Important scripts:

```text
predev / prebuild       node scripts/snapshot-specs.js
dev                     next dev
build                   next build
start                   next start
check                   tsc --noEmit
test:auth-redirect      tsx tests/lib/auth-redirect.test.ts
test:health             tsx tests/api/health.test.ts
test:lazarus            tsx tests/api/lazarus.test.ts
test:admin-chat-lib     tsx tests/lib/admin-chat.test.ts
test:admin-chat-page    tsx tests/pages/admin-chat.test.ts
test:lazarus-page       tsx tests/pages/lazarus-page.test.ts
test:tessera-bridge     tsx tests/lib/tessera-bridge.test.ts
test:tessera-runner     tsx tests/lib/tessera-runner.test.ts
test:tessera-omnimind-page  tsx tests/pages/tessera-omnimind.test.ts
lazarus:runner          tsx scripts/lazarus-runner.ts
lazarus:service         Windows service status wrapper for the runner
lazarus:service:smoke   One-shot production runner smoke; requires explicit live model config for model execution
```

## Lazarus Production MVP Runner

The control plane deploys on Vercel, but the runner is a long-lived polling
process and should not be hosted in Vercel serverless functions. For the current
Windows machine, use the NSSM-backed service script:

```powershell
cd C:\Users\Apocky\source\repos\CSSLv3\cssl-edge
npm run lazarus:service:smoke
```

The smoke command runs one loop against `https://www.apocky.com` with:

```text
LAZARUS_ONCE=1
LAZARUS_ENABLE_MODEL_CALLS=1 for live model execution
```

It reads `LAZARUS_RUNNER_TOKEN` from the current process or ignored `.env.local`.
The token must never be printed.

Install the persistent runner from an elevated PowerShell session after NSSM is
installed and on `PATH`:

```powershell
npm run lazarus:service:install
npm run lazarus:service:start
npm run lazarus:service
```

Service defaults:

```text
service name: ApockyLazarusRunner
control URL: https://www.apocky.com
runner id: apocky-windows-runner
model calls: fail closed unless LAZARUS_ENABLE_MODEL_CALLS=1 and DEEPSEEK_API_KEY are configured
logs: C:\ProgramData\Apocky\Lazarus\logs
```

`/admin/chat` is a single chat function now. Do not split it into GM, DM,
guide, or coder modes in the UI. The server route `/api/admin/bridge` calls a
real configured model provider and returns 503 when no model key is available;
it must not fabricate assistant responses.

Do not enable `LAZARUS_ENABLE_MODEL_CALLS=1` for the service until the admin UI,
Supabase persistence, and one-shot runner smoke all pass.

## High-Level Directory Map

```text
components/             React shared UI and admin wrappers
lib/                    server/browser helpers, auth, telemetry, stores
lib/lazarus/            Lazarus auth guards, data store, types
pages/                  Next.js pages and API routes
pages/api/              serverless API routes
public/                 static assets and manifest files
scripts/                local tooling and Lazarus runner
tests/                  executable TypeScript smoke/unit tests
vercel.json             Vercel build, regions, cron, headers
.env.example            documented env names with empty values
```

Generated/local directories:

```text
.next/
.vercel/
node_modules/
tsconfig.tsbuildinfo
```

Do not document generated files as source of truth.

## Source Inventory

Use this section as a routing map before opening files.

### Components

```text
components/AdminLayout.tsx
  Protected admin shell. Calls /api/admin/check through authFetch and hides
  children until authorization succeeds.

components/AkashicConsent.tsx
  First-visit diagnostics consent banner. Non-blocking and hidden on auth pages.

components/Callout.tsx
  Shared documentation callout component.

components/CodeBlock.tsx
  Shared documentation code block renderer.

components/ContentCard.tsx
  Content feed/list item card.

components/ContentDetail.tsx
  Content detail display component.

components/ContentFeed.tsx
  Content feed layout/renderer.

components/DocsLayout.tsx
  Documentation page layout and navigation shell.

components/PrevNextNav.tsx
  Documentation previous/next navigation.

components/charts/
  Chart components used by admin/analytics surfaces.

components/engine/
  Engine-specific UI components.
```

### Library Helpers

```text
lib/admin-auth.ts
  Server-side bearer/cookie auth resolution and admin allowlist checks.

lib/admin-metrics.ts
  Admin metrics support helpers.

lib/akashic-telemetry/
  Client/server telemetry package: consent, event types, error boundary,
  network/performance taps, and sigma masking.

lib/analytics.ts
  Analytics helper logic.

lib/audit.ts
  Audit/event helper logic for protected operations.

lib/auth.ts
  Supabase auth client, redirect safety, provider config, cookie persistence.

lib/auth-callback.ts
  Shared OAuth/PKCE callback parser and consumer.

lib/browser-auth.ts
  Browser authFetch wrapper and bearer-header bridge.

lib/cap.ts
  Capability/cap-bit helper logic.

lib/content-fetch.ts
  Content read/fetch helpers for content pages and APIs.

lib/content-publish.ts
  Content publishing workflow helpers.

lib/cron-auth.ts
  Cron request authorization helpers.

lib/devblog-posts.ts
  Devblog content indexing/loading.

lib/docs-content.ts
  Docs content indexing/loading.

lib/engine-status.ts
  Engine/status helpers.

lib/lazarus/
  Lazarus auth guards, store, and shared types.

lib/license_filter.ts
  Asset/license allowlist and filtering logic.

lib/markdown.ts
  Markdown parsing/render helpers.

lib/mneme/
  MNEME memory helpers: Anthropic/Voyage adapters, CSL extraction, ingest,
  retrieve, prompts, sigma masks, storage, and types.

lib/realtime.ts
  Realtime helper logic.

lib/response.ts
  Standard API response envelope, served_by, timestamps, commit SHA, hit logs.

lib/sovereign.ts
  Sovereign identity/capability helper logic.

lib/specs-snapshot.ts
  Reads generated specs snapshot used by docs/build scripts.

lib/sse.ts
  Server-sent events helper logic.

lib/stripe.ts
  Stripe/payment helper logic.

lib/supabase.ts
  Supabase server/client helper setup for data routes.

lib/tessera/
  Inert Lazarus -> Tessera bridge DTOs, dry-run adapter, and runner event projection. No I/O or model calls.
```

### Scripts

```text
scripts/snapshot-specs.js
  Runs before dev/build to snapshot specs for the docs/site layer.

scripts/lazarus-runner.ts
  Local Lazarus runner loop. Uses runner token and optional DeepSeek calls.

scripts/dist-build-mycelium.cmd
scripts/dist-build-mycelium.sh
scripts/README-dist-build-mycelium.md
  Mycelium distribution build helpers and documentation.
```

### Tests

```text
tests/lib/auth-redirect.test.ts
  Auth redirect and callback parser tests.

tests/lib/sovereign.test.ts
  Sovereign helper tests.

tests/api/health.test.ts
tests/api/health-w9.test.ts
  Health endpoint tests.

tests/api/lazarus.test.ts
  Lazarus auth, health, queue, runner, event, run, and approval tests.

tests/api/asset-recommend.test.ts
tests/api/marketplace-list.test.ts
tests/api/marketplace-post.test.ts
  Asset/marketplace route tests.

tests/api/companion.test.ts
tests/api/companion-stream.test.ts
  Companion API tests.

tests/api/signaling.test.ts
tests/api/mp-rendezvous-lobby.test.ts
  Multiplayer signaling/lobby tests.

tests/api/stripe-checkout.test.ts
tests/api/stripe-refund.test.ts
tests/api/stripe-webhook.test.ts
  Stripe payment route tests.

tests/api/transparency-*.test.ts
  Transparency endpoint tests.

tests/api/run-share-*.test.ts
  Run sharing feed/submit tests.

tests/api/content/
  Content publishing/moderation/subscription tests.

tests/api/mneme/
  MNEME memory route tests.
```

## Route Map

Core pages:

```text
/                         auth-aware project hub
/login                    OAuth provider login page
/register                 registration page
/auth/callback            Supabase OAuth/PKCE callback consumer
/account                  user account/session page
/admin                    admin landing page
/admin/lazarus            Lazarus control plane UI
/admin/tessera-omnimind   Tessera/Lazarus bridge cockpit UI
/docs/*                   documentation pages
/legal/*                  legal/privacy pages
/download                 LoA download page
```

Recent auth/admin API routes:

```text
/api/auth/me              server-side session/user lookup
/api/auth/logout          clears auth cookies and signs out
/api/auth/magic-link      magic-link sign-in route
/api/auth/oauth           legacy OAuth fallback route
/api/admin/check          admin authorization check for AdminLayout
/api/health               liveness and integration booleans
```

Lazarus API routes:

```text
GET/POST /api/admin/lazarus/tasks      admin creates/lists work queue
GET      /api/admin/lazarus/health     admin reads control-plane health
GET/POST /api/admin/lazarus/runners    GET admin, POST runner-token registration
POST     /api/admin/lazarus/lease      runner-token task leasing
GET/POST /api/admin/lazarus/events     GET admin, POST runner-token events
GET/POST /api/admin/lazarus/runs       GET admin, POST runner-token finish run
GET/POST /api/admin/lazarus/approvals  admin approval request/decision flow
GET      /api/admin/lazarus/fleet      admin reads model/budget fleet config
GET      /api/admin/lazarus/tools      admin reads available tool specs
```

Other major API groups present in this app:

```text
/api/akashic/*            diagnostics/telemetry intake and purge
/api/analytics/*          analytics events and metrics
/api/asset/*              asset search, recommendation, GLB proxy
/api/battle-pass/*        battle pass progress/redeem/unlock
/api/companion*           companion relay/stub and stream route
/api/content/*            publishing, moderation, ratings, subscriptions
/api/cron/*               Vercel cron jobs from vercel.json
/api/gacha/*              gacha pull/history/refund
/api/generate/3d          neural 3D gateway
/api/hotfix/*             hotfix manifest/download/status/revoke
/api/intent               text-to-scene intent endpoint
/api/marketplace/*        marketplace list/post
/api/mneme/*              memory profile operations
/api/payments/stripe/*    checkout/refund/webhook
/api/signaling/*          multiplayer room/signaling helpers
/api/transparency/*       transparency endpoints
```

## Auth Flow

Auth is Supabase browser auth with PKCE. Important behavior:

1. OAuth starts on `/login` client-side. PKCE requires browser storage.
2. Provider redirect target is usually `/auth/callback`.
3. Supabase or provider config can still return to `/` with `?code=...`.
4. Root `/` is now resilient and consumes callback params through
   `lib/auth-callback.ts`.
5. Successful callback persists `sb-access-token` and `sb-refresh-token`
   cookies so server routes can resolve the user.
6. Browser API calls use `authFetch()` to add `Authorization: Bearer <token>`
   and refresh the cookies opportunistically.
7. Server admin checks read bearer auth first, then `sb-access-token` cookie.

If login appears to succeed but the home page still says Sign in:

1. Inspect URL for `?code=...` or hash tokens.
2. Inspect `lib/auth-callback.ts` behavior.
3. Inspect browser cookies `sb-access-token` and `sb-refresh-token`.
4. Call `/api/auth/me` with `authFetch()` from the browser.
5. Ensure no consent UI covers links/buttons.

## Consent And Click Blocker History

There were two blockers that made buttons look non-interactive:

1. A Termly global resource blocker injected in `_document.tsx`.
2. A full-screen Akashic consent `aria-modal` overlay.

Current state:

```text
pages/_document.tsx              Termly resource blocker removed
components/AkashicConsent.tsx    non-blocking fixed banner
components/AkashicConsent.tsx    hidden on /login, /register, /auth/callback
pages/_app.tsx                   still renders AkashicConsent globally
```

Rule: auth pages must remain clear of modal overlays and global banners.

## Admin And Lazarus Security

Admin identity is email allowlist based. Default allowlist includes
`apocky13@gmail.com`; production may override through `APOCKY_ADMIN_EMAILS`.

Server auth helpers live in `lib/admin-auth.ts`. Lazarus route guards live in
`lib/lazarus/auth.ts`.

Expected behavior:

```text
Unauthenticated admin read -> 401
Signed-in non-admin read   -> 403
Signed-in admin read       -> 200
Runner write no token      -> 401
Runner write wrong token   -> 401
Runner write right token   -> 200 or route-specific success
Missing runner token env   -> 503 fail-closed
```

## Lazarus Control Plane

Lazarus is the autonomous coding runner control plane. It is online as a
secured API/UI surface, not as a permanently running local process.

Main browser UI:

```text
/admin/lazarus
```

Main local runner command:

```powershell
npm run lazarus:runner
```

Model calls are disabled unless:

```text
LAZARUS_ENABLE_MODEL_CALLS=1
DEEPSEEK_API_KEY is configured
```

When disabled, the runner uses safe stub summaries. Do not enable model calls
without explicit user approval.

## Tessera / OmniMind Synthesis

Tessera is the OmniMindv2 cognitive architecture: sub-minds, LR tiers,
metacognitive routing, memory, confidence, and cost calibration. Lazarus is the
operations layer: task queue, runner, approvals, events, persistence, and admin
UI.

Current synthesis rule:

```text
Lazarus operates Tessera.
Tessera becomes Lazarus' cognitive backend.
```

The operational roadmap lives in:

```text
docs/TESSERA_OMNIMIND_ROADMAP.md
```

The canonical architecture spec lives in:

```text
C:\Users\Apocky\source\repos\Omnimindv2\specs\06_LAZARUS_TESSERA_SYNTHESIS.csl.md
```

Do not wire live Tessera or DeepSeek execution into production Lazarus until the
bridge dry-run, cost caps, approval gates, and event trace have passing tests.
The first bridge should be inert unless `LAZARUS_TESSERA_BRIDGE=1` is set.

Current inert bridge files:

```text
lib/tessera/types.ts       TesseraGoalEnvelope, TesseraResult, event/cost/policy DTOs
lib/tessera/bridge.ts      LazarusTask + LazarusRun -> TesseraGoalEnvelope + dry-run result
lib/tessera/runner-client.ts  Runner-facing dry-run submission wrapper + event projection
tests/lib/tessera-bridge.test.ts
tests/lib/tessera-runner.test.ts
pages/admin/tessera-omnimind.tsx
tests/pages/tessera-omnimind.test.ts
```

Bridge validation command:

```powershell
npm run test:tessera-bridge
npm run test:tessera-runner
npm run test:tessera-omnimind-page
```

## Key File And Function Map

### Auth

`lib/auth.ts`

```text
resolveAuthRedirect(redirectTo, headers)
  Sanitizes redirect targets and falls back to /account on a trusted origin.

getAuthClient()
  Creates/caches the Supabase browser client. Uses APOCKY_HUB_* or NEXT_PUBLIC_*
  variables. Configured with PKCE and detectSessionInUrl:false.

persistSessionToCookie(accessToken, refreshToken?)
  Writes sb-access-token and sb-refresh-token cookies for server-side API reads.

signInWithMagicLink(email, redirectTo)
  Sends Supabase magic-link auth when the client is configured.

signInWithOAuth(provider, redirectTo)
  Starts OAuth through Supabase. Current login page mostly does its own explicit
  provider call to support PKCE browser flow.

signOut()
  Calls Supabase signOut when configured.

getCurrentUser()
  Reads the current browser user from Supabase.
```

Constants:

```text
AUTH_PROVIDERS      Google, Apple, GitHub, Discord enabled; X/Twitter and Spotify disabled
APOCKY_CHANNELS     external profile/support links
PROFILE_LINKABLE    user profile social link options
```

`lib/auth-callback.ts`

```text
readAuthCallbackParams(search, hash)
  Detects provider errors, PKCE query code, and implicit hash tokens.

clearAuthCallbackFromLocation()
  Removes auth callback query/hash data from the current URL with history.replaceState.

consumeAuthCallbackFromLocation()
  Exchanges PKCE code or hash tokens for a Supabase session, persists cookies,
  clears URL auth params, and returns handled/ok/reason state.
```

`lib/browser-auth.ts`

```text
getBrowserAuthHeaders()
  Reads the browser Supabase session, persists cookies, and returns bearer headers.

authFetch(input, init)
  Wraps fetch with current bearer headers. Use this from protected browser pages.
```

`lib/admin-auth.ts`

```text
getAdminAllowlist()
  Reads APOCKY_ADMIN_EMAILS or falls back to the default admin email.

getAccessTokenFromRequest(req)
  Reads Authorization: Bearer first, then sb-access-token cookie.

getRequestUser(req, timeoutMs)
  Validates the token with Supabase and returns normalized user info.

getAdminAuthorization(req, timeoutMs)
  Adds allowlist authorization to getRequestUser().
```

### Lazarus

`lib/lazarus/auth.ts`

```text
requireAdmin(req, res)
  API guard for admin routes. Sends 401/403 JSON and returns false on failure.

requireRunnerToken(req, res)
  API guard for runner routes. Uses timing-safe token compare and fails closed
  with 503 when LAZARUS_RUNNER_TOKEN is missing.
```

`lib/lazarus/store.ts`

```text
isLazarusStubMode()
  True when Supabase service-role backing is unavailable.

listLazarusTools()
  Returns tool specs exposed to the Lazarus UI.

getLazarusHealth()
  Returns runner/task/run/approval counts and stub state.

listTasks()
  Reads the task queue.

createTask(input)
  Creates an admin-submitted Lazarus task.

listRunners()
  Reads registered runners.

registerRunner(input)
  Runner-token endpoint writes/updates runner heartbeat metadata.

leaseNextTask(runner_id)
  Runner-token endpoint leases the next queued task and creates a run.

listRuns()
  Reads execution attempts.

listEvents(run_id?)
  Reads run event streams.

recordEvent(run_id, kind, message, level, payload)
  Appends runner events.

finishRun(run_id, status, summary, cost, metadata)
  Completes/fails/cancels a run and updates task/runner state.

listApprovals()
  Reads pending and historical approval gates.

requestApproval(run_id, gate, reason, payload)
  Creates an approval gate.

decideApproval(id, status, decided_by)
  Approves or denies a gate.

listFleetConfig()
  Reads privacy/model/budget routing configuration.

resetLazarusMemoryForTests()
  Clears in-memory fallback state for tests.
```

`lib/lazarus/types.ts`

```text
JsonValue / JsonRecord              JSON helper types
LazarusTaskStatus                   queued/leased/running/blocked/completed/failed/cancelled
LazarusRunStatus                    run attempt status
LazarusRunnerStatus                 online/offline/revoked
LazarusApprovalStatus               pending/approved/denied/expired
LazarusEventLevel                   info/warn/error/debug
LazarusModelMode                    deepseek-v4-pro/deepseek-v4-flash/reviewer/stub-safe (legacy type; UI does not offer stub-safe)
LazarusTask                         queued work item
LazarusRun                          execution attempt
LazarusRunner                       registered runner heartbeat
LazarusEvent                        append-only run event
LazarusApproval                     approval gate
LazarusArtifact                     diff/log/screenshot/trace/report metadata
LazarusFleetConfig                  model/privacy/budget config
LAZARUS_APPROVAL_GATES              hard gate identifiers
LazarusToolSpec                     UI-visible tool capability
LazarusHealth                       health response shape
CreateLazarusTaskInput              admin task creation input
RegisterRunnerInput                 runner registration input
LeaseResult                         runner lease response
```

### Pages And Components

`pages/index.tsx`

```text
Home
  Auth-aware project hub. Consumes root-level Supabase callback params, checks
  browser and server session state, and shows Lazarus only when authenticated.
```

`pages/login.tsx`

```text
Login
  OAuth provider login UI. Starts Supabase PKCE browser OAuth with redirect to
  /auth/callback.
```

`pages/auth/callback.tsx`

```text
AuthCallback
  Official callback page. Uses consumeAuthCallbackFromLocation(), shows status,
  and redirects to /account on success.
```

`pages/account.tsx`

```text
Account page
  Mirrors browser session to cookies, calls /api/auth/me, and falls back to
  browser Supabase identity when server lookup is temporarily unavailable.
```

`components/AdminLayout.tsx`

```text
AdminLayout
  Protected admin shell. Calls /api/admin/check with authFetch and renders
  children only after authorization succeeds.
```

`pages/admin/lazarus.tsx`

```text
LazarusConsole
  Admin UI for health, tasks, runners, runs, events, approvals, tools, and fleet
  config. Uses authFetch for API calls.

statusColor(status)
  Local UI color helper for task/run/approval statuses.
```

`components/AkashicConsent.tsx`

```text
AkashicConsent
  First-visit diagnostics consent banner. Non-blocking. Hidden on /login,
  /register, and /auth/callback.
```

`pages/_app.tsx`

```text
App
  Installs Akashic telemetry/error boundary and renders AkashicConsent globally.

attachGlobalErrorListeners()
  Adds window error and unhandled rejection listeners once per browser session.
```

`pages/_document.tsx`

```text
Document
  Sets dark html/body baseline, manifest/icons, and pre-hydrate error catcher.
  Termly script has been removed.
```

### API Helpers

`pages/api/health.ts`

```text
handler(req, res)
  Always returns 200 plus sha, served_by, ts, and config booleans. Does not leak
  secret values.

testHealthCarriesW9Keys()
  Inline test for health boolean keys.

testHealthPaymentsReadyComposite()
  Inline test for payments_ready composite logic.
```

`pages/api/auth/me.ts`

```text
handler(req, res)
  Returns current user from getRequestUser(req), using bearer or cookie auth.
```

`pages/api/admin/check.ts`

```text
handler(req, res)
  Returns authorized/email/stub/reason for AdminLayout. Unauthorized state is a
  clean JSON response rather than a crashed page.
```

`scripts/lazarus-runner.ts`

```text
loadDotEnvLocal()
  Loads local env values for the runner.

config()
  Reads Lazarus runner configuration from env.

postJson(cfg, path, body)
  Sends runner-token authenticated POSTs to the control plane.

register(cfg)
  Registers or refreshes the runner.

emit(cfg, runId, kind, message, level)
  Appends run events.

callDeepSeek(cfg, task)
  Calls DeepSeek only when model calls are enabled and a key exists.

processLease(cfg, lease)
  Handles a leased task, emits events, and finishes the run.

main()
  Runner loop entrypoint.
```

### Tests

`tests/lib/auth-redirect.test.ts`

```text
testProductionRedirects()
  Ensures production redirects stay on trusted origins.

testPreviewAndLocalhostRedirects()
  Ensures Vercel previews and localhost dev redirects work.

testAuthCallbackParamParsing()
  Ensures PKCE query codes and implicit hash tokens are detected.
```

`tests/api/lazarus.test.ts`

```text
testLazarusAuthGates()
  Confirms admin and runner-token gates fail/succeed correctly.

testLazarusHealthAndTools()
  Confirms health/tools responses work.

testLazarusTaskLeaseEventRun()
  Confirms task creation, runner registration, leasing, events, and finishing.

testLazarusApprovals()
  Confirms approval request and decision flow.
```

## Vercel Configuration

`vercel.json` sets:

```text
framework: nextjs
buildCommand: npm run build
installCommand: npm install
region: iad1
pages/api/**/*.ts maxDuration: 30
pages/api/cron/**/*.ts maxDuration: 60
crons: heartbeat, playtest-cycle, mycelium-relay, hotfix refresh, KAN rollup, sigma checkpoint
headers: CORS for /api/*, no-store for auth/admin/legal/account/doc pages
```

## Supabase Relationship

Auth uses Supabase anon client on the browser. Lazarus persistence uses service
role on the server. Database schema lives next door:

```text
..\cssl-supabase\migrations\0042_lazarus.sql
```

The browser must never receive `SUPABASE_SERVICE_ROLE_KEY`.

## Troubleshooting Playbooks

### Login redirects to the wrong place

Check:

```text
lib/auth.ts -> resolveAuthRedirect()
pages/login.tsx -> OAuth redirectTo
pages/auth/callback.tsx -> official callback
pages/index.tsx -> root callback fallback
Supabase dashboard OAuth redirect URLs
Vercel aliases for apocky.com and www.apocky.com
```

### Login succeeds but home still shows Sign in

Check:

```text
URL has ?code=... or hash tokens
lib/auth-callback.ts consumes and clears params
sb-access-token cookie exists
/api/auth/me returns user when called with authFetch
Akashic/Termly/other UI is not blocking clicks
```

### Buttons do nothing

Check browser snapshot for overlays. The known historical blockers were Termly
and Akashic consent. Auth routes must remain overlay-free.

### Lazarus page says not authorized

Check:

```text
Signed-in email is in APOCKY_ADMIN_EMAILS or default allowlist
authFetch is used from browser admin pages
/api/admin/check returns authorized:true
cookies exist after login
```

### Lazarus APIs are public

That is a regression. GET routes must require `requireAdmin()`. Runner write
routes must require `requireRunnerToken()`.

### Lazarus says stub:true

Check server-side Supabase envs:

```text
SUPABASE_URL
SUPABASE_SERVICE_ROLE_KEY
```

Then check that migration `0042_lazarus.sql` exists/applied.

### Runner cannot register

Check:

```text
LAZARUS_RUNNER_TOKEN exists locally and in Vercel
runner sends Authorization: Bearer <token>
LAZARUS_CONTROL_URL points at local dev or production intentionally
```

### Health is 200 but payments_ready is false

This can be expected. `payments_ready` requires Stripe secret, Stripe webhook
secret, and Supabase connectivity. Stripe was not configured during this handoff.

## Completion Standard

For website/auth/Lazarus tasks, completion means:

1. Code change is focused and secrets remain private.
2. Focused tests pass.
3. `npm run check` passes.
4. `npm run build` passes when deployment behavior could change.
5. Production or browser smoke is run if the user asked for live behavior.
6. Final report names the changed files and verification.
