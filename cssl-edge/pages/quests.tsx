import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useMemo, useState } from 'react';

import styles from '../styles/NeuralPages.module.css';

const STORAGE_KEY = 'apocky.public-quests.v1';

const QUESTS = [
  {
    id: 'read-signal',
    rank: '01 · EXPERIENCE',
    title: 'Read a signal',
    copy: 'Begin a free Chaos Tarot reading and see how the active divination system feels in use.',
    href: 'https://chaos-tarot.com/free-reading?source=apocky-quests',
    action: 'Begin a reading',
    external: true,
  },
  {
    id: 'map-constellation',
    rank: '02 · ORIENTATION',
    title: 'Map the constellation',
    copy: 'Use the Atlas map, index, and dictionary to find three connected parts of Apocky.',
    href: '/atlas',
    action: 'Open the Atlas',
    external: false,
  },
  {
    id: 'open-record',
    rank: '03 · ARCHIVE',
    title: 'Open a record',
    copy: 'Search the approved public archive and read one source all the way through.',
    href: '/akashic-records',
    action: 'Search the records',
    external: false,
  },
  {
    id: 'learn-language',
    rank: '04 · LANGUAGE',
    title: 'Learn a symbol',
    copy: 'Choose one recurring word or CSLv3 glyph and learn what it means in this system.',
    href: '/words',
    action: 'Open the dictionary',
    external: false,
  },
  {
    id: 'enter-omnoid',
    rank: '05 · COSMOLOGY',
    title: 'Enter the Omnoid',
    copy: 'Follow one claim through its authored, collaborative, mathematical, and open-hypothesis boundaries.',
    href: '/omnoid-singularity',
    action: 'Read the cosmology',
    external: false,
  },
  {
    id: 'scout-labyrinth',
    rank: '06 · GAME',
    title: 'Scout the Labyrinth',
    copy: 'Inspect the current public test build, its checksum, and its known limitations before deciding whether to download.',
    href: '/download',
    action: 'Inspect the build',
    external: false,
  },
  {
    id: 'join-clearing',
    rank: '07 · COMMUNITY',
    title: 'Enter the Clearing',
    copy: 'Read the public room. Sign in only if and when you want to add your own signal.',
    href: '/clearing',
    action: 'Visit the room',
    external: false,
  },
  {
    id: 'sustain-system',
    rank: '08 · SUSTAIN',
    title: 'Choose what should continue',
    copy: 'Review the live support paths and decide whether the work has earned your backing. Payment is never required to complete this quest.',
    href: '/membership',
    action: 'Review membership',
    external: false,
  },
  {
    id: 'ask-oracle',
    rank: '09 · DECIDE',
    title: 'Ask one clean question',
    copy: 'Use the private Yes / No Oracle, notice your reaction, and keep the generated signal in its reflective boundary.',
    href: '/oracle',
    action: 'Ask the Oracle',
    external: false,
  },
  {
    id: 'compose-working',
    rank: '10 · COMPOSE',
    title: 'Compile a symbolic working',
    copy: 'Build a valid Haloic-derived form and inspect its vocabulary, interpretation, graph, confidence, and authority-none receipt.',
    href: '/spellcraft',
    action: 'Open Spellcraft',
    external: false,
  },
  {
    id: 'craft-sigil',
    rank: '11 · CREATE',
    title: 'Craft visible geometry',
    copy: 'Turn a validated symbolic program into a deterministic sigil and download one deliberate variant.',
    href: '/sigils',
    action: 'Enter the Sigil Studio',
    external: false,
  },
] as const;

function readStoredProgress(): ReadonlySet<string> {
  if (typeof window === 'undefined') return new Set();
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? '[]');
    if (!Array.isArray(value)) return new Set();
    const validIds = new Set(QUESTS.map((quest) => quest.id));
    return new Set(value.filter((id): id is string => typeof id === 'string' && validIds.has(id as typeof QUESTS[number]['id'])));
  } catch {
    return new Set();
  }
}

const Quests: NextPage = () => {
  const [completed, setCompleted] = useState<ReadonlySet<string>>(new Set());
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    setCompleted(readStoredProgress());
    setLoaded(true);
  }, []);

  useEffect(() => {
    if (!loaded) return;
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify([...completed]));
    } catch {
      // Progress remains usable for this visit when browser storage is unavailable.
    }
  }, [completed, loaded]);

  const percentage = useMemo(() => Math.round((completed.size / QUESTS.length) * 100), [completed]);

  const toggleQuest = (id: string): void => {
    setCompleted((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const reset = (): void => {
    setCompleted(new Set());
  };

  return (
    <>
      <Head>
        <title>Public quests · Apocky</title>
        <meta name="description" content="Eleven self-directed quests through Apocky’s tools, archive, cosmology, community, symbolic studio, game, and Chaos Tarot." />
        <meta property="og:title" content="Public quests · Apocky" />
        <meta property="og:description" content="Turn passive browsing into an eight-part expedition through the Apocky constellation." />
        <meta property="og:url" content="https://www.apocky.com/quests" />
        <link rel="canonical" href="https://www.apocky.com/quests" />
      </Head>

      <main className={styles.page}>
        <div className={styles.wrap}>
          <p className={styles.eyebrow}>Public expedition · device-local</p>
          <h1 className={styles.title}>Stop scrolling. <em>Take the constellation personally.</em></h1>
          <p className={styles.lead}>
            Eight small missions turn the site into a route you can finish. Progress stays in this browser;
            there is no leaderboard, account score, surveillance profile, or claim that clicking proves understanding.
          </p>

          <section className={styles.progressPanel} aria-label="Quest progress" aria-live="polite">
            <div className={styles.progressRing} style={{ '--quest-progress': `${percentage}%` } as React.CSSProperties}>
              <span>{percentage}%</span>
            </div>
            <div>
              <h2>{completed.size} of {QUESTS.length} signals completed</h2>
              <p>{completed.size === QUESTS.length ? 'Constellation traversed. Pick a path to revisit—or support what should grow next.' : 'Mark each quest when you decide you have completed it.'}</p>
            </div>
            <button className={styles.reset} type="button" onClick={reset} disabled={completed.size === 0}>Reset progress</button>
          </section>

          <section className={styles.section} aria-labelledby="quest-list-title">
            <div className={styles.sectionHead}>
              <h2 id="quest-list-title">Eleven nodes. One traversal.</h2>
              <p>Do them in order for a guided path, or start wherever the signal is strongest.</p>
            </div>
            <div className={styles.grid2}>
              {QUESTS.map((quest) => {
                const isComplete = completed.has(quest.id);
                const link = quest.external ? (
                  <a className={styles.cardLink} href={quest.href} target="_blank" rel="noopener noreferrer">
                    {quest.action} <span aria-hidden="true">↗</span>
                  </a>
                ) : (
                  <Link className={styles.cardLink} href={quest.href}>{quest.action} →</Link>
                );
                return (
                  <article className={styles.quest} data-complete={isComplete ? 'true' : 'false'} key={quest.id}>
                    <span className={styles.questMeta}>{quest.rank}</span>
                    <h3>{quest.title}</h3>
                    <p>{quest.copy}</p>
                    <div className={styles.questActions}>
                      {link}
                      <button className={styles.questButton} type="button" aria-pressed={isComplete} onClick={() => toggleQuest(quest.id)}>
                        {isComplete ? 'Mark incomplete' : 'Mark complete'}
                      </button>
                    </div>
                  </article>
                );
              })}
            </div>
          </section>

          <p className={styles.truth}>
            <strong>Local state.</strong>
            <span>
              Quest progress is stored only in this browser under <code>{STORAGE_KEY}</code>. Clearing site data removes it.
              It is not sent to Apocky, Chaos Tarot, Ko-fi, or Patreon.
            </span>
          </p>
        </div>
      </main>
    </>
  );
};

export default Quests;
