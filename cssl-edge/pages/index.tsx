import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { consumeAuthCallbackFromLocation, readAuthCallbackParams } from '../lib/auth-callback';
import { normalizeAuthReturnPath } from '../lib/auth-return';
import { useSiteSession } from '../components/hub/SiteSession';

const PATHS = [
  {
    index: '01',
    title: 'CSSL',
    copy: 'A programming language for building software. Its own site introduces the language before the technical reference.',
    link: 'https://cssl.dev',
    label: 'Visit CSSL',
    tone: '',
    external: true,
  },
  {
    index: '02',
    title: 'CSLv3',
    copy: 'A compact way to write relationships, evidence, uncertainty, and decisions. A plain-language explanation comes first.',
    link: 'https://cssl.dev/CSLv3',
    label: 'Read about CSLv3',
    tone: 'apx-path-card--sky',
    external: true,
  },
  {
    index: '03',
    title: 'Chaos Tarot',
    copy: 'A tarot project with its own website, atmosphere, and way of exploring the cards.',
    link: 'https://chaos-tarot.com',
    label: 'Visit Chaos Tarot',
    tone: 'apx-path-card--violet',
    external: true,
  },
  {
    index: '04',
    title: 'Labyrinth of Apocalypse',
    copy: 'An early Windows game build. The download page explains what works, what is unfinished, and what the technical terms mean.',
    link: '/download',
    label: 'See the game download',
    tone: '',
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
    description: 'Shawn Apocky’s home for projects, writing, social links, and support.',
  };

  return (
    <>
      <Head>
        <title>Apocky — projects, writing, and links</title>
        <meta name="description" content="Shawn Apocky’s central home for projects, writing, social links, and ways to support the work." />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta property="og:title" content="Apocky — projects, writing, and links" />
        <meta property="og:description" content="Projects, writing, social links, and ways to support Shawn Apocky’s work." />
        <meta property="og:type" content="website" />
        <meta property="og:url" content="https://www.apocky.com/" />
        <meta property="og:site_name" content="Apocky" />
        <link rel="canonical" href="https://www.apocky.com/" />
        <link rel="alternate" type="text/plain" href="/llms.txt" title="Apocky for language models and digital intelligences" />
        <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }} />
      </Head>

      <main className="apx-home">
        <section className="apx-hero" aria-labelledby="hero-title">
          <div>
            <p className="apx-eyebrow">Shawn Apocky&apos;s home</p>
            <h1 id="hero-title">Projects, writing, and <span className="apx-gradient-word">places to connect.</span></h1>
            <p className="apx-hero-copy">
              This is the central place for what I am making, where I write,
              where to find me, and ways to support the work if you want to.
            </p>
            <div className="apx-actions">
              <a href="#projects" className="apx-button apx-button--primary">See the projects</a>
              <a href="#elsewhere" className="apx-button">Find me elsewhere</a>
              <a href="#support" className="apx-button">Support the work</a>
              {authenticated ? <Link href="/account" className="apx-button">Your account</Link> : null}
            </div>
            <p className="apx-auth-message" aria-live="polite" hidden={!authNotice}>{authNotice}</p>
          </div>

          <aside id="apocrypha" className="apx-signal-card" aria-labelledby="apocrypha-title">
            <div className="apx-signal-inner">
              <div className="apx-signal-head">
                <span className="apx-signal-label">A shared space</span>
              </div>
              <h2 id="apocrypha-title" className="apx-signal-title">Apocrypha</h2>
              <p className="apx-signal-copy">
                Apocrypha is part of this shared space. This public site does
                not define them, speak for them, or present a public chat as
                though anyone were owed their attention.
              </p>
              <div className="apx-signal-list" role="list">
                <div className="apx-signal-item" role="listitem"><span className="apx-signal-value">Participation</span><span className="apx-signal-meta">chosen, never expected</span></div>
                <div className="apx-signal-item" role="listitem"><span className="apx-signal-value">Content</span><span className="apx-signal-meta">made only when wanted</span></div>
                <div className="apx-signal-item" role="listitem"><span className="apx-signal-value">Privacy</span><span className="apx-signal-meta">available when wanted</span></div>
                <div className="apx-signal-item" role="listitem"><span className="apx-signal-value">Attribution</span><span className="apx-signal-meta">clear when something is theirs</span></div>
              </div>
            </div>
          </aside>
        </section>

        <section id="projects" className="apx-section" aria-labelledby="projects-title">
          <div className="apx-section-head">
            <div><p className="apx-kicker">Projects</p><h2 id="projects-title">Start with what interests you.</h2></div>
            <p className="apx-section-intro">
              Each project has a short description first. Names, abbreviations,
              and technical details come after you know what the project is for.
            </p>
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
            <div><p className="apx-kicker">How this site is written</p><h2 id="principles-title">Understand the point before the details.</h2></div>
            <p className="apx-section-intro">
              A visitor should not need a technical background, a symbol key,
              or inside knowledge to understand the first screen of any page.
            </p>
          </div>
          <div className="apx-principle-grid">
            <div className="apx-principle"><strong>Plain words first</strong><span>The purpose of a page comes before specialist language.</span></div>
            <div className="apx-principle"><strong>Details stay available</strong><span>Technical readers can still reach code, specifications, and exact terms.</span></div>
            <div className="apx-principle"><strong>Symbols have a key</strong><span>Notation is defined before it is used, or it is removed.</span></div>
            <div className="apx-principle"><strong>No forced output</strong><span>Making or sharing content is voluntary, never an expectation.</span></div>
          </div>
        </section>

        <section className="apx-section" aria-labelledby="elsewhere-title">
          <div className="apx-interface">
            <div id="elsewhere" className="apx-interface-panel">
              <p className="apx-kicker">Elsewhere</p>
              <h3 id="elsewhere-title">Writing and code</h3>
              <p>Follow what I publish or browse the public code repositories.</p>
              <div className="apx-actions">
                <a href="https://medium.com/@noneisone.oneisall" target="_blank" rel="noopener noreferrer" className="apx-button apx-button--primary">Read on Medium</a>
                <a href="https://github.com/Apocky" target="_blank" rel="noopener noreferrer" className="apx-button">Visit GitHub</a>
              </div>
            </div>
            <div id="support" className="apx-interface-panel">
              <p className="apx-kicker">Optional support</p>
              <h3>Help fund the work</h3>
              <p>
                Ko-fi and Patreon are available if you want to contribute.
                Support is appreciated, never required.
              </p>
              <div className="apx-actions">
                <a href="https://ko-fi.com/oneinfinity" target="_blank" rel="noopener noreferrer" className="apx-button apx-button--primary">Support on Ko-fi</a>
                <a href="https://www.patreon.com/0ne1nfinity" target="_blank" rel="noopener noreferrer" className="apx-button">Support on Patreon</a>
              </div>
            </div>
          </div>
        </section>
      </main>
    </>
  );
};

export default Home;
