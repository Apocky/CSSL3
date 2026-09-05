import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

import ShowcaseVideo from '../components/showcase/ShowcaseVideo';
import styles from '../styles/Showcase.module.css';

const TRANSCRIPT = [
  ['00:00–00:07.5', 'APOCKY. A living atlas of tools, symbols, and interconnected ideas. Explore www.apocky.com.'],
  ['00:07.5–00:15', 'CHAOS TAROT. Your question. Your draw. Your reflection. Try a free reading at chaos-tarot.com/free-reading.'],
  ['00:15–00:23', 'TWO DOORS. ONE CONSTELLATION. Explore. Draw. Decide what resonates. www.apocky.com and chaos-tarot.com/free-reading.'],
] as const;

const Showcase: NextPage = () => {
  const structuredData = {
    '@context': 'https://schema.org',
    '@type': 'VideoObject',
    name: 'Two Doors. One Constellation — Apocky + Chaos Tarot',
    description: 'A 23-second illustrated introduction to the Apocky Atlas and the free Chaos Tarot reading experience.',
    thumbnailUrl: [
      'https://www.apocky.com/showcase/promo-apocky-chaos-landscape-cover-v1.png',
      'https://www.apocky.com/showcase/promo-apocky-chaos-vertical-cover-v1.png',
    ],
    uploadDate: '2026-09-03',
    duration: 'PT23S',
    contentUrl: 'https://www.apocky.com/showcase/promo-apocky-chaos-landscape-23s-v1.mp4',
    embedUrl: 'https://www.apocky.com/showcase',
    inLanguage: 'en',
    isFamilyFriendly: true,
    transcript: TRANSCRIPT.map(([time, copy]) => `${time}: ${copy}`).join(' '),
    potentialAction: {
      '@type': 'WatchAction',
      target: 'https://www.apocky.com/showcase',
    },
  };

  return (
    <>
      <Head>
        <title>Apocky + Chaos Tarot Showcase · Two Doors, One Constellation</title>
        <meta
          name="description"
          content="Watch the 23-second Apocky and Chaos Tarot showcase, then explore the interactive Atlas, ask the Yes / No Oracle, or begin a free reading."
        />
        <meta name="keywords" content="Apocky showcase, Chaos Tarot, tarot video, interactive Atlas, yes no oracle, divination tools" />
        <meta name="robots" content="index,follow,max-image-preview:large,max-video-preview:-1" />
        <meta name="theme-color" content="#000000" />
        <meta property="og:type" content="video.other" />
        <meta property="og:site_name" content="Apocky" />
        <meta property="og:title" content="Two Doors. One Constellation." />
        <meta property="og:description" content="A 23-second passage between Apocky and Chaos Tarot." />
        <meta property="og:url" content="https://www.apocky.com/showcase" />
        <meta property="og:image" content="https://www.apocky.com/showcase/promo-apocky-chaos-landscape-cover-v1.png" />
        <meta property="og:image:width" content="1920" />
        <meta property="og:image:height" content="1080" />
        <meta property="og:image:alt" content="Illustrated indigo and violet doorway connecting Apocky and Chaos Tarot" />
        <meta property="og:video" content="https://www.apocky.com/showcase/promo-apocky-chaos-landscape-23s-v1.mp4" />
        <meta property="og:video:secure_url" content="https://www.apocky.com/showcase/promo-apocky-chaos-landscape-23s-v1.mp4" />
        <meta property="og:video:type" content="video/mp4" />
        <meta property="og:video:width" content="1920" />
        <meta property="og:video:height" content="1080" />
        <meta name="twitter:card" content="summary_large_image" />
        <meta name="twitter:title" content="Two Doors. One Constellation." />
        <meta name="twitter:description" content="Watch the 23-second Apocky + Chaos Tarot showcase." />
        <meta name="twitter:image" content="https://www.apocky.com/showcase/promo-apocky-chaos-landscape-cover-v1.png" />
        <meta name="twitter:image:alt" content="Illustrated indigo and violet doorway connecting Apocky and Chaos Tarot" />
        <link rel="canonical" href="https://www.apocky.com/showcase" />
        <link rel="preload" as="image" href="/showcase/promo-apocky-chaos-landscape-cover-v1.png" />
        <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }} />
      </Head>

      <main className={styles.page}>
        <div className={styles.wrap}>
          <header className={styles.hero}>
            <div>
              <p className={styles.eyebrow}>Apocky + Chaos Tarot · 23 seconds</p>
              <h1 className={styles.title}>Two doors. <em>One constellation.</em></h1>
            </div>
            <div>
              <p className={styles.lead}>
                Discover tools, writing, and symbolic readings in this short introduction.
                Watch, then choose something to try.
              </p>
              <div className={styles.actions}>
                <Link className={styles.primary} href="/atlas">Find a tool or idea →</Link>
                <a className={styles.secondary} href="https://chaos-tarot.com/free-reading?source=apocky-showcase" target="_blank" rel="noopener noreferrer">Begin a free reading ↗</a>
              </div>
            </div>
          </header>

          <ShowcaseVideo />

          <section className={styles.section} aria-labelledby="art-boundary-title">
            <div className={styles.truthPanel} id="showcase-art-disclosure">
              <strong className={styles.truthLabel} id="art-boundary-title">About the artwork</strong>
              <p>
                <strong>Illustrative concept art, not product photography.</strong> The cards, spaces, and doorway
                visualize a relationship between two live sites; they do not depict a physical deck or promise a
                feature. Follow the links below for current public functionality and availability.
              </p>
            </div>
          </section>

          <section className={styles.section} aria-labelledby="choose-title">
            <div className={styles.sectionHeader}>
              <h2 id="choose-title">What would you like to try?</h2>
              <p>Read, make something, or explore a question.</p>
            </div>
            <div className={styles.cardGrid}>
              <article className={styles.card}>
                <span className={styles.cardTag}>MAP</span>
                <h3>Explore the Atlas</h3>
                <p>Find tools, stories, and ideas by what you want to do.</p>
                <Link className={styles.cardLink} href="/atlas">Browse the Atlas →</Link>
              </article>
              <article className={styles.card}>
                <span className={styles.cardTag}>ASK</span>
                <h3>Explore a Yes / No question</h3>
                <p>A symbolic reading on Chaos Tarot. This currently requires a separate sign-in.</p>
                <a className={styles.cardLink} href="https://chaos-tarot.com/yes-no" target="_blank" rel="noopener noreferrer">Open Chaos Tarot ↗</a>
              </article>
              <article className={styles.card}>
                <span className={styles.cardTag}>DRAW</span>
                <h3>Begin free on Chaos Tarot</h3>
                <p>Open the independent reading experience without an Apocky account or payment gate.</p>
                <a className={styles.cardLink} href="https://chaos-tarot.com/free-reading?source=apocky-showcase-card" target="_blank" rel="noopener noreferrer">Begin a free reading ↗</a>
              </article>
              <article className={styles.card}>
                <span className={styles.cardTag}>SUSTAIN</span>
                <h3>Keep the work alive</h3>
                <p>Compare the currently active support routes and see what remains free before choosing anything.</p>
                <Link className={styles.cardLink} href="/membership">See membership and support →</Link>
              </article>
            </div>
          </section>

          <section className={styles.section} aria-labelledby="transcript-title">
            <div className={styles.sectionHeader}>
              <h2 id="transcript-title">Read instead of watch.</h2>
              <p>The complete on-screen copy remains available without playing media.</p>
            </div>
            <details className={styles.transcript}>
              <summary>Open the 23-second transcript</summary>
              <div className={styles.transcriptText}>
                {TRANSCRIPT.map(([time, copy]) => <p key={time}><strong>{time}</strong><br />{copy}</p>)}
                <p>Audio: instrumental ambient synth only; no speech or voice-over.</p>
                <a className={styles.downloadLink} href="/showcase/promo-apocky-chaos-23s-transcript-v1.txt" download>Download the plain-text transcript</a>
                {' · '}
                <a className={styles.downloadLink} href="/showcase/promo-apocky-chaos-23s-en-v1.srt" download>Download SRT captions</a>
              </div>
            </details>
          </section>
        </div>
      </main>
    </>
  );
};

export default Showcase;
