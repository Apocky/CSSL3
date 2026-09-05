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
            width: min(780px, calc(100% - 36px));
            margin: 0 auto;
            padding: clamp(54px, 8vw, 90px) 0 100px;
            color: #d6dfdd;
            line-height: 1.7;
          }
          .apx-legal h1 {
            margin: 0;
            color: #f5f8f7;
            font-size: clamp(2.25rem, 6vw, 4.8rem);
            line-height: 1;
            letter-spacing: -0.055em;
          }
          .apx-legal-updated { margin: 14px 0 34px; color: #758783; font-size: 0.82rem; }
          .apx-legal-note {
            margin: 0 0 30px;
            border: 1px solid rgba(255, 196, 125, 0.3);
            border-radius: 12px;
            background: rgba(255, 196, 125, 0.06);
            padding: 16px 18px;
            color: #e9d0b1;
            font-size: 0.88rem;
          }
          .apx-legal h2 { margin: 38px 0 10px; color: #adffef; font-size: 1.18rem; letter-spacing: -0.01em; }
          .apx-legal h3 { margin: 26px 0 8px; color: #d6dfdd; font-size: 1rem; }
          .apx-legal p, .apx-legal li { color: #bdc9c7; font-size: 0.94rem; }
          .apx-legal ul, .apx-legal ol { padding-left: 1.35rem; }
          .apx-legal li { margin: 7px 0; }
          .apx-legal a { color: #8ddcff; }
          .apx-legal code { border-radius: 4px; background: rgba(141, 220, 255, 0.08); padding: 0.1rem 0.3rem; color: #b9eaff; }
          .apx-legal footer { margin-top: 54px; border-top: 1px solid rgba(153, 204, 194, 0.14); padding-top: 22px; color: #758783; }
          .apx-legal footer p { color: #758783; font-size: 0.8rem; }
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
