// cssl-edge · lib/stripe.ts
// Server-only Stripe SDK wrapper. Mirrors getSupabase() stub-pattern :
// returns null when STRIPE_SECRET_KEY missing — routes degrade to stub-mode
// rather than 500-ing. Honors task-spec : Stripe-secret NEVER-in-git, only
// process.env from Vercel / local .env.local.
//
// All product-ids resolve through PRODUCT_CATALOG. Cosmetic-channel-only
// monetization-axiom (per spec/grand-vision/13_INFINITE_LABYRINTH_LEGACY.csl)
// — no pay-for-power · no DRM · no anti-cheat.

import Stripe from 'stripe';

let _client: Stripe | null | undefined;

export function getStripe(): Stripe | null {
  if (_client !== undefined) return _client;
  const key = process.env['STRIPE_SECRET_KEY'];
  if (!key || key.length < 8) {
    _client = null;
    return null;
  }
  _client = new Stripe(key, {
    // Pin to a stable API version so mid-season price-creates do not regress.
    apiVersion: '2024-12-18.acacia' as Stripe.LatestApiVersion,
    typescript: true,
    appInfo: {
      name: 'cssl-edge',
      version: '0.1.0',
      url: 'https://apocky.com',
    },
  });
  return _client;
}

// Reset for tests (mirrors _resetSupabaseForTests).
export function _resetStripeForTests(): void {
  _client = undefined;
}

// ─── Product catalog ──────────────────────────────────────────────────────
// Source-of-truth for available products. Stripe-Price-IDs come from env so
// product-ids in code are stable across rotations of the live Stripe acct.

export interface ProductDescriptor {
  id: string;
  display_name: string;
  blurb: string;
  price_cents: number;
  currency: 'usd';
  // env-var name carrying the Stripe price-id (price_xxxx)
  stripe_price_env: string;
  // monetization tier — cosmetic-only-axiom enforced at code-review
  tier: 'alpha-free' | 'cosmetic' | 'subscription';
  // shows on /buy regardless of stub-mode
  visible: boolean;
}

export const PRODUCT_CATALOG: ReadonlyArray<ProductDescriptor> = [
  {
    id: 'loa-alpha',
    display_name: 'Labyrinth of Apocalypse · Alpha',
    blurb: 'First public alpha · self-hosted · DRM-free · feedback welcome. Free during alpha; sliding-scale tier on v1.0 release.',
    price_cents: 0,
    currency: 'usd',
    stripe_price_env: 'STRIPE_PRICE_LOA_ALPHA',
    tier: 'alpha-free',
    visible: true,
  },
  {
    id: 'loa-cosmetic-mycelial-bloom',
    display_name: 'Mycelial Bloom · cosmetic shader-pack',
    blurb: 'Substrate-painted Home pocket-dimension shader. Cosmetic-only · zero gameplay impact.',
    price_cents: 500,
    currency: 'usd',
    stripe_price_env: 'STRIPE_PRICE_COSMETIC_MYCELIAL',
    tier: 'cosmetic',
    visible: true,
  },
  {
    id: 'dgi-pro',
    display_name: 'ApockyDGI · Pro Tier',
    blurb: 'Higher per-month query budget · priority queue · DGI-Pro entitlement. Cancel anytime.',
    price_cents: 2500,
    currency: 'usd',
    stripe_price_env: 'STRIPE_PRICE_DGI_PRO',
    tier: 'subscription',
    visible: true,
  },
  {
    id: 'mycelium-plus',
    display_name: 'Mycelium · Plus',
    blurb: 'Faster mycelial sync · larger Home-dimension storage. Cosmetic + convenience · no power.',
    price_cents: 700,
    currency: 'usd',
    stripe_price_env: 'STRIPE_PRICE_MYCELIUM_PLUS',
    tier: 'subscription',
    visible: true,
  },
];

export function findProduct(id: string): ProductDescriptor | null {
  return PRODUCT_CATALOG.find((p) => p.id === id) ?? null;
}

// Resolve the live Stripe price-id from env. Returns null when env missing —
// caller MUST surface stub-mode to the client (no silent live-Stripe-call).
export function resolvePriceId(p: ProductDescriptor): string | null {
  const v = process.env[p.stripe_price_env];
  return v && v.length > 0 ? v : null;
}

// Webhook signing secret. Distinct from STRIPE_SECRET_KEY so a leak of one
// does not compromise the other.
export function getWebhookSigningSecret(): string | null {
  const v = process.env['STRIPE_WEBHOOK_SIGNING_SECRET'];
  return v && v.length > 0 ? v : null;
}
