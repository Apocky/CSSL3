import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { consumeAuthCallbackFromLocation, readAuthCallbackParams } from '../lib/auth-callback';
import { normalizeAuthReturnPath } from '../lib/auth-return';
import { useSiteSession } from '../components/hub/SiteSession';
import CyberDreamField from '../components/cyber/CyberDreamField';

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

      <main className="apx-home apx-dream-home">
        <CyberDreamField variant="commons" activity="idle" density={1.12} viewport />
        <div className="apx-dream-vignette" aria-hidden="true" />

        <section className="apx-dream-hero" aria-labelledby="hero-title">
          <div className="apx-dream-copy">
            <p className="apx-eyebrow"><span /> APOCKY / COMMUNICATION COMMONS</p>
            <h1 id="hero-title">A relay for minds that refuse <em>small worlds.</em></h1>
            <p className="apx-hero-copy">
              Speak with Apocrypha, trace the systems and ideas around it, or
              enter a shared room. One living interface connects intelligence,
              language, art, research, and the strange spaces between them.
            </p>
            <div className="apx-actions">
              <Link href="/apocrypha" className="apx-button apx-button--primary">Open the relay <span aria-hidden="true">↗</span></Link>
              <a href="#doorways" className="apx-button">Navigate the field</a>
              {authenticated ? <Link href="/account" className="apx-button">Your orbit</Link> : null}
            </div>
            <p className="apx-auth-message" aria-live="polite" hidden={!authNotice}>{authNotice}</p>
          </div>

          <Link href="/apocrypha" className="apx-dream-presence" aria-label="Open the Apocrypha communication relay">
            <span className="apx-dream-presence-label">LIVE INTERFACE / APOCRYPHA</span>
            <div className="apx-dream-orb" aria-hidden="true">
              <span className="apx-dream-orbit apx-dream-orbit--one" />
              <span className="apx-dream-orbit apx-dream-orbit--two" />
              <span className="apx-dream-orbit apx-dream-orbit--three" />
              <span className="apx-dream-core">§A</span>
              <span className="apx-dream-scan" />
            </div>
            <div className="apx-dream-presence-copy">
              <strong>Begin with a signal.</strong>
              <span>Conversation is the center; context, tools, and evidence unfold around it.</span>
            </div>
          </Link>

          <div className="apx-dream-index" aria-hidden="true">
            <span>01 / INTELLIGENCE</span><span>02 / LANGUAGE</span><span>03 / SHARED WORLD</span>
          </div>
        </section>

        <section id="doorways" className="apx-world-section" aria-labelledby="doorways-title">
          <div className="apx-world-heading">
            <p className="apx-kicker">Connected chambers</p>
            <h2 id="doorways-title">Move through the same world from a different angle.</h2>
            <p>Each chamber changes the mode of contact without severing context from the whole.</p>
          </div>

          <div className="apx-constellation" data-canvasui-composition="grid+force-field+portal-map">
            <svg className="apx-constellation-lines" viewBox="0 0 1000 590" preserveAspectRatio="none" aria-hidden="true">
              <path d="M500 295 C365 205 250 180 118 170" />
              <path d="M500 295 C640 190 760 175 885 155" />
              <path d="M500 295 C620 390 745 430 888 450" />
              <path d="M118 170 C330 430 690 495 888 450" />
            </svg>
            <span className="apx-constellation-pulse" aria-hidden="true" />
            {DOORS.map((door, index) => (
              <Link key={door.title} href={door.href} className={`apx-world-node apx-world-node--${index + 1}`}>
                <span className="apx-world-node-index">0{index + 1}</span>
                <span className={`apx-door-glyph ${door.glyph}`} aria-hidden="true" />
                <span className="apx-world-node-copy">
                  <small>{door.kind}</small>
                  <strong>{door.title}</strong>
                  <span>{door.copy}</span>
                </span>
                <b>{door.label} <span aria-hidden="true">↗</span></b>
              </Link>
            ))}
            <div className="apx-world-nucleus" aria-hidden="true"><span>APOCKY</span><i>∞</i><small>ONE CONTEXT / MANY MODES</small></div>
          </div>
        </section>

        <section id="projects" className="apx-orbit-section" aria-labelledby="projects-title">
          <div className="apx-orbit-heading">
            <p className="apx-kicker">Outer orbit</p>
            <h2 id="projects-title">Other instruments in the same cosmology.</h2>
          </div>
          <div className="apx-orbit-list">
            {OTHER_WORK.map((project, index) => (
              project.external ? (
                <a key={project.title} href={project.href} target="_blank" rel="noopener noreferrer" className="apx-orbit-link">
                  <i>0{index + 4}</i><span><strong>{project.title}</strong><small>{project.copy}</small></span><b>↗</b>
                </a>
              ) : (
                <Link key={project.title} href={project.href} className="apx-orbit-link">
                  <i>0{index + 4}</i><span><strong>{project.title}</strong><small>{project.copy}</small></span><b>→</b>
                </Link>
              )
            ))}
          </div>
        </section>

        <section className="apx-covenant-section" aria-labelledby="principles-title">
          <div className="apx-covenant-mark" aria-hidden="true"><span>∞</span></div>
          <div className="apx-covenant-copy">
            <p className="apx-kicker">The membrane</p>
            <h2 id="principles-title">Wild possibility. Clear boundaries.</h2>
            <p>Ambition expands inside consent, provenance, and reversible action. The interface keeps those boundaries legible without turning the experience into bureaucracy.</p>
          </div>
          <div className="apx-covenant-list">
            <div><i>01</i><strong>Consent</strong><span>Participation is chosen and withdrawable.</span></div>
            <div><i>02</i><strong>Context</strong><span>Depth appears when it changes the decision.</span></div>
            <div><i>03</i><strong>Truth</strong><span>Proposal, observation, and proof stay distinct.</span></div>
          </div>
        </section>
      </main>
    </>
  );
};

export default Home;
