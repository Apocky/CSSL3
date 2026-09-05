import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';

import { useSiteSession } from '../components/hub/SiteSession';
import HomeVisual from '../components/home/HomeVisual';
import sections from '../components/home/HomeSections.module.css';
import { consumeAuthCallbackFromLocation, readAuthCallbackParams } from '../lib/auth-callback';
import { normalizeAuthReturnPath } from '../lib/auth-return';
import { apocryphaRelease } from '../lib/brain/release-manifest';
import { SUPPORT_LINKS } from '../lib/support-links';

type Gateway = {
  readonly eyebrow: string;
  readonly title: string;
  readonly copy: string;
  readonly action: string;
  readonly href: string;
  readonly external?: boolean;
  readonly primary?: boolean;
};

const MORE_PATHS = [
  { href: '/spellcraft', title: 'Create', copy: 'Spellcraft, sigils, and a device-local spellbook.' },
  { href: '/akashic-records', title: 'Read', copy: 'Approved writing and public-safe conversations.' },
  { href: '/codex-apockalypsis', title: 'Codex Apockalypsis', copy: 'Dark fantasy, dark comedy, and the Omnoid. Read the evolving Good Book and its companion references.' },
  { href: '/clearing', title: 'Gather', copy: 'The public social room for messages and threads.' },
  { href: '/quests', title: 'Play', copy: 'A private-on-device path through the public worlds.' },
] as const;

function GatewayContents({ gateway }: { gateway: Gateway }): JSX.Element {
  return (
    <>
      <span className="apx-home-gateway-eyebrow">{gateway.eyebrow}</span>
      <span className="apx-home-gateway-copy"><strong>{gateway.title}</strong><small>{gateway.copy}</small></span>
      <span className="apx-home-gateway-action">{gateway.action} <span aria-hidden="true">{gateway.external ? '↗' : '→'}</span></span>
    </>
  );
}

const Home: NextPage = () => {
  const [authNotice, setAuthNotice] = useState<string | null>(null);
  const { authenticated, refresh } = useSiteSession();
  const koFi = SUPPORT_LINKS.find((link) => link.name === 'Ko-fi');
  const release = apocryphaRelease.status === 'live' ? apocryphaRelease.manifest : null;

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

  const apocryphaGateway: Gateway = {
      eyebrow: authenticated ? 'YOUR ACCOUNT · CONVERSATIONS' : 'APOCRYPHA · SIGN IN TO CHAT',
      title: 'Talk to Apocrypha',
      copy: 'Ask a question, explore an idea, and return to your own conversations. Use Apocrypha here or get the mobile app.',
      action: 'Open Apocrypha',
      href: '/apocrypha',
      primary: true,
    };

  const gateways: readonly Gateway[] = [
    apocryphaGateway,
    {
      eyebrow: 'PUBLIC · MULTIDIMENSIONAL',
      title: 'Explore Atlas',
      copy: 'Move through every project as a visual map, searchable index, matrix, or plain-language dictionary.',
      action: 'Open the Atlas',
      href: '/atlas',
    },
    {
      eyebrow: 'LIVE · EXTERNAL',
      title: 'Enter Chaos Tarot',
      copy: 'Ask for a free reading, use focused divination tools, or explore the physical Chaos Tarot deck.',
      action: 'Begin a free reading',
      href: 'https://chaos-tarot.com/free-reading?source=apocky-home',
      external: true,
    },
  ];

  const structuredData = {
    '@context': 'https://schema.org',
    '@graph': [
      {
        '@type': 'WebSite',
        '@id': 'https://www.apocky.com/#website',
        name: 'Apocky',
        url: 'https://www.apocky.com/',
        description: 'Shawn Apocky’s living system for conversation, symbolic tools, public memory, games, and interconnected worlds.',
        creator: { '@id': 'https://www.apocky.com/#shawn-apocky' },
        potentialAction: {
          '@type': 'SearchAction',
          target: 'https://www.apocky.com/atlas?q={search_term_string}',
          'query-input': 'required name=search_term_string',
        },
      },
      {
        '@type': 'Person',
        '@id': 'https://www.apocky.com/#shawn-apocky',
        name: 'Shawn Apocky',
        url: 'https://www.apocky.com/',
      },
    ],
  };

  return (
    <>
      <Head>
        <title>Apocky · Conversation, symbolic tools, and connected worlds</title>
        <meta name="description" content="Meet Apocrypha, explore the multidimensional Atlas, read Shawn Apocky’s public archive, create symbolic tools, and enter Chaos Tarot." />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#000000" />
        <meta property="og:title" content="Apocky · Strange questions, useful answers" />
        <meta property="og:description" content="A living system for conversation, symbolic practice, public memory, games, and interconnected worlds." />
        <meta property="og:type" content="website" />
        <meta property="og:url" content="https://www.apocky.com/" />
        <meta property="og:site_name" content="Apocky" />
        <meta name="twitter:card" content="summary_large_image" />
        <link rel="canonical" href="https://www.apocky.com/" />
        <link rel="alternate" type="text/plain" href="/llms.txt" title="Apocky for language models and digital intelligences" />
        <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }} />
      </Head>

      <main className="apx-home apx-home-landing">
        <section className="apx-home-stage" aria-labelledby="hero-title">
          <div className="apx-home-intro">
            <div className={sections.lockup}>
              <HomeVisual variant="compact" />
              <p className={`apx-home-identity ${sections.identity}`}>SHAWN APOCKY · A LIVING CREATIVE SYSTEM</p>
            </div>
            <h1 id="hero-title">Strange questions.<br /><span className="apx-gradient-word">Useful ways through.</span></h1>
            <p className="apx-home-value">
              Apocky connects conversation, symbolic tools, public memory, games, and cosmology without asking you to understand the whole machine first.
            </p>
            <p className="apx-home-invitation">Bring one question. Choose where it should open.</p>
            <p className="apx-auth-message" aria-live="polite" hidden={!authNotice}>{authNotice}</p>
          </div>

          <HomeVisual />

          <nav className="apx-home-gateways" aria-label="Three ways into Apocky">
            {gateways.map((gateway) => {
              const className = `apx-home-gateway${gateway.primary ? ' apx-home-gateway--primary' : ''}${gateway.external ? ' apx-home-gateway--chaos' : ''}`;
              return gateway.external ? (
                <a key={gateway.title} className={className} href={gateway.href} target="_blank" rel="noopener noreferrer">
                  <GatewayContents gateway={gateway} />
                </a>
              ) : (
                <Link key={gateway.title} className={className} href={gateway.href}>
                  <GatewayContents gateway={gateway} />
                </Link>
              );
            })}
          </nav>
        </section>

        <section className="apx-home-evidence" aria-label="Current public and release evidence">
          <div><span>SITE</span><strong>Public hub available</strong></div>
          <div><span>BUILD</span><strong>{release?.release_label ?? 'Evidence unavailable'}</strong></div>
          <div><span>VERSION</span><strong>{release?.version ?? 'Unverified'}</strong></div>
          <nav aria-label="Evidence links">
            <Link className={sections.textLink} href="/status">Current status</Link>
            <a className={sections.textLink} href="/releases/apocrypha-living/manifest.json">Build manifest</a>
          </nav>
        </section>

        <details className="apx-home-more">
          <summary>
            <span><strong>More of Apocky</strong><small>Creation, writing, community, play, and support—when you want them.</small></span>
            <i aria-hidden="true">+</i>
          </summary>
          <div className="apx-home-more-body">
            <nav className="apx-home-more-paths" aria-label="More Apocky experiences">
              {MORE_PATHS.map((path) => (
                <Link key={path.href} href={path.href}><strong>{path.title}</strong><small>{path.copy}</small><span aria-hidden="true">→</span></Link>
              ))}
            </nav>
            <aside className="apx-home-support" aria-labelledby="home-support-title">
              <div><p>OPTIONAL SUPPORT</p><h2 id="home-support-title">Help the work keep growing.</h2></div>
              <p>If Apocky gives you something useful and you want more of it, membership or a direct contribution funds the next careful release.</p>
              <div>
                <Link className={sections.textLink} href="/membership">See support options →</Link>
                {koFi ? <a className={sections.textLink} href={koFi.href} target="_blank" rel="noopener noreferrer">Contribute on Ko-fi ↗</a> : null}
              </div>
            </aside>
          </div>
        </details>
      </main>
    </>
  );
};

export default Home;
