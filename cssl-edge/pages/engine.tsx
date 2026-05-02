// /engine · LIVE engine-status page · public-readable · transparency-mandate
// W14-M · phone-first responsive · auto-refresh 5s · stub-mode-aware
// I> "is-engine-running · how-much-learned-this-hour" · -Apocky
// W! NO auth-gate · NO behind-login · ALL visitors see truth
// W! sovereign-pause cap-gated SERVER-side · UI shows toggle but only-functional with-cap

import type { NextPage } from 'next';
import type { CSSProperties } from 'react';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useMemo, useState } from 'react';
import HeartbeatPulse from '../components/engine/HeartbeatPulse';
import CycleCounter from '../components/engine/CycleCounter';
import LiveFeed from '../components/engine/LiveFeed';
import SovereignPause from '../components/engine/SovereignPause';
import RecentEventsTable from '../components/engine/RecentEventsTable';
import {
  useHeartbeat,
  useCycles,
  useRecentEvents,
  usePauseState,
  fmtCompact,
  fmtBytes,
  fmtRelTimeCoarse,
  emptyHeartbeat,
} from '../lib/engine-status';
import type { EventKind, EngineEvent } from '../lib/engine-status';

// ─────────────────────────────────────────────
// § Palette · matches existing apocky.com pages
// ─────────────────────────────────────────────
const C = {
  bg: '#0a0a0f',
  border: '#1f1f2a',
  cardBg: 'rgba(20, 20, 30, 0.5)',
  textDim: '#7a7a8c',
  text: '#cdd6e4',
  accentPurple: '#c084fc',
  accentBlue: '#7dd3fc',
  accentAmber: '#fbbf24',
  accentMint: '#34d399',
  accentRose: '#f87171',
  accentLavender: '#a78bfa',
};

// ─────────────────────────────────────────────
// § Helper · find last-event-ts of a given kind
// ─────────────────────────────────────────────
function lastTsOf(events: EngineEvent[], kind: EventKind | EventKind[]): number | null {
  const kinds = Array.isArray(kind) ? new Set(kind) : new Set([kind]);
  let best = 0;
  for (const e of events) {
    if (kinds.has(e.kind) && e.ts > best) best = e.ts;
  }
  return best > 0 ? best : null;
}

// ─────────────────────────────────────────────
// § Cap detection · query-string ?cap=<int> only-when-Apocky
// I> server enforces · UI honors hint
// ─────────────────────────────────────────────
function useUrlCap(): number {
  const [cap, setCap] = useState(0);
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const params = new URLSearchParams(window.location.search);
    const raw = params.get('cap');
    if (raw) {
      const n = parseInt(raw, 16);
      if (!Number.isNaN(n) && n > 0) {
        setCap(n);
        return;
      }
      const dec = parseInt(raw, 10);
      if (!Number.isNaN(dec) && dec > 0) setCap(dec);
    }
  }, []);
  return cap;
}

// ─────────────────────────────────────────────
// § Page
// ─────────────────────────────────────────────
const EnginePage: NextPage = () => {
  const cap = useUrlCap();
  const heartbeat = useHeartbeat(5000);
  const cycles = useCycles(undefined, 5000);
  const events = useRecentEvents(50, 5000);
  const pause = usePauseState(10000);

  // ⊑ now-tick · drives relative-time labels without re-fetching
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  const hb = heartbeat.data ?? emptyHeartbeat();
  const cy = cycles.data;
  const eventList = events.data?.events ?? [];

  const allStub =
    (heartbeat.data?.stub ?? false) &&
    (cycles.data?.stub ?? false) &&
    (events.data?.stub ?? false);

  const someStub =
    (heartbeat.data?.stub ?? false) ||
    (cycles.data?.stub ?? false) ||
    (events.data?.stub ?? false);

  // ⊑ derive per-counter last-event ts
  const lastSelfAuthor = useMemo(() => lastTsOf(eventList, 'self_author'), [eventList]);
  const lastPlaytest = useMemo(() => lastTsOf(eventList, 'playtest'), [eventList]);
  const lastKan = useMemo(() => lastTsOf(eventList, 'kan_rollup'), [eventList]);
  const lastMycelium = useMemo(() => lastTsOf(eventList, 'mycelium_sync'), [eventList]);
  const lastSigma = useMemo(() => lastTsOf(eventList, 'sigma_anchor'), [eventList]);
  const lastApockyOnline = useMemo(() => {
    // I> coarse · "Apocky-class" event = self_author OR sovereign_resume
    return lastTsOf(eventList, ['self_author', 'sovereign_resume', 'sovereign_pause']);
  }, [eventList]);

  // ⊑ idle-detection · no-event-in-2-min from canonical kinds
  const isIdle = useMemo(() => {
    if (allStub) return false;
    const lastAny = lastTsOf(eventList, [
      'self_author',
      'playtest',
      'kan_rollup',
      'mycelium_sync',
    ]);
    if (lastAny === null) return true;
    return now - lastAny > 120_000;
  }, [eventList, now, allStub]);

  // ⊑ bytes-per-cycle indicator ω-flavor
  const bytesPerCycle = useMemo(() => {
    if (!cy) return null;
    const totalCycles = cy.self_author + cy.playtest + cy.kan_rollup + cy.mycelium_sync;
    if (totalCycles === 0) return null;
    return cy.bytes_processed / totalCycles;
  }, [cy]);

  const totalCycles = cy
    ? cy.self_author + cy.playtest + cy.kan_rollup + cy.mycelium_sync
    : 0;

  // I> phone-first 1-col · tablet 2-col · desktop 4-col
  const counterGrid: CSSProperties = {
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fill, minmax(220px, 1fr))',
    gap: '0.75rem',
    marginBottom: '1.25rem',
  };
  const heartbeatGrid: CSSProperties = {
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
    gap: '0.75rem',
    marginBottom: '1.25rem',
  };
  const sideBySideGrid: CSSProperties = {
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
    gap: '0.75rem',
    marginBottom: '1.25rem',
  };

  return (
    <>
      <Head>
        <title>§ Engine · live status · apocky.com</title>
        <meta
          name="description"
          content="Real-time engine-status for the Apocky-mycelium · self-author cycles · playtests · KAN-rollups · mycelium-syncs · transparency-by-default"
        />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#0a0a0f" />
        <style>{`
          * { box-sizing: border-box; }
          html, body { margin: 0; padding: 0; -webkit-tap-highlight-color: transparent; }
          body {
            background: radial-gradient(ellipse at top, #15151f 0%, #0a0a0f 50%, #050507 100%);
            color: #e6e6f0;
            font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
            min-height: 100vh;
            min-height: 100dvh;
            -webkit-font-smoothing: antialiased;
            font-size: 15px;
            line-height: 1.5;
          }
          a { color: inherit; text-decoration: none; }
        `}</style>
      </Head>

      <main
        style={{
          maxWidth: 1200,
          margin: '0 auto',
          padding: '1.5rem 1rem 4rem',
          minHeight: '100dvh',
        }}
      >
        {/* ── Header ─────────────────────────────── */}
        <header style={{ marginBottom: '1.5rem' }}>
          <Link href="/" style={{ color: C.accentBlue, fontSize: '0.8rem' }}>
            ← apocky.com
          </Link>
          <h1
            style={{
              fontSize: 'clamp(1.5rem, 5vw, 1.95rem)',
              margin: '0.5rem 0 0.4rem',
              fontWeight: 700,
              backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%)',
              WebkitBackgroundClip: 'text',
              WebkitTextFillColor: 'transparent',
            }}
          >
            § Engine · live status
          </h1>
          <p style={{ color: C.textDim, fontSize: '0.82rem', margin: 0 }}>
            real-time visibility · is-engine-running · how-much-learned-this-hour · auto-refresh 5s
          </p>
          <p
            style={{
              color: '#5a5a6a',
              fontSize: '0.72rem',
              marginTop: 8,
              marginBottom: 0,
              fontStyle: 'italic',
            }}
          >
            ◇ public-readable · transparency-by-default · k-anon-respecting summaries · all engine
            actions Σ-Chain-anchored
          </p>
        </header>

        {/* ── Stub-mode banner ──────────────────── */}
        {someStub && (
          <div
            style={{
              padding: '0.85rem 1rem',
              background: 'rgba(251, 191, 36, 0.08)',
              border: '1px solid rgba(251, 191, 36, 0.4)',
              borderRadius: 6,
              marginBottom: '1.25rem',
              fontSize: '0.78rem',
              color: C.accentAmber,
            }}
          >
            <strong>⚠ partial stub-mode</strong>
            <p style={{ margin: '0.4rem 0 0', color: C.textDim }}>
              {allStub
                ? 'engine-orchestrator (W14-J/K) not yet deployed · all cards render zero-state · this is the expected baseline pre-launch'
                : 'some endpoints offline · ¬ all data-paths wired yet · cards mix live + zero-state'}
            </p>
          </div>
        )}

        {/* ── Idle indicator ─────────────────────── */}
        {isIdle && !allStub && (
          <div
            style={{
              padding: '0.65rem 0.9rem',
              background: 'rgba(167, 139, 250, 0.06)',
              border: `1px solid ${C.accentLavender}40`,
              borderRadius: 6,
              marginBottom: '1.25rem',
              fontSize: '0.78rem',
              color: C.text,
              display: 'flex',
              alignItems: 'center',
              gap: 8,
            }}
          >
            <span style={{ color: C.accentLavender, fontSize: '1rem' }}>○</span>
            <span>
              engine idle · no cycles in last 2 min · awaiting next prompt or scheduled tick
            </span>
          </div>
        )}

        {/* ── Section 1 · Heartbeats ─────────────── */}
        <section aria-labelledby="hb-h">
          <h2
            id="hb-h"
            style={{
              fontSize: '0.78rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: C.textDim,
              margin: '0 0 0.6rem',
              fontWeight: 400,
            }}
          >
            <span style={{ color: C.accentLavender, marginRight: 6 }}>♥</span>
            heartbeats
          </h2>
          <div style={heartbeatGrid}>
            <HeartbeatPulse
              label="LOCAL"
              node={hb.local}
              now={now}
              stub={hb.stub}
            />
            <HeartbeatPulse
              label="CLOUD"
              node={hb.cloud}
              now={now}
              stub={hb.stub}
            />
          </div>
        </section>

        {/* ── Section 2 · Cycle counters ──────────── */}
        <section aria-labelledby="cy-h">
          <h2
            id="cy-h"
            style={{
              fontSize: '0.78rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: C.textDim,
              margin: '0 0 0.6rem',
              fontWeight: 400,
            }}
          >
            <span style={{ color: C.accentLavender, marginRight: 6 }}>∂</span>
            cycles · since-Apocky-last-online ({fmtRelTimeCoarse(lastApockyOnline, now)})
          </h2>

          {/* aggregate caption */}
          <p
            style={{
              color: C.textDim,
              fontSize: '0.78rem',
              margin: '0 0 0.85rem',
              padding: '0.55rem 0.8rem',
              background: 'rgba(192, 132, 252, 0.06)',
              border: `1px solid ${C.accentPurple}33`,
              borderRadius: 6,
            }}
          >
            <span style={{ color: C.accentPurple }}>this hour:</span>{' '}
            {fmtCompact(cy?.kan_rollup ?? 0)} KAN-bias-updates ·{' '}
            {fmtCompact(cy?.self_author ?? 0)} self-author-drafts ·{' '}
            {fmtCompact(cy?.playtest ?? 0)} playtests ·{' '}
            {fmtCompact(cy?.mycelium_sync ?? 0)} mycelium-pattern-federations
          </p>

          <div style={counterGrid}>
            <CycleCounter
              glyph="✍"
              label="self-author"
              value={cy?.self_author ?? 0}
              accent={C.accentPurple}
              lastEventTs={lastSelfAuthor}
              now={now}
              stub={cy?.stub}
              bytesPerCycle={bytesPerCycle}
            />
            <CycleCounter
              glyph="⊑"
              label="playtest"
              value={cy?.playtest ?? 0}
              accent={C.accentBlue}
              lastEventTs={lastPlaytest}
              now={now}
              stub={cy?.stub}
            />
            <CycleCounter
              glyph="∂"
              label="KAN rollup"
              value={cy?.kan_rollup ?? 0}
              accent={C.accentAmber}
              lastEventTs={lastKan}
              now={now}
              stub={cy?.stub}
            />
            <CycleCounter
              glyph="⌬"
              label="mycelium sync"
              value={cy?.mycelium_sync ?? 0}
              accent={C.accentLavender}
              lastEventTs={lastMycelium}
              now={now}
              stub={cy?.stub}
            />
            <CycleCounter
              glyph="◇"
              label="Σ-Chain anchors"
              value={cy?.sigma_chain_anchors ?? 0}
              accent={C.accentMint}
              lastEventTs={lastSigma}
              now={now}
              stub={cy?.stub}
              subline="immutable · ¬ revocable"
            />
            <CycleCounter
              glyph="ω"
              label="bytes processed"
              value={null}
              accent={C.accentBlue}
              now={now}
              stub={cy?.stub}
              subline={cy?.bytes_processed ? fmtBytes(cy.bytes_processed) : '—'}
            />
          </div>
        </section>

        {/* ── Section 3 · Live feed + Sovereign-pause ─ */}
        <section aria-labelledby="ln-h">
          <h2
            id="ln-h"
            style={{
              fontSize: '0.78rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: C.textDim,
              margin: '0 0 0.6rem',
              fontWeight: 400,
            }}
          >
            <span style={{ color: C.accentLavender, marginRight: 6 }}>∿</span>
            now · sovereign-controls
          </h2>
          <div style={sideBySideGrid}>
            <LiveFeed
              events={eventList}
              now={now}
              stub={events.data?.stub}
              glyph="∿"
              title="happening now"
              windowMs={60_000}
            />
            <SovereignPause
              state={pause.data}
              now={now}
              cap={cap}
              stub={pause.data?.stub}
            />
          </div>
        </section>

        {/* ── Section 4 · Recent events table ────── */}
        <section aria-labelledby="rec-h">
          <h2
            id="rec-h"
            style={{
              fontSize: '0.78rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: C.textDim,
              margin: '0 0 0.6rem',
              fontWeight: 400,
            }}
          >
            <span style={{ color: C.accentLavender, marginRight: 6 }}>⌖</span>
            history
          </h2>
          <RecentEventsTable
            events={eventList}
            now={now}
            stub={events.data?.stub}
            limit={50}
          />
        </section>

        {/* ── Footer ──────────────────────────────── */}
        <footer
          style={{
            marginTop: '2rem',
            paddingTop: '1.25rem',
            borderTop: `1px solid ${C.border}`,
            fontSize: '0.7rem',
            color: '#5a5a6a',
            display: 'flex',
            flexWrap: 'wrap',
            gap: '0.6rem',
            justifyContent: 'space-between',
            lineHeight: 1.5,
          }}
        >
          <div>
            ◇ public-readable · NO auth-gate · transparency-mandate · all-events k-anon ≥ 10
          </div>
          <div style={{ display: 'flex', gap: '0.85rem', flexWrap: 'wrap' }}>
            <Link href="/transparency" style={{ color: C.accentBlue }}>
              /transparency
            </Link>
            <Link href="/admin/analytics" style={{ color: C.accentBlue }}>
              /admin/analytics
            </Link>
            <Link href="/mycelium" style={{ color: C.accentBlue }}>
              /mycelium
            </Link>
          </div>
        </footer>
        <div style={{ marginTop: 12, fontSize: '0.65rem', color: '#5a5a6a', textAlign: 'center' }}>
          §  total cycles since Apocky-last-online: {fmtCompact(totalCycles)} ·  total bytes:{' '}
          {fmtBytes(cy?.bytes_processed ?? 0)}
        </div>
      </main>
    </>
  );
};

// ⊑ inline test : page export is renderable
export function _testExportsAreFunctions(): boolean {
  return typeof EnginePage === 'function';
}

export default EnginePage;
