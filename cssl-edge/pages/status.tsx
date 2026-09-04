import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useCallback, useEffect, useState } from 'react';

import styles from '../styles/NeuralPages.module.css';

interface PublicHealth {
  readonly ok: boolean;
  readonly ts?: string;
  readonly version?: string;
  readonly auth_supabase_configured?: boolean;
  readonly data_supabase_configured?: boolean;
  readonly payments_ready?: boolean;
}

type ProbeState =
  | { readonly kind: 'loading'; readonly code: 'APX-STATUS-CHECKING' }
  | { readonly kind: 'ready'; readonly code: 'APX-CORE-READY'; readonly health: PublicHealth }
  | { readonly kind: 'unavailable'; readonly code: 'APX-STATUS-UNAVAILABLE'; readonly detail: string };

const Status: NextPage = () => {
  const [probe, setProbe] = useState<ProbeState>({ kind: 'loading', code: 'APX-STATUS-CHECKING' });

  const check = useCallback(async (): Promise<void> => {
    setProbe({ kind: 'loading', code: 'APX-STATUS-CHECKING' });
    const controller = new AbortController();
    const timeout = window.setTimeout(() => controller.abort(), 8000);
    try {
      const response = await fetch('/api/health', {
        method: 'GET',
        cache: 'no-store',
        headers: { Accept: 'application/json' },
        signal: controller.signal,
      });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const value: unknown = await response.json();
      if (!value || typeof value !== 'object' || (value as PublicHealth).ok !== true) {
        throw new Error('Invalid health response');
      }
      setProbe({ kind: 'ready', code: 'APX-CORE-READY', health: value as PublicHealth });
    } catch (error) {
      setProbe({
        kind: 'unavailable',
        code: 'APX-STATUS-UNAVAILABLE',
        detail: error instanceof DOMException && error.name === 'AbortError' ? 'The status check timed out.' : 'The status endpoint did not answer normally.',
      });
    } finally {
      window.clearTimeout(timeout);
    }
  }, []);

  useEffect(() => {
    void check();
  }, [check]);

  const isReady = probe.kind === 'ready';
  const authReady = isReady && Boolean(probe.health.auth_supabase_configured);
  const archiveReady = isReady && Boolean(probe.health.data_supabase_configured);
  const directPaymentsReady = isReady && Boolean(probe.health.payments_ready);

  return (
    <>
      <Head>
        <title>System status · Apocky</title>
        <meta name="description" content="A live, privacy-bounded status view for Apocky’s public core, account connection, archive data connection, and support routes." />
        <meta name="robots" content="index,follow" />
        <meta property="og:title" content="System status · Apocky" />
        <meta property="og:url" content="https://www.apocky.com/status" />
        <link rel="canonical" href="https://www.apocky.com/status" />
      </Head>

      <main className={styles.page}>
        <div className={styles.wrap}>
          <p className={styles.eyebrow}>Public observatory</p>
          <h1 className={styles.title}>A nervous system should know <em>when it cannot feel.</em></h1>
          <p className={styles.lead}>
            This page checks Apocky’s public health endpoint from your browser and translates it into stable,
            supportable states. It does not expose keys, account data, private logs, or internal diagnostics.
          </p>

          <div className={styles.actions}>
            <button className={styles.primary} type="button" onClick={() => void check()} disabled={probe.kind === 'loading'}>
              {probe.kind === 'loading' ? 'Checking…' : 'Run status check'}
            </button>
            <Link className={styles.secondary} href="/start">Use another route →</Link>
          </div>

          <div className={styles.statusGrid} aria-live="polite" aria-busy={probe.kind === 'loading'}>
            <article className={`${styles.statusItem} ${isReady ? styles.statusGood : styles.statusDegraded}`}>
              <span>Public core · {probe.code}</span>
              <strong>{probe.kind === 'loading' ? 'Checking' : isReady ? 'Operational' : 'Status unavailable'}</strong>
            </article>
            <article className={`${styles.statusItem} ${authReady ? styles.statusGood : styles.statusDegraded}`}>
              <span>Identity · {probe.kind === 'loading' ? 'APX-AUTH-CHECKING' : authReady ? 'APX-AUTH-READY' : 'APX-AUTH-DEGRADED'}</span>
              <strong>{probe.kind === 'loading' ? 'Checking' : authReady ? 'Connected' : 'Use public routes or retry'}</strong>
            </article>
            <article className={`${styles.statusItem} ${archiveReady ? styles.statusGood : styles.statusDegraded}`}>
              <span>Data plane · {probe.kind === 'loading' ? 'APX-DATA-CHECKING' : archiveReady ? 'APX-DATA-READY' : 'APX-DATA-DEGRADED'}</span>
              <strong>{probe.kind === 'loading' ? 'Checking' : archiveReady ? 'Connected' : 'Static public pages remain available'}</strong>
            </article>
            <article className={`${styles.statusItem} ${styles.statusGood}`}>
              <span>Chaos Tarot relay · CT-HANDOFF-EXTERNAL</span>
              <strong>Independent destination</strong>
            </article>
            <article className={`${styles.statusItem} ${styles.statusGood}`}>
              <span>Membership relay · APX-SUPPORT-EXTERNAL</span>
              <strong>Ko-fi and Patreon</strong>
            </article>
            <article className={`${styles.statusItem} ${directPaymentsReady ? styles.statusGood : styles.statusDegraded}`}>
              <span>Apocky direct checkout · {directPaymentsReady ? 'APX-PAYMENTS-READY' : 'APX-PAYMENTS-NOT-OFFERED'}</span>
              <strong>{directPaymentsReady ? 'Configured' : 'Use the external support routes'}</strong>
            </article>
          </div>

          <p className={styles.truth}>
            <strong>{probe.code}</strong>
            <span>
              {probe.kind === 'ready'
                ? `Observed at ${probe.health.ts ?? 'the latest response'}${probe.health.version ? ` · health API version ${probe.health.version}` : ''}. Configuration flags mean a connection is present; they do not prove every user flow succeeds.`
                : probe.kind === 'unavailable'
                  ? `${probe.detail} Refresh or use the public fallback links below.`
                  : 'Waiting for a bounded response from the same-origin health endpoint.'}
            </span>
          </p>

          <section className={styles.section} aria-labelledby="fallback-title">
            <div className={styles.sectionHead}>
              <h2 id="fallback-title">Every important path gets a fallback.</h2>
              <p>A degraded integration should narrow the route, not erase the rest of the site.</p>
            </div>
            <div className={styles.grid3}>
              <article className={styles.card}>
                <h3>Account trouble</h3>
                <p>Retry sign-in, or continue through pages that do not require an account.</p>
                <Link className={styles.cardLink} href="/login?next=%2Faccount">Retry sign-in →</Link>
              </article>
              <article className={styles.card}>
                <h3>Archive trouble</h3>
                <p>Use the Atlas, dictionary, cosmology, and project pages while the data route recovers.</p>
                <Link className={styles.cardLink} href="/atlas">Open the Atlas →</Link>
              </article>
              <article className={styles.card}>
                <h3>Need a live experience</h3>
                <p>Chaos Tarot runs as an independent destination with its own account and recovery path.</p>
                <a className={styles.cardLink} href="https://chaos-tarot.com/free-reading?source=apocky-status" target="_blank" rel="noopener noreferrer">Open Chaos Tarot ↗</a>
              </article>
            </div>
          </section>
        </div>
      </main>
    </>
  );
};

export default Status;
