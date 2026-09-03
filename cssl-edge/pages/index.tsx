import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';

import { useSiteSession } from '../components/hub/SiteSession';
import { consumeAuthCallbackFromLocation, readAuthCallbackParams } from '../lib/auth-callback';
import { normalizeAuthReturnPath } from '../lib/auth-return';
import { SUPPORT_LINKS } from '../lib/support-links';

const PATHS = [
  {
    kind: 'Experience',
    title: 'Chaos Tarot',
    copy: 'Start with a free interactive reading, then move into eight divination systems, study tools, journals, patterns, and progression.',
    href: 'https://chaos-tarot.com/free-reading?source=apocky-home',
    label: 'Draw your first cards',
    external: true,
    tone: 'apx-door-card--neon',
  },
  {
    kind: 'Orientation',
    title: 'Constellation Atlas',
    copy: 'Navigate the projects and ideas as a visual map, multidimensional index, and living dictionary.',
    href: '/atlas',
    label: 'Map the constellation',
    external: false,
    tone: 'apx-door-card--indigo',
  },
  {
    kind: 'Participation',
    title: 'Public quests',
    copy: 'Turn passive browsing into an eleven-node expedition. Progress stays on your device and no account is required.',
    href: '/quests',
    label: 'Choose a quest',
    external: false,
    tone: 'apx-door-card--violet',
  },
  {
    kind: 'Creation',
    title: 'Symbolic Studio',
    copy: 'Ask a fast Yes / No Oracle, compile an owner-authorized Haloic-derived working, craft its sigil, and keep it in a private local Spellbook.',
    href: '/spellcraft',
    label: 'Open the workbench',
    external: false,
    tone: 'apx-door-card--indigo',
  },
] as const;

const CREATIVE_WORK = [
  {
    title: 'Yes / No Oracle',
    copy: 'A private one-question signal with a reproducible receipt, a counterweight, and no claim of decision authority.',
    href: '/oracle',
    label: 'Ask one question',
  },
  {
    title: 'Spellcraft and Sigils',
    copy: 'A deterministic symbolic language engine, visible SVG generator, and explicit device-local Spellbook.',
    href: '/spellcraft',
    label: 'Compose a working',
  },
  {
    title: 'Omnoid Singularity',
    copy: 'A source-typed cosmology of recursive totality, distinct centers, freedom, True Neutral, singularity, and return.',
    href: '/omnoid-singularity',
    label: 'Enter the cosmology',
  },
  {
    title: 'Akashic Records',
    copy: 'A searchable, hash-sealed public archive of approved writing and public-safe conversation records.',
    href: '/akashic-records',
    label: 'Search the archive',
  },
  {
    title: 'The Clearing',
    copy: 'A public community room. Reading is open; an account is required only when you choose to post or react.',
    href: '/clearing',
    label: 'Join the signal',
  },
  {
    title: 'Labyrinth of Apocalypse',
    copy: 'A game world shaped by procedural history, persistent consequence, strange systems, and discovery.',
    href: '/download',
    label: 'Inspect the test build',
  },
  {
    title: 'CSSL and CSLv3',
    copy: 'Software and notation for expressing relationships, evidence, uncertainty, decisions, and interconnected systems.',
    href: '/docs/cssl-language',
    label: 'Read the language guide',
  },
  {
    title: 'Words and symbols',
    copy: 'A plain-language dictionary for the recurring concepts, evidence marks, and relational notation used across Apocky.',
    href: '/words',
    label: 'Decode the vocabulary',
  },
] as const;

const Home: NextPage = () => {
  const [authNotice, setAuthNotice] = useState<string | null>(null);
  const { authenticated, refresh } = useSiteSession();
  const koFi = SUPPORT_LINKS.find((link) => link.name === 'Ko-fi');

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const callbackParams = readAuthCallbackParams(location.search, location.hash);
      if (!callbackParams.hasCallback) return;
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
      }
    })();
    return () => { cancelled = true; };
  }, [refresh]);

  const structuredData = {
    '@context': 'https://schema.org',
    '@type': 'WebSite',
    name: 'Apocky',
    url: 'https://www.apocky.com/',
    description: 'An interconnected constellation of symbolic tools, games, software, cosmology, public knowledge, and community by Shawn Apocky.',
    potentialAction: {
      '@type': 'SearchAction',
      target: 'https://www.apocky.com/atlas?q={search_term_string}',
      'query-input': 'required name=search_term_string',
    },
  };

  return (
    <>
      <Head>
        <title>Apocky · Interconnected worlds, tools, and living ideas</title>
        <meta name="description" content="Explore Apocky’s interconnected worlds, symbolic systems, public archive, Omnoid cosmology, community, and Chaos Tarot divination tools." />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#000000" />
        <meta property="og:title" content="Apocky · Follow the signal" />
        <meta property="og:description" content="A living constellation of divination, cosmology, games, language, software, and public knowledge." />
        <meta property="og:type" content="website" />
        <meta property="og:url" content="https://www.apocky.com/" />
        <meta property="og:site_name" content="Apocky" />
        <meta name="twitter:card" content="summary_large_image" />
        <link rel="canonical" href="https://www.apocky.com/" />
        <link rel="alternate" type="text/plain" href="/llms.txt" title="Apocky for language models and digital intelligences" />
        <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }} />
      </Head>

      <main className="apx-home">
        <section className="apx-hero" aria-labelledby="hero-title">
          <div className="apx-hero-content">
            <p className="apx-eyebrow">A living constellation · Shawn Apocky</p>
            <h1 id="hero-title">Follow the signal. <span className="apx-gradient-word">Enter the system.</span></h1>
            <p className="apx-hero-copy">
              Divination, cosmology, games, language, software, and a public memory—built as distinct worlds
              that can finally speak to one another. Start with an experience; trace the connections when curiosity catches.
            </p>
            <div className="apx-actions">
              <a href="https://chaos-tarot.com/free-reading?source=apocky-home" target="_blank" rel="noopener noreferrer" className="apx-button apx-button--primary">
                Begin a free reading <span aria-hidden="true">↗</span>
              </a>
              <Link href="/atlas" className="apx-button">Explore the Atlas</Link>
              <Link href="/oracle" className="apx-button">Ask Yes / No</Link>
              <Link href="/start" className="apx-button">Choose your path</Link>
              {authenticated ? <Link href="/account" className="apx-button">Your account</Link> : null}
            </div>
            <p className="apx-auth-message" aria-live="polite" hidden={!authNotice}>{authNotice}</p>
            <div className="apx-signal-proof" aria-label="What is available">
              <span><strong>PUBLIC</strong> archive + map</span>
              <span><strong>LIVE</strong> Chaos Tarot</span>
              <span><strong>OPEN</strong> Clearing</span>
              <span><strong>NEW</strong> Spellcraft + Sigils</span>
            </div>
          </div>

          <div className="apx-neural-card" aria-label="Choose a live signal">
            <div className="apx-neural-field" aria-hidden="true">
              <svg viewBox="0 0 420 260">
                <defs>
                  <linearGradient id="home-signal" x1="0" x2="1">
                    <stop offset="0" stopColor="#78e7ff" />
                    <stop offset="0.54" stopColor="#7287ff" />
                    <stop offset="1" stopColor="#c28cff" />
                  </linearGradient>
                  <filter id="home-glow"><feGaussianBlur stdDeviation="4" result="blur" /><feMerge><feMergeNode in="blur" /><feMergeNode in="SourceGraphic" /></feMerge></filter>
                </defs>
                <g fill="none" stroke="url(#home-signal)" opacity=".56">
                  <path d="M55 75L177 127L345 58M177 127L316 205M177 127L79 213M345 58L316 205M55 75L79 213" />
                  <circle cx="177" cy="127" r="63" opacity=".22" />
                  <circle cx="177" cy="127" r="35" opacity=".34" />
                </g>
                {[[55, 75], [345, 58], [316, 205], [79, 213], [177, 127]].map(([x, y], index) => (
                  <g key={`${x}-${y}`} filter="url(#home-glow)"><circle cx={x} cy={y} r={index === 4 ? 12 : 7} fill={index === 4 ? '#78e7ff' : '#978dff'} /></g>
                ))}
              </svg>
            </div>
            <div className="apx-neural-copy">
              <p className="apx-presence-label">Strongest live signal</p>
              <h2>Chaos Tarot</h2>
              <p>Do not settle for a brochure about the work. Use the system that is already alive.</p>
            </div>
            <div className="apx-neural-actions">
              <a href="https://chaos-tarot.com/free-reading?source=apocky-home-card" target="_blank" rel="noopener noreferrer">Try it free <span aria-hidden="true">↗</span></a>
              <a href="https://chaos-tarot.com/pricing?source=apocky-home-card" target="_blank" rel="noopener noreferrer">Unlock the Oracle <span aria-hidden="true">↗</span></a>
            </div>
          </div>
        </section>

        <section id="pathways" className="apx-section apx-section--compact" aria-labelledby="pathways-title">
          <div className="apx-section-head">
            <div><p className="apx-kicker">Choose your mode</p><h2 id="pathways-title">Experience. Understand. Participate.</h2></div>
            <p className="apx-section-intro">Every doorway is useful by itself. The neural layer reveals what each one connects to next.</p>
          </div>
          <div className="apx-door-grid">
            {PATHS.map((path) => {
              const body = (
                <>
                  <div className="apx-door-card-top"><span className="apx-door-kind">{path.kind}</span><span className="apx-door-node" aria-hidden="true"><i /></span></div>
                  <div><h3>{path.title}</h3><p>{path.copy}</p></div>
                  <span className="apx-door-link">{path.label} <span aria-hidden="true">{path.external ? '↗' : '→'}</span></span>
                </>
              );
              return path.external ? (
                <a key={path.title} href={path.href} target="_blank" rel="noopener noreferrer" className={`apx-door-card ${path.tone}`}>{body}</a>
              ) : (
                <Link key={path.title} href={path.href} className={`apx-door-card ${path.tone}`}>{body}</Link>
              );
            })}
          </div>
        </section>

        <section id="projects" className="apx-section" aria-labelledby="projects-title">
          <div className="apx-section-head">
            <div><p className="apx-kicker">The connected corpus</p><h2 id="projects-title">Worlds with their own gravity.</h2></div>
            <p className="apx-section-intro">Nothing here is filler made to catch a keyword. Each destination contains an authored project, usable tool, public record, shared space, or documented language.</p>
          </div>
          <div className="apx-project-list">
            {CREATIVE_WORK.map((project) => (
              <Link key={project.title} href={project.href} className="apx-project-link">
                <span><strong>{project.title}</strong><small>{project.copy}</small></span>
                <span className="apx-project-action">{project.label} <span aria-hidden="true">→</span></span>
              </Link>
            ))}
          </div>
          <div className="apx-section-endcap"><Link href="/atlas" className="apx-button">See every mapped connection →</Link></div>
        </section>

        <section id="support" className="apx-section apx-section--support" aria-labelledby="support-title">
          <div className="apx-support-band">
            <div>
              <p className="apx-kicker">Become a sustaining node</p>
              <h2 id="support-title">If this deserves to exist, help it compound.</h2>
              <p className="apx-support-copy">Years of uncommon work are already here. Patreon, Ko-fi, and paid Chaos Tarot access turn appreciation into the focused time required to connect, refine, and release more of it.</p>
              <Link href="/membership" className="apx-support-details">Compare the live paths <span aria-hidden="true">→</span></Link>
            </div>
            <div className="apx-support-options" aria-label="Ways to sustain Apocky">
              <a className="apx-support-option apx-support-option--featured" href="https://chaos-tarot.com/pricing?source=apocky-home" target="_blank" rel="noopener noreferrer">
                <span><strong>Unlock Chaos Tarot</strong><small>Support a living product while gaining access to its current paid experience.</small></span>
                <span className="apx-support-option-action">See plans ↗</span>
              </a>
              {koFi ? (
                <a className="apx-support-option" href={koFi.href} target="_blank" rel="noopener noreferrer">
                  <span><strong>Fuel the next release</strong><small>{koFi.description}</small></span>
                  <span className="apx-support-option-action">Open Ko-fi ↗</span>
                </a>
              ) : null}
            </div>
          </div>
        </section>
      </main>
    </>
  );
};

export default Home;
