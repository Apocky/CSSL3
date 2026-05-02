# Akashic-Webpage-Records

Substrate-native diagnostic layer for apocky.com. Every page event becomes a cell in the ω-field, Σ-mask-gated for sovereignty, KAN-pattern-clusterable for self-healing, mycelium-federated for cross-session learning.

**Errors aren't bugs · they're spores in the substrate that the system learns from.**

This is not Sentry / Datadog / LogRocket. There is no third-party SaaS. There are no advertising IDs. There are no telemetry cookies. The substrate is the diagnostic.

---

## Substrate parallels

| substrate primitive | telemetry mapping |
| --- | --- |
| ω-field cell | `AkashicEvent` |
| Σ-mask | `sigma_mask` bitmask + per-kind gate-table |
| KAN pattern | server-side cluster signature (`cluster_signature`) |
| Mycelium | optional cross-session federation (`SIGMA_FEDERATED`) |
| Akashic | the in-DB record itself (`public.akashic_events`) |
| Sovereign cap | `cap_witness` proof + `purgeAllMine()` |

---

## Quick start

```tsx
// _app.tsx
import { useEffect } from 'react';
import { akashicInstall, AkashicErrorBoundary } from '@/lib/akashic-telemetry';
import AkashicConsent from '@/components/AkashicConsent';

function App({ Component, pageProps }) {
  useEffect(() => {
    akashicInstall({
      dpl_id: process.env['NEXT_PUBLIC_VERCEL_DEPLOYMENT_ID'] ?? 'local',
      commit_sha: process.env['NEXT_PUBLIC_VERCEL_GIT_COMMIT_SHA'] ?? 'unknown',
      build_time: process.env['NEXT_PUBLIC_BUILD_TIME'] ?? new Date().toISOString(),
    });
  }, []);
  return (
    <AkashicErrorBoundary>
      <AkashicConsent />
      <Component {...pageProps} />
    </AkashicErrorBoundary>
  );
}
```

---

## Consent tiers

The user picks one. Default is **Spore** (aggregate-only, k-anon ≥ 10). Sovereign-revocable at any time.

| tier | mask | k-anon | captures |
| --- | --- | --- | --- |
| `none` | 0 | infinity | nothing leaves the browser |
| `spore` | aggregate | 10 | page-views, perf, error-counts |
| `mycelium` | aggregate + pattern | 5 | + stack traces, cluster signatures |
| `akashic` | aggregate + pattern + federated | 5 | + console.error/.warn, user-flow |

---

## Event kinds (the discriminator)

`page.view` · `page.error` · `page.unload` · `react.error` · `promise.unhandled` · `console.error` · `console.warn` · `perf.lcp` · `perf.fid` · `perf.cls` · `perf.inp` · `perf.ttfb` · `perf.fcp` · `perf.long_task` · `perf.resource_slow` · `perf.resource_fail` · `net.fail` · `net.slow` · `consent.granted` · `consent.revoked` · `consent.purge_request` · `user.flow` · `deploy.detected`

---

## Defense-in-depth error capture (six layers)

1. **`_document.tsx` inline early-error script** · catches errors BEFORE React hydrates (white-screen-of-death detection)
2. **`_app.tsx` top-level `AkashicErrorBoundary`** · catches React render-tree errors
3. **Per-page / per-section `AkashicErrorBoundary`** · finer-grained boundaries opt-in
4. **`window.onerror`** + **`window.onunhandledrejection`** · fallback for non-React errors
5. **Network tap** · intercepts `fetch()` + XHR · captures status >= 400, slow > 3s, network errors
6. **Console tap** · `console.error` / `.warn` (consent-gated to Akashic tier)

---

## Sovereign purge

Every user can purge all their events at any time:

```ts
import { purgeAllMine } from '@/lib/akashic-telemetry';
await purgeAllMine(myCapWitness);
```

Hits `DELETE /api/akashic/purge` with `x-akashic-cap-witness` header. Server validates the cap and `DELETE`s every row in `akashic_events` whose `user_cap_hash` matches.

The /admin/telemetry page surfaces a "purge all my events" button for the admin themselves.

---

## Deploy-version drift detection

The client polls `/api/akashic/version` every 60s. If the server-reported `dpl_id` differs from the `dpl_id` baked into the running bundle, a `deploy.detected` cell is stamped — and the admin/telemetry dashboard surfaces "this version is stale, refresh recommended".

This is the canary for stuck-deploys (the very issue Apocky just hit on Vercel).

---

## Sigma-mask gating

```
SIGMA_NONE        = 0b0000  (never flushed)
SIGMA_SELF        = 0b0001  (client-only)
SIGMA_AGGREGATE   = 0b0010  (counts only · k-anon enforced)
SIGMA_PATTERN     = 0b0100  (cluster signatures · k-anon enforced)
SIGMA_FEDERATED   = 0b1000  (cross-session mycelium · explicit opt-in)
```

Per-event `sigma_mask` is set by `applyGate()` at capture time; server re-checks before persistence.

---

## What the user sees vs. what we collect

The `AkashicConsent` overlay explains in plain language. No dark-pattern banner. No "I agree to everything by visiting this site". Sovereignty is the default; consent is informed; revocation is one click.

---

## Future iterations

1. **KAN pattern clustering** · server-side cluster events by `cluster_signature` ; surface "this 4-frame React error happened in 17 sessions on dpl_X"
2. **Mycelium federation** · cross-session bias-learning · "this perf issue affects mobile-Safari users"
3. **Admin dashboard** · `/admin/telemetry` surfaces clusters · k-anon gates the detail view · sovereign-purge controls

---

## Mythological frame

The Akashic Records, in mystical tradition, are a non-physical record of every event in the universe. Here they are: a substrate-native record of every event on apocky.com, opt-in, k-anonymous, sovereignly purgeable, never weaponized.
