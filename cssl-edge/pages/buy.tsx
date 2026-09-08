import type { NextPage } from 'next';
import Head from 'next/head';
import Script from 'next/script';
import React, { useEffect, useState } from 'react';

import { SUPPORT_LINKS } from '../lib/support-links';

export { SUPPORT_LINKS };

export const CHAOS_TAROT_BUY_BUTTON_ID = 'buy_btn_1UD2TD2M59SA2Ef7B02nUiO9';
export const CHAOS_TAROT_PRICE_LABEL = '$3.33 per month';
export const CHAOS_TAROT_REFUND_DAYS = 14;
export const STRIPE_BUY_BUTTON_SCRIPT = 'https://js.stripe.com/v3/buy-button.js';
export const STRIPE_BUY_BUTTON_DEADLINE_MS = 8_000;

declare global {
  namespace JSX {
    interface IntrinsicElements {
      'stripe-buy-button': React.DetailedHTMLProps<React.HTMLAttributes<HTMLElement>, HTMLElement> & {
        'buy-button-id': string;
        'publishable-key': string;
      };
    }
  }
}

type CheckoutState = 'loading' | 'ready' | 'unavailable';

function StripeCheckout(): JSX.Element {
  const [state, setState] = useState<CheckoutState>('loading');

  useEffect(() => {
    const registry = window.customElements;
    if (!registry) {
      setState('unavailable');
      return undefined;
    }
    if (registry.get('stripe-buy-button')) {
      setState('ready');
      return undefined;
    }

    let active = true;
    const deadline = window.setTimeout(() => {
      if (active) setState('unavailable');
    }, STRIPE_BUY_BUTTON_DEADLINE_MS);
    void registry.whenDefined('stripe-buy-button').then(() => {
      if (!active) return;
      window.clearTimeout(deadline);
      setState('ready');
    }, () => {
      if (!active) return;
      window.clearTimeout(deadline);
      setState('unavailable');
    });
    return () => {
      active = false;
      window.clearTimeout(deadline);
    };
  }, []);

  return <>
    <Script
      id="stripe-buy-button-script"
      src={STRIPE_BUY_BUTTON_SCRIPT}
      strategy="afterInteractive"
      onError={() => { setState('unavailable'); }}
    />
    <div className="checkout-frame" data-checkout-state={state} aria-busy={state === 'loading'}>
      {React.createElement('stripe-buy-button', {
        'buy-button-id': CHAOS_TAROT_BUY_BUTTON_ID,
        'publishable-key': 'pk_live_51PtJw92M59SA2Ef7bOvdRnArvKVJ9aNjUbodmvdd6lsyYIf1cWmPDfbutYaIIgY5HmVObYWB2bnXtIcSyfZhJaEq00govSs3sm',
      })}
      {state === 'loading' ? <p className="checkout-status" role="status">Loading secure checkout…</p> : null}
      {state === 'unavailable' ? <div className="checkout-error" role="alert">
        <p>Secure checkout could not load.</p>
        <button type="button" onClick={() => { window.location.reload(); }}>Reload secure checkout</button>
      </div> : null}
    </div>
  </>;
}

const Buy: NextPage = () => (
  <>
    <Head>
      <title>Support Apocky · Chaos Tarot</title>
      <meta
        name="description"
        content="Join Chaos Tarot for $3.33 per month through Stripe, or support Apocky through Ko-fi or Patreon."
      />
      <link rel="canonical" href="https://www.apocky.com/buy" />
      <style>{`
        .support-page {
          width: min(900px, calc(100% - 36px));
          margin: 0 auto;
          padding: clamp(36px, 5vw, 56px) 0 clamp(48px, 6vw, 72px);
        }
        .support-page h1 { margin: 0; font-size: var(--apx-fs-h1); line-height: 1.05; letter-spacing: -.035em; text-wrap: balance; }
        .support-lead { max-width: 640px; margin: 16px 0 0; color: var(--apx-copy); font-size: 1rem; line-height: 1.6; }
        .support-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 14px; margin-top: 44px; }
        .support-card {
          min-height: 250px;
          display: flex;
          flex-direction: column;
          border: 1px solid var(--apx-line);
          border-radius: 18px;
          background: var(--apx-panel);
          padding: 24px;
          color: inherit;
          text-decoration: none;
        }
        .support-card h2 { margin: 0; font-size: 1.25rem; }
        .support-card p { color: var(--apx-copy); line-height: 1.65; }
        .support-card span { margin-top: auto; color: var(--apx-mint); font-weight: 700; }
        .support-card--checkout { grid-column: span 2; }
        .support-price { margin: 6px 0 0; color: var(--apx-mint); font-size: 1.05rem; font-weight: 700; }
        .checkout-frame { margin-top: auto; min-height: 72px; padding-top: 20px; }
        .checkout-frame stripe-buy-button { display: block; min-height: 48px; }
        .checkout-status, .checkout-error p { margin: 10px 0 0; color: var(--apx-muted); font-size: .9rem; }
        .checkout-frame[data-checkout-state='ready'] .checkout-status { display: none; }
        .checkout-error button {
          margin-top: 12px;
          border: 1px solid var(--apx-line);
          border-radius: 999px;
          background: transparent;
          padding: 10px 16px;
          color: var(--apx-mint);
          font: inherit;
          font-weight: 700;
          cursor: pointer;
        }
        .support-note { margin-top: 38px; border-top: 1px solid var(--apx-line); padding-top: 24px; color: var(--apx-muted); line-height: 1.7; }
        @media (max-width: 760px) { .support-grid { grid-template-columns: 1fr; } .support-card, .support-card--checkout { min-height: 190px; grid-column: auto; } }
      `}</style>
    </Head>
    <main className="support-page">
      <p className="apx-eyebrow">Optional support</p>
      <h1>Support the work</h1>
      <p className="support-lead">
        Chaos Tarot membership is available through Stripe. Ko-fi and Patreon are also available if you prefer
        general support. Support is appreciated, never required, and does not buy control over creative decisions
        or anyone else.
      </p>

      <div className="support-grid">
        <section className="support-card support-card--checkout" aria-labelledby="chaos-tarot-membership">
          <h2 id="chaos-tarot-membership">Chaos Tarot membership</h2>
          <p className="support-price">{CHAOS_TAROT_PRICE_LABEL}</p>
          <p>Monthly Chaos Tarot access with Apocrypha-guided AI oracle interpretations. Renews monthly until canceled.</p>
          <StripeCheckout />
        </section>
        {SUPPORT_LINKS.map((link) => (
          <a
            key={link.name}
            className="support-card"
            href={link.href}
            target="_blank"
            rel="noopener noreferrer"
          >
            <h2>{link.name}</h2>
            <p>{link.description} The external service’s own terms and privacy policy apply.</p>
            <span>Open {link.name} in a new tab</span>
          </a>
        ))}
      </div>

      <p className="support-note">
        <strong>Refund and cancellation terms:</strong> {CHAOS_TAROT_REFUND_DAYS}-day no-questions-asked refund.
        Cancel subscriptions at any time through <a href="/account">your account</a>; access continues through the paid period. Renewal notices honor
        CA Bus. &amp; Prof. Code §17602(b). If Stripe is unreachable, email{' '}
        <a href="mailto:apocky13@gmail.com?subject=%5Brefund%5D">apocky13@gmail.com</a>.
      </p>
    </main>
  </>
);

export default Buy;
