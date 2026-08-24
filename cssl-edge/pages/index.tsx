import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { consumeAuthCallbackFromLocation, readAuthCallbackParams } from '../lib/auth-callback';
import { normalizeAuthReturnPath } from '../lib/auth-return';
import { SUPPORT_LINKS } from '../lib/support-links';
import { useSiteSession } from '../components/hub/SiteSession';

const PORTALS = [
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

const CREATIVE_WORK = [
  {
    title: 'Omnoid Singularity',
    copy: 'My concise cosmology of recursive totality, distinct centers, freedom, True Neutral, singularity, and return—with visual and CSLv3 maps.',
    href: '/omnoid-singularity',
    label: 'Read the cosmology',
    external: false,
  },
  {
    title: 'Akashic Records',
    copy: 'A searchable, hash-sealed public archive of my approved writing and public-safe Codex conversation transcripts.',
    href: '/akashic-records',
    label: 'Explore the archive',
    external: false,
  },
  {
    title: 'Chaos Tarot',
    copy: 'An evolving symbolic-art and tarot project built around atmosphere, reflection, and authored interpretation.',
    href: 'https://chaos-tarot.com',
    label: 'Enter Chaos Tarot',
    external: true,
  },
  {
    title: 'Labyrinth of Apocalypse',
    copy: 'A game world shaped by procedural history, persistent consequences, strange systems, and discovery.',
    href: '/download',
    label: 'Explore the game',
    external: false,
  },
  {
    title: 'CSSL',
    copy: 'A programming language for expressing and building interconnected software systems.',
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
    description: 'The creative work and projects of Shawn Apocky: games, languages, symbolic art, writing, and living systems.',
  };

  return (
    <>
      <Head>
        <title>Apocky — creative works and projects</title>
        <meta name="description" content="The creative work and projects of Shawn Apocky: games, languages, symbolic art, writing, and living systems." />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta property="og:title" content="Apocky — creative works and projects" />
        <meta property="og:description" content="Games, languages, symbolic art, writing, and interconnected creative systems by Shawn Apocky." />
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
            <p className="apx-eyebrow">Creative works · Shawn Apocky</p>
            <h1 id="hero-title">Worlds, languages, symbols, and <span className="apx-gradient-word">living systems.</span></h1>
            <p className="apx-hero-copy">
              This is the home of my games, software, writing, symbolic art, and
              works in progress. Explore the projects here or join the public
              community room.
            </p>
            <div className="apx-actions">
              <a href="#projects" className="apx-button apx-button--primary">Explore the work</a>
              <Link href="/clearing" className="apx-button">Enter the Clearing</Link>
              {authenticated ? <Link href="/account" className="apx-button">Your account</Link> : null}
            </div>
            <p className="apx-auth-message" aria-live="polite" hidden={!authNotice}>{authNotice}</p>
          </div>

          <a href="https://chaos-tarot.com" target="_blank" rel="noopener noreferrer" className="apx-presence-card" aria-label="Visit the Chaos Tarot creative project">
            <div className="apx-presence-field" aria-hidden="true">
              <span className="apx-presence-orbit apx-presence-orbit--outer" />
              <span className="apx-presence-orbit apx-presence-orbit--inner" />
              <span className="apx-presence-core" />
            </div>
            <div className="apx-presence-copy">
              <p className="apx-presence-label">Featured creative work</p>
              <h2>Chaos Tarot</h2>
              <p>
                An evolving symbolic-art project with its own atmosphere,
                visual language, and way of exploring the cards.
              </p>
              <span className="apx-presence-link">Enter Chaos Tarot <span aria-hidden="true">↗</span></span>
            </div>
          </a>
        </section>

        <section id="projects" className="apx-section" aria-labelledby="projects-title">
          <div className="apx-section-head">
            <div>
              <p className="apx-kicker">Selected work</p>
              <h2 id="projects-title">Creative projects and systems.</h2>
            </div>
            <p className="apx-section-intro">
              Each project has its own identity and purpose. This page keeps
              them together without flattening them into one product.
            </p>
          </div>

          <div className="apx-project-list">
            {CREATIVE_WORK.map((project) => (
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

        <section id="support" className="apx-section apx-section--support" aria-labelledby="support-title">
          <div className="apx-support-band">
            <div>
              <p className="apx-kicker">Optional support</p>
              <h2 id="support-title">Help sustain the work.</h2>
              <p className="apx-support-copy">
                If you would like to help keep Apocky’s writing, software, and creative projects moving,
                Ko-fi and Patreon are available. Support is appreciated, never required, and does not buy
                control over creative decisions.
              </p>
              <Link href="/buy" className="apx-support-details">
                Read how support works <span aria-hidden="true">→</span>
              </Link>
            </div>

            <div className="apx-support-options" aria-label="Ways to support Apocky">
              {SUPPORT_LINKS.map((link) => (
                <a
                  key={link.name}
                  className="apx-support-option"
                  href={link.href}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  <span>
                    <strong>{link.name}</strong>
                    <small>{link.description}</small>
                  </span>
                  <span className="apx-support-option-action">
                    {link.label} <span aria-hidden="true">↗</span>
                  </span>
                </a>
              ))}
            </div>
          </div>
        </section>

        <section id="doorways" className="apx-section apx-section--compact" aria-labelledby="doorways-title">
          <div className="apx-section-head">
            <div>
              <p className="apx-kicker">Communication & community</p>
              <h2 id="doorways-title">Separate places, clear purposes.</h2>
            </div>
            <p className="apx-section-intro">
              The homepage stays focused on the work. These portals open only
              when you choose conversation, community, or deeper context.
            </p>
          </div>

          <div className="apx-door-grid">
            {PORTALS.map((door) => (
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
      </main>
    </>
  );
};

export default Home;
