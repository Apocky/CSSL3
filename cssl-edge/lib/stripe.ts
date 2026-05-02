// cssl-edge · lib/stripe.ts
// Server-only Stripe SDK wrapper. Mirrors getSupabase() stub-pattern :
// returns null when STRIPE_SECRET_KEY missing — routes degrade to stub-mode
// rather than 500-ing. Honors task-spec : Stripe-secret NEVER-in-git, only
// process.env from Vercel / local .env.local.
//
// All product-ids resolve through PRODUCT_CATALOG. Cosmetic-channel-only
// monetization-axiom (per spec/grand-vision/13_INFINITE_LABYRINTH_LEGACY.csl)
// — no pay-for-power · no DRM · no anti-cheat.
//
// § Q-07 RESOLVED 2026-05-01 (Apocky-canonical) :
//   verbatim : "Hold on cosmetics, main game first."
//   COSMETIC_LAUNCH_PAUSED = true ; cosmetic-products visible:false
//   alpha-free product remains visible (main-game pathway)
//   Stripe-infrastructure preserved · /buy banner notifies users
//   resume-trigger : Apocky greenlights "main-game-shipped"

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
  // 'continuation' = paid-resurrection-tokens for Tier-2 Hardcore-Permadeath
  // (per Q-02 + Apocky's "monetize continuations" caveat · 2026-05-01)
  // continuation is NOT pay-for-power : it grants restoration of state-at-death,
  // never new stats/gear/XP. Rate-limited ≤5 per character-lifetime.
  tier: 'alpha-free' | 'cosmetic' | 'subscription' | 'continuation';
  // shows on /buy regardless of stub-mode
  visible: boolean;
}

// § Q-07 PAUSE-FLAG · cosmetic-launch held until-main-game-ships
// Apocky 2026-05-01 : "Hold on cosmetics, main game first."
export const COSMETIC_LAUNCH_PAUSED = true;

export const PRODUCT_CATALOG: ReadonlyArray<ProductDescriptor> = [
  {
    id: 'loa-alpha',
    display_name: 'Labyrinth of Apocalypse · Alpha',
    blurb: 'First public alpha · self-hosted · DRM-free · feedback welcome. Free during alpha; sliding-scale tier on v1.0 release.',
    price_cents: 0,
    currency: 'usd',
    stripe_price_env: 'STRIPE_PRICE_LOA_ALPHA',
    tier: 'alpha-free',
    visible: true, // alpha stays visible (main-game pathway · Q-07-aligned)
  },
  {
    id: 'loa-cosmetic-mycelial-bloom',
    display_name: 'Mycelial Bloom · cosmetic shader-pack',
    blurb: 'Substrate-painted Home pocket-dimension shader. Cosmetic-only · zero gameplay impact.',
    price_cents: 500,
    currency: 'usd',
    stripe_price_env: 'STRIPE_PRICE_COSMETIC_MYCELIAL',
    tier: 'cosmetic',
    visible: false, // § Q-07 paused · re-enable post main-game-ship
  },
  {
    id: 'mycelium-plus',
    display_name: 'Mycelium · Plus',
    blurb: 'Faster mycelial sync · larger Home-dimension storage. Cosmetic + convenience · no power.',
    price_cents: 700,
    currency: 'usd',
    stripe_price_env: 'STRIPE_PRICE_MYCELIUM_PLUS',
    tier: 'subscription',
    visible: false, // § Q-07 paused · re-enable post main-game-ship
  },

  // § Q-02 Continuation-Tokens (Apocky-canonical 2026-05-01 · "monetize
  //   continuations · pay to resurrect instantaneously and wholly").
  // Cosmetic-only-axiom intact : these grant RESTORATION of state-at-death,
  // never new stats/gear/XP. Rate-limited ≤5 per character-lifetime.
  // visible:false until Hardcore-Permadeath tier ships with the main game.
  {
    id: 'loa-continuum-token-single',
    display_name: 'Continuum-Token · Single',
    blurb: 'Petition the substrate to undo a phase-shift. One Hardcore-Permadeath resurrection · instantaneous and whole. Rate-limited ≤5 per character-lifetime.',
    price_cents: 499, // $4.99
    currency: 'usd',
    stripe_price_env: 'STRIPE_PRICE_CONTINUUM_TOKEN_SINGLE',
    tier: 'continuation',
    visible: false, // ships with Hardcore-tier launch
  },
  {
    id: 'loa-continuum-token-3pack',
    display_name: 'Continuum-Token · 3-pack',
    blurb: 'Three petition-tokens · ~20% bundle savings. Tokens are gift-economy-transferable to friends. No expiry · no FOMO.',
    price_cents: 1199, // $11.99 · ~20% bundle savings vs 3 × 499
    currency: 'usd',
    stripe_price_env: 'STRIPE_PRICE_CONTINUUM_TOKEN_3PACK',
    tier: 'continuation',
    visible: false, // ships with Hardcore-tier launch
  },
  {
    id: 'loa-continuum-token-10pack',
    display_name: 'Continuum-Token · 10-pack',
    blurb: 'Ten petition-tokens · ~30% bundle savings. Tokens transferable peer-to-peer. The substrate keeps every petition Σ-Chain-anchored · biography-transparent.',
    price_cents: 3499, // $34.99 · ~30% bundle savings vs 10 × 499
    currency: 'usd',
    stripe_price_env: 'STRIPE_PRICE_CONTINUUM_TOKEN_10PACK',
    tier: 'continuation',
    visible: false, // ships with Hardcore-tier launch
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
