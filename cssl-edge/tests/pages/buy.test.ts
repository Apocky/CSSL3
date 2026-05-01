// cssl-edge · tests/pages/buy.test.ts
// Smoke: /buy page module + product-catalog shape.

import Buy from '@/pages/buy';
import { PRODUCT_CATALOG } from '@/lib/stripe';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

export function testBuyDefaultExport(): void {
  assert(typeof Buy === 'function', 'buy default export must be a component');
}

export function testProductCatalogShape(): void {
  assert(PRODUCT_CATALOG.length >= 4, `expected ≥4 products, got ${PRODUCT_CATALOG.length}`);
  const ids = new Set<string>();
  for (const p of PRODUCT_CATALOG) {
    assert(!ids.has(p.id), `duplicate product_id : ${p.id}`);
    ids.add(p.id);
    assert(['alpha-free', 'cosmetic', 'subscription'].includes(p.tier), `tier value : ${p.tier}`);
    assert(p.price_cents >= 0, `price non-negative : ${p.id}`);
    assert(p.currency === 'usd', `currency usd : ${p.id}`);
    assert(typeof p.stripe_price_env === 'string' && p.stripe_price_env.startsWith('STRIPE_PRICE_'), 'env-var prefix');
  }
}

export function testCosmeticChannelOnly(): void {
  // Honor cosmetic-channel-only-axiom : NO product can be tagged 'pay-for-power'.
  for (const p of PRODUCT_CATALOG) {
    assert(p.tier !== ('pay-for-power' as unknown), `forbidden tier on ${p.id}`);
  }
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  try {
    testBuyDefaultExport();
    testProductCatalogShape();
    testCosmeticChannelOnly();
    // eslint-disable-next-line no-console
    console.log('buy.test : OK · 3 tests passed');
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  }
}
