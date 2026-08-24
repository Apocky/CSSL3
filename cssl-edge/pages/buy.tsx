import type { NextPage } from 'next';
import Head from 'next/head';

import { SUPPORT_LINKS } from '../lib/support-links';

export { SUPPORT_LINKS };

const Buy: NextPage = () => (
  <>
    <Head>
      <title>Download or support · Apocky</title>
      <meta
        name="description"
        content="Download the free Labyrinth of Apocalypse test build or choose an external way to support Apocky’s work."
      />
      <link rel="canonical" href="https://www.apocky.com/buy" />
      <style>{`
        .support-page {
          width: min(900px, calc(100% - 36px));
          margin: 0 auto;
          padding: clamp(64px, 9vw, 110px) 0 110px;
        }
        .support-page h1 { margin: 0; font-size: clamp(2.8rem, 8vw, 6rem); line-height: .95; letter-spacing: -.06em; }
        .support-lead { max-width: 690px; margin: 24px 0 0; color: var(--apx-copy); font-size: 1.08rem; line-height: 1.75; }
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
        .support-note { margin-top: 38px; border-top: 1px solid var(--apx-line); padding-top: 24px; color: var(--apx-muted); line-height: 1.7; }
        @media (max-width: 760px) { .support-grid { grid-template-columns: 1fr; } .support-card { min-height: 190px; } }
      `}</style>
    </Head>
    <main className="support-page">
      <p className="apx-eyebrow">Optional support</p>
      <h1>Download or support</h1>
      <p className="support-lead">
        Labyrinth of Apocalypse is currently a free, unfinished test build. If you want to support Apocky’s
        work, Ko-fi and Patreon are available. Support is appreciated, never required, and does not buy control
        over creative decisions or anyone else.
      </p>

      <div className="support-grid">
        <a className="support-card" href="/download">
          <h2>Download LoA</h2>
          <p>Read what works, what is unfinished, and how to check the file before opening it.</p>
          <span>View the free test build</span>
        </a>
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
        There is no direct checkout on this page. If a direct purchase is introduced later, the item, full
        price, renewal terms, and refund terms must be shown before payment.
      </p>
    </main>
  </>
);

export default Buy;
