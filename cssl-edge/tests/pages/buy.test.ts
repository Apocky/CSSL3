import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import Buy, {
  CHAOS_TAROT_BUY_BUTTON_ID,
  CHAOS_TAROT_PRICE_LABEL,
  CHAOS_TAROT_REFUND_DAYS,
  STRIPE_BUY_BUTTON_DEADLINE_MS,
  STRIPE_BUY_BUTTON_SCRIPT,
  SUPPORT_LINKS,
} from '@/pages/buy';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

export function testBuyDefaultExport(): void {
  assert(typeof Buy === 'function', 'buy default export must be a component');
}

export function testSupportLinks(): void {
  assert(SUPPORT_LINKS.length === 2, 'Ko-fi and Patreon are the two support destinations');
  const names = new Set(SUPPORT_LINKS.map((link) => link.name));
  assert(names.has('Ko-fi'), 'Ko-fi link is present');
  assert(names.has('Patreon'), 'Patreon link is present');
  assert(SUPPORT_LINKS.find((link) => link.name === 'Ko-fi')?.href === 'https://ko-fi.com/oneinfinity', 'Ko-fi destination remains exact');
  assert(SUPPORT_LINKS.find((link) => link.name === 'Patreon')?.href === 'https://www.patreon.com/0ne1nfinity', 'Patreon destination remains exact');
  for (const link of SUPPORT_LINKS) {
    assert(link.href.startsWith('https://'), `${link.name} uses HTTPS`);
    assert(link.description.length > 0, `${link.name} has a plain-language description`);
    assert(link.label.startsWith('Support on '), `${link.name} has a clear action label`);
  }
}

export function testLiveChaosCheckoutContract(): void {
  assert(
    CHAOS_TAROT_BUY_BUTTON_ID === 'buy_btn_1UD2TD2M59SA2Ef7B02nUiO9',
    'live Chaos Tarot Stripe buy-button ID remains exact',
  );
  assert(CHAOS_TAROT_PRICE_LABEL === '$3.33 per month', 'monthly price remains $3.33');
  assert(CHAOS_TAROT_REFUND_DAYS === 14, 'refund period remains 14 days');
  assert(STRIPE_BUY_BUTTON_SCRIPT === 'https://js.stripe.com/v3/buy-button.js', 'official Stripe buy-button script remains exact');
  assert(STRIPE_BUY_BUTTON_DEADLINE_MS > 0, 'checkout load has a finite visible outcome deadline');
}

export function testCheckoutMarkup(): void {
  const markup = renderToStaticMarkup(React.createElement(Buy));
  assert(markup.includes('<stripe-buy-button'), 'Stripe web component is present in rendered markup');
  assert(markup.includes(`buy-button-id="${CHAOS_TAROT_BUY_BUTTON_ID}"`), 'rendered checkout uses the supplied live buy-button ID');
  assert(markup.includes('pk_live_51PtJw92M59SA2Ef7'), 'rendered checkout uses the supplied live publishable key');
  assert(markup.includes('Loading secure checkout'), 'checkout has visible loading feedback instead of a zero-height blank');
}

export function testCheckoutContentSecurityPolicy(): void {
  const config = JSON.parse(readFileSync(resolve(process.cwd(), 'vercel.json'), 'utf8')) as {
    headers?: Array<{ source?: string; headers?: Array<{ key?: string; value?: string }> }>;
  };
  const rules = config.headers ?? [];
  const globalRule = rules.find(rule => rule.source === '/(.*)');
  const csp = globalRule?.headers?.find(header => header.key === 'Content-Security-Policy')?.value ?? '';
  assert(Boolean(csp), 'production CSP is configured');
  assert(/script-src[^;]*https:\/\/js\.stripe\.com/.test(csp), 'buy CSP permits the official Stripe script origin');
  assert(/frame-src[^;]*https:\/\/js\.stripe\.com/.test(csp), 'buy CSP permits the official Stripe frame origin');
  assert(!/frame-src[^;]*'none'/.test(csp), 'production CSP does not retain the frame ban that hid checkout');
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
    testSupportLinks();
    testLiveChaosCheckoutContract();
    testCheckoutMarkup();
    testCheckoutContentSecurityPolicy();
    // eslint-disable-next-line no-console
    console.log('buy.test : OK · 5 tests passed');
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  }
}
