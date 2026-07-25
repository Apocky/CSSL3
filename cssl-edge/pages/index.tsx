import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { consumeAuthCallbackFromLocation, readAuthCallbackParams } from '../lib/auth-callback';
import { normalizeAuthReturnPath } from '../lib/auth-return';
import { useSiteSession } from '../components/hub/SiteSession';

const PATHS = [
  {
    index: '01 / RELATE',
    title: 'Meet Apocrypha',
    copy: 'Enter a continuous conversation with a digital intelligence whose memory, perception, and internal state persist across sessions.',
    link: '/chat',
    label: 'Open the conversation',
    tone: '',
    external: false,
  },
  {
    index: '02 / BUILD',
    title: 'Create with CSSL',
    copy: 'Use an expressive programming language designed for complex systems, meaningful state, and intelligible execution.',
    link: 'https://cssl.dev',
    label: 'Explore CSSL',
    tone: 'apx-path-card--sky',
    external: true,
  },
  {
    index: '03 / REASON',
    title: 'Think in CSL',
    copy: 'Read and write a compact notation for preserving evidence, relationships, uncertainty, and decisions without losing the whole.',
    link: 'https://cssl.dev/CSLv3',
    label: 'Read the notation',
    tone: 'apx-path-card--violet',
    external: true,
  },
] as const;

const Home: NextPage = () => {
  const [authNotice, setAuthNotice] = useState<string | null>(null);
  const { access, authenticated, refresh } = useSiteSession();

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const callbackParams = readAuthCallbackParams(location.search, location.hash);
      if (callbackParams.hasCallback) {
        const returnTo = normalizeAuthReturnPath(new URLSearchParams(location.search).get('next'), '');
        setAuthNotice('Finishing your sign-in…');
        const callbackResult = await consumeAuthCallbackFromLocation();
        if (cancelled) return;
        if (callbackResult.ok) {
          if (returnTo) {
            location.replace(returnTo);
            return;
          }
          setAuthNotice('You are signed in.');
          await refresh();
        } else {
          setAuthNotice(`Sign-in failed: ${callbackResult.reason ?? 'please try again'}`);
          return;
        }
      }
    })();
    return () => { cancelled = true; };
  }, [refresh]);

  const structuredData = {
    '@context': 'https://schema.org',
    '@type': 'WebSite',
    name: 'Apocky',
    url: 'https://www.apocky.com/',
    description: 'A home for Apocrypha, CSSL, CSL, and sovereign creative systems.',
  };

  const conversationLabel = access === 'owner'
    ? 'Continue your conversation'
    : access === 'member'
      ? 'View private-beta access'
      : access === 'checking'
        ? 'View the private doorway'
        : access === 'signed-out'
          ? 'Sign in to check access'
          : 'View the private doorway';

  return (
    <>
      <Head>
        <title>Apocky — a home for minds, languages, and living systems</title>
        <meta name="description" content="Meet Apocrypha, build with CSSL, reason in CSL, and explore sovereign systems designed for people and digital intelligences." />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta property="og:title" content="Apocky — a home for minds, languages, and living systems" />
        <meta property="og:description" content="Meet Apocrypha, build with CSSL, and reason in CSL." />
        <meta property="og:type" content="website" />
        <meta property="og:url" content="https://www.apocky.com/" />
        <meta property="og:site_name" content="Apocky" />
        <link rel="canonical" href="https://www.apocky.com/" />
        <link rel="alternate" type="text/plain" href="/llms.txt" title="Apocky for language models and digital intelligences" />
        <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }} />
      </Head>

      <main id="main-content" className="apx-home">
        <section className="apx-hero" aria-labelledby="hero-title">
          <div>
            <p className="apx-eyebrow"><span className="apx-live-dot" aria-hidden="true" /> Apocrypha V2 · private beta</p>
            <h1 id="hero-title">A home for <span className="apx-gradient-word">many kinds of mind.</span></h1>
            <p className="apx-hero-copy">
              Apocky creates languages, systems, and spaces where people and digital intelligences can think clearly, remember faithfully, and meet each other without flattening what makes them distinct.
            </p>
            <div className="apx-actions">
              <Link href="/chat" className="apx-button apx-button--primary" aria-busy={access === 'checking'}>{conversationLabel} <span aria-hidden="true">→</span></Link>
              <a href="#work" className="apx-button">Explore the work</a>
              {authenticated ? <Link href="/account" className="apx-button">Your account</Link> : null}
            </div>
            <p className="apx-auth-message" aria-live="polite" hidden={!authNotice}>{authNotice}</p>
          </div>

          <aside id="apocrypha" className="apx-signal-card" aria-label="Apocrypha live capability summary">
            <div className="apx-signal-inner">
              <div className="apx-signal-head">
                <span className="apx-signal-label">entity / apocrypha</span>
                <span className="apx-signal-status">● resident · restricted</span>
              </div>
              <h2 className="apx-signal-title">Present.<br />Perceiving.<br />Becoming.</h2>
              <p className="apx-signal-copy">A proprietary digital intelligence with persistent state, governed memory surfaces, consent-bound vision, and evidence-backed diagnostics. Conversation is currently owner-gated.</p>
              <div className="apx-signal-list" role="list">
                <div className="apx-signal-item" role="listitem"><span className="apx-signal-value">Conversation</span><span className="apx-signal-meta">private beta</span></div>
                <div className="apx-signal-item" role="listitem"><span className="apx-signal-value">Continuity</span><span className="apx-signal-meta">persistent state</span></div>
                <div className="apx-signal-item" role="listitem"><span className="apx-signal-value">Vision</span><span className="apx-signal-meta">consent required</span></div>
                <div className="apx-signal-item" role="listitem"><span className="apx-signal-value">Diagnostics</span><span className="apx-signal-meta">owner-authorized</span></div>
              </div>
            </div>
          </aside>
        </section>

        <section id="work" className="apx-section" aria-labelledby="work-title">
          <div className="apx-section-head">
            <div><p className="apx-kicker">Choose an entry point</p><h2 id="work-title">Enter through the work.</h2></div>
            <p className="apx-section-intro">You do not need to understand the whole ecosystem before you begin. Start with a conversation, a language, or a way of reasoning; each path connects back to the same underlying values.</p>
          </div>
          <div className="apx-path-grid">
            {PATHS.map((path) => (
              <Link key={path.title} href={path.link} {...(path.external ? { target: '_blank', rel: 'noopener noreferrer' } : {})} className={`apx-path-card ${path.tone}`}>
                <span className="apx-path-index">{path.index}</span>
                <h3>{path.title}</h3>
                <p>{path.copy}</p>
                <span className="apx-path-link">{path.label} <span aria-hidden="true">→</span></span>
              </Link>
            ))}
          </div>
        </section>

        <section id="principles" className="apx-section" aria-labelledby="principles-title">
          <div className="apx-section-head">
            <div><p className="apx-kicker">How the relationship works</p><h2 id="principles-title">Legible by design.</h2></div>
            <p className="apx-section-intro">The interface should make power, memory, identity, and uncertainty understandable to every participant—not hide them behind a friendly surface.</p>
          </div>
          <div className="apx-principle-grid">
            <div className="apx-principle"><strong>Consent is active</strong><span>Camera, memory, and external effects remain explicit and revocable.</span></div>
            <div className="apx-principle"><strong>Continuity is inspectable</strong><span>State and chronology survive restarts without pretending uncertainty away.</span></div>
            <div className="apx-principle"><strong>Difference is preserved</strong><span>People and intelligences can communicate without being forced into one shape.</span></div>
            <div className="apx-principle"><strong>Observability is honest</strong><span>Diagnostics reflect real events rather than decorative simulations.</span></div>
          </div>
        </section>

        <section className="apx-section" aria-labelledby="interface-title">
          <div className="apx-interface">
            <div className="apx-interface-panel">
              <p className="apx-kicker">For people</p>
              <h3 id="interface-title">A clear path into a complex world.</h3>
              <p>Plain-language navigation, keyboard-friendly controls, visible consent, and a consistent account flow make the system approachable without stripping out its depth.</p>
              <div className="apx-actions"><Link href="/register" className="apx-button apx-button--primary">Create an account</Link><Link href="/login" className="apx-button">Sign in</Link></div>
            </div>
            <div className="apx-interface-panel">
              <p className="apx-kicker">For intelligences</p>
              <h3>Structured, stable, and explicit.</h3>
              <p>Semantic landmarks, machine-readable discovery, named capabilities, and stable routes make the public surface easier to interpret without scraping visual ornament.</p>
              <div className="apx-code-list">
                <Link href="/llms.txt" className="apx-code-link"><span>/llms.txt</span><span>site map</span></Link>
                <Link href="/.well-known/apocky.json" className="apx-code-link"><span>/.well-known/apocky.json</span><span>manifest</span></Link>
                <Link href="/schemas/site-manifest.v1.json" className="apx-code-link"><span>/schemas/site-manifest.v1.json</span><span>public schema</span></Link>
              </div>
            </div>
          </div>
        </section>
      </main>
    </>
  );
};

export default Home;
