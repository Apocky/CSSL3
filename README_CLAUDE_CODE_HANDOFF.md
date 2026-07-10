# Claude Code Handoff README: CSSLv3 Workspace

This file is the practical handoff entry point for Claude Code or another agent.
It explains where the active browser/site work lives, which directories are
specification-only, and which commands are safe to run first.

## Start Here

Read these files in order when arriving cold:

1. `PRIME_DIRECTIVE.md` - immutable project constraint and sovereignty rules.
2. `README.md` - broad CSSLv3 project identity and architecture.
3. `cssl-edge/README_CLAUDE_CODE_HANDOFF.md` - active website, auth, Lazarus,
   browser setup, deployment, and function map.
4. `cssl-supabase/README_CLAUDE_CODE_HANDOFF.md` - database schema, migrations,
   verification, and Lazarus persistence.

## Current Production State

The active public website is the Vercel project `apocky-com`. The obsolete
Vercel project named `frontend` was removed after it was found to still own
`apocky.com`.

Current production deployment at handoff time:

```text
https://apocky-80rpmyeq1-shawn-bakers-projects-cb1c9715.vercel.app
```

Current aliases point at that deployment:

```text
https://apocky.com
https://www.apocky.com
https://apocky-com.vercel.app
```

Verified production status at handoff time:

```text
GET /                         -> 200
GET /?code=shape-only         -> 200
GET /login                    -> 200
GET /auth/callback            -> 200
GET /api/health               -> 200
GET /api/admin/lazarus/health -> 401 when unauthenticated
GET /api/admin/lazarus/tasks  -> 401 when unauthenticated
GET /api/admin/lazarus/runners -> 401 when unauthenticated
```

Browser verification also passed: the home page Sign in link navigated to
`/login`, and the login provider buttons were visible without a consent prompt
covering them.

## Repository Map

```text
CSSLv3/
  README.md                         broad CSSLv3 project README
  README_CLAUDE_CODE_HANDOFF.md     this handoff index
  PRIME_DIRECTIVE.md                immutable sovereignty/consent constraints
  DECISIONS.md                      append-only architecture decisions
  compiler-cssl/                    CSSL compiler work
  compiler-rs/                      Rust compiler/back-end work
  engine/                           runtime/engine implementation work
  examples/                         language/runtime examples
  specs/                            long-form implementation and system specs
  stdlib/                           CSSL standard library surface
  tools/                            support tools
  scripts/                          workspace scripts
  cssl-edge/                        active Next.js website and API layer
  cssl-supabase/                    Supabase schema, migrations, verification
```

## Active Browser Project

Use `cssl-edge/` for all browser-visible website work. That is the Next.js app
deployed to Vercel as `apocky-com`.

Useful commands:

```powershell
cd C:\Users\Apocky\source\repos\CSSLv3\cssl-edge
npm install
npm run dev
```

Then open:

```text
http://localhost:3000
```

Validation commands:

```powershell
npm run test:auth-redirect
npm run test:health
npm run test:lazarus
npm run check
npm run build
```

Deploy command when ready:

```powershell
vercel --prod
```

Do not deploy just because documentation changed unless the user explicitly
wants the docs published through the site.

## Active Database Project

Use `cssl-supabase/` for schema, RLS, seed data, and verification SQL.

The Lazarus control-plane schema is in:

```text
cssl-supabase/migrations/0042_lazarus.sql
```

Production Lazarus currently runs in Supabase-backed mode when the server has
`SUPABASE_URL` and `SUPABASE_SERVICE_ROLE_KEY`. If either is missing, the store
falls back to in-memory stub mode.

## Important Environment Variables

Do not print or commit actual values. `.env.local` exists in `cssl-edge/` and
must be treated as secret-bearing local state.

Website/auth variables:

```text
APOCKY_HUB_SUPABASE_URL
APOCKY_HUB_SUPABASE_ANON_KEY
NEXT_PUBLIC_SUPABASE_URL
NEXT_PUBLIC_SUPABASE_ANON_KEY
APOCKY_ADMIN_EMAILS
```

Database/server variables:

```text
SUPABASE_URL
SUPABASE_ANON_KEY
SUPABASE_SERVICE_ROLE_KEY
```

Lazarus runner variables:

```text
LAZARUS_RUNNER_TOKEN
LAZARUS_CONTROL_URL
LAZARUS_RUNNER_ID
LAZARUS_RUNNER_LABEL
LAZARUS_ENABLE_MODEL_CALLS
DEEPSEEK_API_KEY
```

Payment/cron variables:

```text
STRIPE_SECRET_KEY
STRIPE_WEBHOOK_SIGNING_SECRET
CRON_SECRET
```

At handoff time `LAZARUS_RUNNER_TOKEN` is configured in Vercel production,
preview, development, and local `.env.local`. Never reveal the token value.

## What Was Recently Fixed

1. Obsolete domain ownership:
   `apocky.com` was still attached to an obsolete Vercel project named
   `frontend`. That project was deleted, and aliases were restored to
   `apocky-com`.

2. Website outage:
   `apocky-com.vercel.app` worked while custom domains were broken. Vercel
   aliases were restored and smoke-tested.

3. Auth callback handling:
   Supabase PKCE sometimes returned to `/` with `?code=...`. The root page now
   consumes callback params through `cssl-edge/lib/auth-callback.ts`.

4. Login looked broken after hard refresh:
   The user saw Sign in after login because the root page did not consume the
   PKCE code. Fixed and deployed.

5. Buttons appeared non-interactive:
   First-visit consent UI was blocking clicks. The Termly resource blocker was
   removed, and `AkashicConsent` is now a non-blocking banner hidden on auth
   pages.

6. Lazarus security:
   Admin API reads now require admin auth. Runner writes require
   `LAZARUS_RUNNER_TOKEN`. Missing runner token fails closed.

## Security Rules For Agents

Use these rules while working in this workspace:

1. Never print secrets from `.env.local`, Vercel, or Supabase.
2. Never commit `.env.local`, `.next/`, `node_modules/`, or Vercel local state.
3. Do not start a long-running production Lazarus runner unless the user asks.
4. Do not enable `LAZARUS_ENABLE_MODEL_CALLS=1` unless the user asks and budget
   routing is clear.
5. Public Lazarus API reads returning 401 while unauthenticated is correct.
6. Keep auth pages free from modal overlays, banners, or global consent prompts.
7. If login seems broken, inspect the URL for `?code=...` and inspect cookies
   before changing Supabase settings.

## Cross-Repo Relationship

```text
CSLv3      -> dense notation system, separate repo, canonical notation
CSSLv3     -> compiled language, runtime, website/API layer, Supabase schema
cssl-edge  -> Next.js/Vercel public site and API layer inside CSSLv3
cssl-supabase -> Supabase schema inside CSSLv3
```

CSSLv3 is not CSLv3. The repo name is close enough to cause mistakes, so keep
the distinction explicit in handoffs. Omniverse is a separate Omega-substrate
specification repository and is not part of the Apocky.com/Lazarus handoff.

## Quick Recovery Checklist

If the website is down or auth looks broken:

1. Check `https://apocky-com.vercel.app` and `https://www.apocky.com`.
2. Confirm Vercel aliases point to the same `apocky-com` production deployment.
3. Check `GET /api/health` for Supabase/auth booleans.
4. Check `/login` and `/auth/callback` return 200.
5. Check the browser snapshot for consent UI covering links or buttons.
6. Check if the browser URL has `?code=...`; root should consume it.
7. Run `npm run test:auth-redirect`, `npm run check`, and `npm run build`.

## Handoff Pointers

Use the more detailed child handoff files for actual implementation work:

```text
cssl-edge/README_CLAUDE_CODE_HANDOFF.md
cssl-supabase/README_CLAUDE_CODE_HANDOFF.md
```

