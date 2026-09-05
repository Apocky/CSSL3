import Head from 'next/head';
import type { ReactNode } from 'react';

interface LegalDocumentProps {
  title: string;
  description: string;
  updated: string;
  children: ReactNode;
}

export default function LegalDocument({
  title,
  description,
  updated,
  children,
}: LegalDocumentProps): JSX.Element {
  return (
    <>
      <Head>
        <title>{title} · Apocky</title>
        <meta name="description" content={description} />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <style>{`
          .apx-legal {
            width: min(760px, calc(100% - 36px));
            margin: 0 auto;
            padding: clamp(28px, 4vw, 44px) 0 clamp(40px, 5vw, 64px);
            color: var(--apx-copy, #d8dcf4);
            line-height: 1.65;
          }
          .apx-legal h1 {
            margin: 0;
            color: var(--apx-ink, #f5f3ff);
            font-family: var(--apx-display, Georgia, serif);
            font-size: var(--apx-fs-h1, clamp(2rem, 3.6vw, 3.1rem));
            line-height: 1.05;
            letter-spacing: -0.03em;
          }
          .apx-legal-updated { margin: 10px 0 22px; color: var(--apx-muted, #9ca6cc); font: 600 var(--apx-fs-micro, 0.72rem)/1.4 var(--apx-mono, ui-monospace, monospace); letter-spacing: 0.04em; }
          .apx-legal-note {
            margin: 0 0 24px;
            border: 1px solid rgba(185, 152, 255, 0.32);
            border-radius: 12px;
            background: rgba(109, 93, 252, 0.09);
            padding: 12px 14px;
            color: var(--apx-copy, #d8dcf4);
            font-size: 0.875rem;
            line-height: 1.55;
          }
          .apx-legal h2 { margin: 30px 0 8px; color: var(--apx-sky, #7ddcff); font-size: 1.15rem; font-weight: 650; letter-spacing: -0.01em; }
          .apx-legal h3 { margin: 20px 0 6px; color: var(--apx-ink, #f5f3ff); font-size: 1rem; font-weight: 650; }
          .apx-legal p, .apx-legal li { color: var(--apx-copy, #d8dcf4); font-size: 0.95rem; }
          .apx-legal ul, .apx-legal ol { padding-left: 1.3rem; }
          .apx-legal li { margin: 6px 0; }
          .apx-legal a { color: var(--apx-mint, #64d8ff); text-underline-offset: 0.18em; }
          .apx-legal code { border-radius: 4px; background: rgba(125, 220, 255, 0.1); padding: 0.08rem 0.32rem; color: var(--apx-sky, #7ddcff); font: 0.88em var(--apx-mono, ui-monospace, monospace); }
          .apx-legal footer { margin-top: 36px; border-top: 1px solid var(--apx-line, rgba(169, 181, 255, 0.17)); padding-top: 16px; color: var(--apx-muted, #9ca6cc); }
          .apx-legal footer p { color: var(--apx-muted, #9ca6cc); font-size: 0.8rem; }
        `}</style>
      </Head>
      <article className="apx-legal">
        <h1>{title}</h1>
        <p className="apx-legal-updated">Last updated {updated}</p>
        <div className="apx-legal-note">
          This is working legal text written for clarity. It has not been presented as a substitute for
          professional legal review.
        </div>
        {children}
      </article>
    </>
  );
}
