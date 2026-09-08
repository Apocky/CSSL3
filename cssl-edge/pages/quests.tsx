import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useMemo, useState } from 'react';

import styles from '../styles/NeuralPages.module.css';

const STORAGE_KEY = 'apocky.public-quests.v1';

const QUESTS = [
  {
    id: 'read-signal',
    rank: '01 · Reflect',
    title: 'Take a moment to reflect',
    copy: 'Try a free Chaos Tarot reading. Notice which part gives you something to think about.',
    href: 'https://chaos-tarot.com/free-reading?source=apocky-quests',
    action: 'Begin a reading',
    external: true,
  },
  {
    id: 'map-constellation',
    rank: '02 · Explore',
    title: 'Find your next stop',
    copy: 'Search the directory and find three tools, stories, or ideas you want to try.',
    href: '/atlas',
    action: 'Browse the directory',
    external: false,
  },
  {
    id: 'open-record',
    rank: '03 · Read',
    title: 'Read something through',
    copy: 'Choose a published thought or story and read it all the way through. Keep one idea that stays with you.',
    href: '/akashic-records',
    action: 'Find something to read',
    external: false,
  },
  {
    id: 'learn-language',
    rank: '04 · Learn',
    title: 'Find a word you can use',
    copy: 'Look up a word, read its example, and try using it in a sentence of your own.',
    href: '/words',
    action: 'Find a definition',
    external: false,
  },
  {
    id: 'enter-omnoid',
    rank: '05 · Imagine',
    title: 'Explore an unfamiliar idea',
    copy: 'Read about the Omnoid, Shawn’s evolving cosmology. Find an idea you want to question or explore.',
    href: '/omnoid-singularity',
    action: 'Read the cosmology',
    external: false,
  },
  {
    id: 'scout-labyrinth',
    rank: '06 · Play',
    title: 'Try the Labyrinth',
    copy: 'See what the game offers and which devices it supports before you decide to download it.',
    href: '/download',
    action: 'See the game',
    external: false,
  },
  {
    id: 'join-clearing',
    rank: '07 · Connect',
    title: 'Visit the community',
    copy: 'Read a conversation in the Clearing. Sign in if you want to join in.',
    href: '/clearing',
    action: 'Visit the room',
    external: false,
  },
  {
    id: 'sustain-system',
    rank: '08 · Support',
    title: 'Choose what you want to support',
    copy: 'Look at the ways to support the work. Deciding to keep reading is a valid choice too; this activity never requires payment.',
    href: '/membership',
    action: 'See support options',
    external: false,
  },
  {
    id: 'ask-oracle',
    rank: '09 · Ask',
    title: 'Ask a question, notice your reaction',
    copy: 'Sign in on Chaos Tarot for a yes-or-no reading. Treat the answer as a prompt and notice what you think about it.',
    href: 'https://chaos-tarot.com/yes-no?source=apocky-quests',
    action: 'Open Oracle · sign-in required',
    external: true,
  },
  {
    id: 'compose-working',
    rank: '10 · Make',
    title: 'Make a spell of your own',
    copy: 'Choose something to focus on, create a reflection, and save the words you want to keep.',
    href: '/spellcraft',
    action: 'Create a spell',
    external: false,
  },
  {
    id: 'craft-sigil',
    rank: '11 · Draw',
    title: 'Make a mark that matters to you',
    copy: 'Choose a meaning, make a sigil, and try a few shapes. Download the one you like.',
    href: '/sigils',
    action: 'Make a sigil',
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
        <title>Things to try · Apocky</title>
        <meta name="description" content="Small activities for making, thinking, reading, and connecting on Apocky." />
        <meta property="og:title" content="Things to try · Apocky" />
        <meta property="og:description" content="Choose something to make, read, or explore. Keep your progress in this browser." />
        <meta property="og:url" content="https://www.apocky.com/quests" />
        <link rel="canonical" href="https://www.apocky.com/quests" />
      </Head>

      <main className={styles.page}>
        <div className={styles.wrap}>
          <p className={styles.eyebrow}>Things to try</p>
          <h1 className={styles.title}>Choose a small adventure.</h1>
          <p className={styles.lead}>
            Make something, read an idea, or meet the community. Pick any activity and mark it complete when it feels done. Your progress stays in this browser.
          </p>

          <section className={styles.progressPanel} aria-label="Quest progress" aria-live="polite">
            <div className={styles.progressRing} style={{ '--quest-progress': `${percentage}%` } as React.CSSProperties}>
              <span>{percentage}%</span>
            </div>
            <div>
              <h2>{completed.size} of {QUESTS.length} activities complete</h2>
              <p>{completed.size === QUESTS.length ? 'You’ve tried them all. Revisit a favorite whenever you like.' : 'Mark each activity when you decide you have completed it.'}</p>
            </div>
            <button className={styles.reset} type="button" onClick={reset} disabled={completed.size === 0}>Reset progress</button>
          </section>

          <section className={styles.section} aria-labelledby="quest-list-title">
            <div className={styles.sectionHead}>
              <h2 id="quest-list-title">Pick something to try.</h2>
              <p>Start anywhere. There is no required order.</p>
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
            <strong>Your progress.</strong>
            <span>
              Completed activities are saved only in this browser. Clearing site data removes them.
            </span>
          </p>
        </div>
      </main>
    </>
  );
};

export default Quests;
