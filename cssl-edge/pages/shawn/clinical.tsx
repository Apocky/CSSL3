import type { GetServerSideProps, NextApiRequest, NextPage } from 'next';
import Head from 'next/head';

import type { ClinicalDossier } from '../../lib/shawn/clinical-auth';

type ClinicalPageProps =
  | { readonly state: 'authorized'; readonly dossier: ClinicalDossier }
  | { readonly state: 'forbidden' }
  | { readonly state: 'unavailable' };

function applyPrivateResponseHeaders(res: Parameters<GetServerSideProps>[0]['res']): void {
  res.setHeader('Cache-Control', 'private, no-store, max-age=0, must-revalidate');
  res.setHeader('Surrogate-Control', 'no-store');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Expires', '0');
  res.setHeader('Referrer-Policy', 'no-referrer');
  res.setHeader('X-Robots-Tag', 'noindex, nofollow, noarchive, nosnippet, noimageindex');
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('X-Frame-Options', 'DENY');
}

export const getServerSideProps: GetServerSideProps<ClinicalPageProps> = async (context) => {
  applyPrivateResponseHeaders(context.res);

  // Dynamic import keeps the service-role boundary out of the browser bundle.
  const { resolveClinicalRoute } = await import('../../lib/shawn/clinical-auth');
  const decision = await resolveClinicalRoute(context.req as unknown as NextApiRequest);

  if (decision.kind === 'redirect') {
    return { redirect: { destination: decision.destination, permanent: false } };
  }

  context.res.statusCode = decision.statusCode;
  return { props: decision.props };
};

const frameStyle: React.CSSProperties = {
  minHeight: '100dvh',
  margin: 0,
  background: 'radial-gradient(circle at 12% 0%, #172028 0%, #090c10 42%, #050607 100%)',
  color: '#e8e3d8',
  fontFamily: 'Georgia, Cambria, Times New Roman, serif',
};

const panelStyle: React.CSSProperties = {
  maxWidth: 900,
  margin: '0 auto',
  padding: 'clamp(2rem, 6vw, 5rem) clamp(1rem, 4vw, 2.5rem) 6rem',
};

const ClinicalPage: NextPage<ClinicalPageProps> = (props) => {
  const authorized = props.state === 'authorized';
  const title = authorized ? props.dossier.title : 'Private clinician dossier';

  return (
    <div style={frameStyle}>
      <Head>
        <title>Private clinician dossier · Shawn / Apocky</title>
        <meta name="robots" content="noindex, nofollow, noarchive, nosnippet, noimageindex" />
        <meta name="googlebot" content="noindex, nofollow, noarchive, nosnippet, noimageindex" />
        <meta name="referrer" content="no-referrer" />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      </Head>

      <main style={panelStyle}>
        <a
          href="/shawn"
          style={{ color: '#9fc6c0', fontFamily: 'ui-monospace, monospace', fontSize: '0.82rem' }}
        >
          ← public atlas
        </a>

        <header style={{ margin: '3rem 0 2.5rem', borderBottom: '1px solid #354039', paddingBottom: '1.5rem' }}>
          <p
            style={{
              margin: '0 0 0.75rem',
              color: '#b7925a',
              fontFamily: 'ui-monospace, monospace',
              fontSize: '0.72rem',
              letterSpacing: '0.16em',
              textTransform: 'uppercase',
            }}
          >
            Restricted · authenticated dossier
          </p>
          <h1 style={{ margin: 0, fontSize: 'clamp(2rem, 6vw, 4rem)', lineHeight: 1.04, fontWeight: 500 }}>
            {title}
          </h1>
          {authorized && props.dossier.updatedAt && (
            <p style={{ color: '#9aa39c', fontFamily: 'ui-monospace, monospace', fontSize: '0.78rem' }}>
              Updated <time dateTime={props.dossier.updatedAt}>{props.dossier.updatedAt}</time>
            </p>
          )}
        </header>

        {props.state === 'forbidden' && (
          <section aria-labelledby="access-heading" style={{ maxWidth: 620 }}>
            <h2 id="access-heading" style={{ fontWeight: 500 }}>Access not granted</h2>
            <p style={{ color: '#b8beb8', lineHeight: 1.7 }}>
              Your account is signed in, but it does not have active access to this private dossier.
            </p>
          </section>
        )}

        {props.state === 'unavailable' && (
          <section aria-labelledby="unavailable-heading" style={{ maxWidth: 620 }}>
            <h2 id="unavailable-heading" style={{ fontWeight: 500 }}>Dossier unavailable</h2>
            <p style={{ color: '#b8beb8', lineHeight: 1.7 }}>
              The private dossier cannot be opened right now. No dossier content was disclosed.
            </p>
          </section>
        )}

        {authorized && (
          <article>
            {props.dossier.notice && (
              <aside
                style={{
                  marginBottom: '2.5rem',
                  padding: '1rem 1.15rem',
                  border: '1px solid #665634',
                  background: 'rgba(183, 146, 90, 0.08)',
                  color: '#d7c8ac',
                  lineHeight: 1.65,
                }}
              >
                {props.dossier.notice}
              </aside>
            )}

            {props.dossier.sections.map((section, index) => (
              <section
                key={section.id}
                id={section.id}
                aria-labelledby={`${section.id}-heading`}
                style={{ padding: '2rem 0', borderTop: index === 0 ? 'none' : '1px solid #252d29' }}
              >
                <h2 id={`${section.id}-heading`} style={{ fontSize: 'clamp(1.35rem, 3vw, 2rem)', fontWeight: 500 }}>
                  {section.title}
                </h2>
                {section.paragraphs.map((paragraph, paragraphIndex) => (
                  <p key={paragraphIndex} style={{ maxWidth: '76ch', color: '#d1d3cc', lineHeight: 1.78 }}>
                    {paragraph}
                  </p>
                ))}
                {section.points && section.points.length > 0 && (
                  <ul style={{ maxWidth: '74ch', color: '#c8cbc4', lineHeight: 1.72, paddingLeft: '1.4rem' }}>
                    {section.points.map((point, pointIndex) => <li key={pointIndex}>{point}</li>)}
                  </ul>
                )}
              </section>
            ))}
          </article>
        )}
      </main>
    </div>
  );
};

export default ClinicalPage;
