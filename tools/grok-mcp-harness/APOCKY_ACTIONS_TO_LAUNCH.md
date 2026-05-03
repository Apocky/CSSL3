# Apocky-Actions to Launch Tier-1 + Tier-2 Real Checkout

After the deploy lands `/products/harness` and `/products/early-access` pages, the checkout buttons go through `/api/payments/stripe/checkout` (existing W9 infra). Currently buttons return `stub-mode` because the Stripe price-IDs aren't set. Here's what you need to do — once — to flip from stub to live.

## 1 · Create Stripe products + prices (Stripe Dashboard, ~10 min)

Go to https://dashboard.stripe.com/products → "Add product" 7 times.

For each, set the recurring/one-time correctly. After creation, copy the `price_xxxxxxxxxxxxx` ID.

| Product | Type | Price | Env-var name |
|---|---|---|---|
| Sovereign MCP Harness · Starter | Recurring (monthly) | $49 | `STRIPE_PRICE_HARNESS_STARTER` |
| Sovereign MCP Harness · Pro | Recurring (monthly) | $99 | `STRIPE_PRICE_HARNESS_PRO` |
| Sovereign MCP Harness · Studio | Recurring (monthly) | $199 | `STRIPE_PRICE_HARNESS_STUDIO` |
| Sovereign MCP Harness · Lifetime | One-time | $999 | `STRIPE_PRICE_HARNESS_LIFETIME` |
| apocky.com · Early-Access | Recurring (monthly) | $19 | `STRIPE_PRICE_EARLY_ACCESS` |
| apocky.com · Studio | Recurring (monthly) | $99 | `STRIPE_PRICE_APOCKY_STUDIO` |
| apocky.com · Lifetime | One-time | $999 | `STRIPE_PRICE_APOCKY_LIFETIME` |

## 2 · Set env-vars in Vercel (~5 min)

Project → `apocky-com` → Settings → Environment Variables → Add 7 entries (Production environment).

Each entry: `name = STRIPE_PRICE_HARNESS_STARTER`, `value = price_xxxxxxxxxxxxx` from step 1.

You should already have `STRIPE_SECRET_KEY` and `STRIPE_WEBHOOK_SIGNING_SECRET` set from the W9-polish landing.

## 3 · Redeploy (~1 min)

```powershell
cd C:\Users\Apocky\source\repos\CSSLv3\cssl-edge
vercel --prod --force
```

After deploy, verify:

```bash
curl -s -X POST https://www.apocky.com/api/payments/stripe/checkout \
  -H "content-type: application/json" \
  -d '{"product_id":"harness-starter","success_url":"https://apocky.com/store","cancel_url":"https://apocky.com/store","cap":3}'
```

Expect `{"checkout_url":"https://checkout.stripe.com/c/pay/cs_..."}` — that's a working live checkout session.

## 4 · Set up the entitlements webhook (~15 min · CAN BE DEFERRED)

The existing webhook `/api/payments/stripe/webhook` (W9-polish) handles `checkout.session.completed` and writes to Supabase entitlements. You should:

1. In Stripe Dashboard → Developers → Webhooks → Add endpoint
2. URL: `https://www.apocky.com/api/payments/stripe/webhook`
3. Events: `checkout.session.completed`, `customer.subscription.created`, `customer.subscription.deleted`, `invoice.paid`
4. Copy the signing secret → set as `STRIPE_WEBHOOK_SIGNING_SECRET` in Vercel env (overwrite if needed)

Without this step, Stripe still takes payment fine — you just won't get automatic Discord-invite or alpha-key delivery. You can hand-deliver to first 5-10 customers manually while the webhook is wired.

## 5 · Decide on delivery (1 day)

Right now both products are sold but delivery is manual. Options:

- **Email-delivery (simplest)** : webhook sends email with bearer-token + harness download link
- **Supabase entitlement** : webhook writes to Supabase, customer logs into apocky.com/account, sees their entitlements, downloads from there
- **GitHub-private-repo invite** : webhook sends GitHub invite to private repo containing harness code

For revenue-now, recommend: ship Email-delivery. It's the lowest friction.

## 6 · Marketing minimum (4 hours, day-of-launch)

- Tweet on X: "Just shipped /store · 4 tiers · sovereignty-respecting AI tooling · link"
- Hacker News Show-HN: "Show HN: Sovereign MCP Harness — run your own AI workspace"
- Email warm contacts (~10 solo devs) directly: "Hey, here's what I've been building, $49/mo if you want in early"
- Discord post in any indie-dev / GameDev / AI-tools channels you're already in

## Total time to live · ~30-45 minutes

If you skip the webhook (step 4) and hand-deliver, you can be revenue-positive in under an hour from when the deploy lands.

---

## What's already shipped (you don't have to do)

- ✓ `/store` page · 4-tier overview
- ✓ `/products/harness` page · 4 tier cards · Stripe-checkout buttons
- ✓ `/products/early-access` page · 3 tier cards · Stripe-checkout buttons
- ✓ `/api/payments/stripe/checkout` endpoint (W9-polish)
- ✓ Stub-mode-aware UI (won't crash if env-vars missing)
- ✓ Bearer-token error handling on checkout
- ✓ Cap-witness gating (`STRIPE_CHECKOUT_INIT`)
- ✓ Webhook endpoint exists at `/api/payments/stripe/webhook`
- ✓ Supabase entitlement table from spec/22 · ready
- ✓ All product entries in `lib/stripe.ts` PRODUCT_CATALOG · 7 new products

## Stub-mode behavior

If `STRIPE_PRICE_HARNESS_*` env-vars aren't set, the checkout endpoint returns `{"stub": true, "message": "..."}` and the UI shows a yellow banner "Stripe not yet configured · email pre-order". This means: customers can still see the pricing pages and contact you, but actual checkout doesn't fire until you complete steps 1-3. Safe to deploy in any state.
