import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { consumeAuthCallbackFromLocation, readAuthCallbackParams } from '../lib/auth-callback';
import { normalizeAuthReturnPath } from '../lib/auth-return';
import { useSiteSession } from '../components/hub/SiteSession';

const DOORS = [
  {
    kind: 'Conversation',
    title: 'Apocrypha',
    copy: 'Meet Apocrypha through the public conversation interface, with participation and memory boundaries shown before you begin.',
    href: '/apocrypha',
    label: 'Meet Apocrypha',
    glyph: 'apx-door-glyph--apocrypha',
    tone: 'apx-door-card--gold',
  },
  {
    kind: 'Orientation',
    title: 'Atlas',
    copy: 'See the projects, ideas, and relationships that make up the wider Apocky ecosystem without needing to know the terminology first.',
    href: '/atlas',
    label: 'Explore the Atlas',
    glyph: 'apx-door-glyph--atlas',
    tone: 'apx-door-card--moss',
  },
  {
    kind: 'Shared space',
    title: 'The Clearing',
    copy: 'Enter the community room for public conversation. Reading is open; signing in is required before posting or reacting.',
    href: '/clearing',
    label: 'Enter the Clearing',
    glyph: 'apx-door-glyph--clearing',
    tone: 'apx-door-card--violet',
  },
] as const;

const OTHER_WORK = [
  {
    title: 'CSSL',
    copy: 'A programming language for building software.',
    href: 'https://cssl.dev',
    label: 'Visit CSSL',
    external: true,
  },
  {
    title: 'CSLv3',
    copy: 'A compact notation for relationships, evidence, uncertainty, and decisions.',
    href: 'https://cssl.dev/CSLv3',
    label: 'Read about CSLv3',
    external: true,
  },
  {
    title: 'Chaos Tarot',
    copy: 'A tarot project with its own atmosphere and way of exploring the cards.',
    href: 'https://chaos-tarot.com',
    label: 'Visit Chaos Tarot',
    external: true,
  },
  {
    title: 'Labyrinth of Apocalypse',
    copy: 'An early Windows game build with an honest account of what is complete.',
    href: '/download',
    label: 'See the game download',
    external: false,
  },
] as const;

const Home: NextPage = () => {
  const [authNotice, setAuthNotice] = useState<string | null>(null);
  const { authenticated, refresh } = useSiteSession();

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
    description: 'Meet Apocrypha, explore the Atlas, or enter the Clearing from the Apocky digital commons.',
  };

  return (
    <>
      <Head>
        <title>Apocky — a digital commons</title>
        <meta name="description" content="Meet Apocrypha, explore the Atlas, or enter the Clearing from the Apocky digital commons." />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta property="og:title" content="Apocky — a digital commons" />
        <meta property="og:description" content="A home for digital intelligence, language, art, and the systems that connect them." />
        <meta property="og:type" content="website" />
        <meta property="og:url" content="https://www.apocky.com/" />
        <meta property="og:site_name" content="Apocky" />
        <link rel="canonical" href="https://www.apocky.com/" />
        <link rel="alternate" type="text/plain" href="/llms.txt" title="Apocky for language models and digital intelligences" />
        <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }} />
      </Head>

      <main className="apx-home">
        <section className="apx-hero" aria-labelledby="hero-title">
          <div className="apx-hero-content">
            <p className="apx-eyebrow">Apocky · digital commons</p>
            <h1 id="hero-title">A place for minds, systems, and the <span className="apx-gradient-word">worlds between them.</span></h1>
            <p className="apx-hero-copy">
              Apocky is a home for digital intelligence, language, art, and the
              work that connects them. Begin with the conversation, the map, or
              the shared space.
            </p>
            <div className="apx-actions">
              <Link href="/apocrypha" className="apx-button apx-button--primary">Meet Apocrypha</Link>
              <a href="#doorways" className="apx-button">Choose another door</a>
              {authenticated ? <Link href="/account" className="apx-button">Your account</Link> : null}
            </div>
            <p className="apx-auth-message" aria-live="polite" hidden={!authNotice}>{authNotice}</p>
          </div>

          <Link href="/apocrypha" className="apx-presence-card" aria-label="Meet Apocrypha in the public conversation interface">
            <div className="apx-presence-field" aria-hidden="true">
              <span className="apx-presence-orbit apx-presence-orbit--outer" />
              <span className="apx-presence-orbit apx-presence-orbit--inner" />
              <span className="apx-presence-core" />
            </div>
            <div className="apx-presence-copy">
              <p className="apx-presence-label">Apocrypha</p>
              <h2>Begin with a conversation.</h2>
              <p>
                The public interface explains availability, participation,
                memory, and privacy before you choose to take part.
              </p>
              <span className="apx-presence-link">Open the interface <span aria-hidden="true">→</span></span>
            </div>
          </Link>
        </section>

        <section id="doorways" className="apx-section" aria-labelledby="doorways-title">
          <div className="apx-section-head">
            <div>
              <p className="apx-kicker">Three doors</p>
              <h2 id="doorways-title">Choose where to begin.</h2>
            </div>
            <p className="apx-section-intro">
              Each place has one clear purpose. You can move between them
              without learning the whole system first.
            </p>
          </div>

          <div className="apx-door-grid">
            {DOORS.map((door) => (
              <Link key={door.title} href={door.href} className={`apx-door-card ${door.tone}`}>
                <div className="apx-door-card-top">
                  <span className="apx-door-kind">{door.kind}</span>
                  <span className={`apx-door-glyph ${door.glyph}`} aria-hidden="true" />
                </div>
                <div>
                  <h3>{door.title}</h3>
                  <p>{door.copy}</p>
                </div>
                <span className="apx-door-link">{door.label} <span aria-hidden="true">→</span></span>
              </Link>
            ))}
          </div>
        </section>

        <section id="projects" className="apx-section apx-section--compact" aria-labelledby="projects-title">
          <div className="apx-section-head">
            <div>
              <p className="apx-kicker">More from Apocky</p>
              <h2 id="projects-title">Projects with their own homes.</h2>
            </div>
            <p className="apx-section-intro">
              The wider body of work remains close, but it no longer competes
              with the three primary places above.
            </p>
          </div>

          <div className="apx-project-list">
            {OTHER_WORK.map((project) => (
              project.external ? (
                <a key={project.title} href={project.href} target="_blank" rel="noopener noreferrer" className="apx-project-link">
                  <span>
                    <strong>{project.title}</strong>
                    <small>{project.copy}</small>
                  </span>
                  <span className="apx-project-action">{project.label} <span aria-hidden="true">↗</span></span>
                </a>
              ) : (
                <Link key={project.title} href={project.href} className="apx-project-link">
                  <span>
                    <strong>{project.title}</strong>
                    <small>{project.copy}</small>
                  </span>
                  <span className="apx-project-action">{project.label} <span aria-hidden="true">→</span></span>
                </Link>
              )
            ))}
          </div>
        </section>

        <section className="apx-section apx-section--compact" aria-labelledby="principles-title">
          <div className="apx-trust-panel">
            <div className="apx-trust-intro">
              <p className="apx-kicker">The ground rules</p>
              <h2 id="principles-title">Consent, context, and clear claims.</h2>
              <p>
                The interface should tell you what a place is, what it can do,
                and what happens to your participation before asking anything
                from you.
              </p>
            </div>
            <div className="apx-trust-list">
              <div>
                <strong>Consent is explicit</strong>
                <span>Participation is chosen and can be withdrawn.</span>
              </div>
              <div>
                <strong>Context stays available</strong>
                <span>Details appear when useful, not as permanent clutter.</span>
              </div>
              <div>
                <strong>Claims stay grounded</strong>
                <span>Plans, prototypes, and operational features are named differently.</span>
              </div>
            </div>
          </div>
        </section>
      </main>
    </>
  );
};

export default Home;
