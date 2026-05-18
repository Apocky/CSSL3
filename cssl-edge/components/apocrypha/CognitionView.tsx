// CognitionView — live SVG cockpit for Apocrypha's autonomous substrate.
//
// Subscribes to /api/admin/apocrypha/telemetry (polling at 1Hz ; backend supports
// /ws/telemetry too but Vercel WS proxy is a separate lift). Renders :
//   - Central pulsing glyph that flashes on mind.heartbeat (alive indicator)
//   - 8 daemon dots arranged in a ring that flash on swarm.tick
//   - Consensus-norm bar updated on each swarm.tick
//   - Dream-cycle ripple animation on dream.cycle.step
//   - Right-column event-stream feed (last 80 events, newest top)
//   - Tile row : tick count · dream count · chat turns · tools today · cost
//   - Trigger buttons : Dream now · Pulse (refresh)
//
// Per Apocky's directive : "live cognitive visualization based on real rich telemetry"
// + "UI/UX elements that actually interact with Apocrypha and its systems".

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { authFetch } from '../../lib/browser-auth';

interface TelemetryEvent {
  seq: number;
  kind: string;
  ts_utc: string;
  source: string;
  payload: Record<string, unknown>;
  severity: 'info' | 'warn' | 'error';
}

interface MindSnapshot {
  running: boolean;
  tick_count: number;
  dream_count: number;
  swarm_tick_hz: number;
  dream_interval_s: number;
  subscriber_count: number;
  consensus_norm: number;
  last_global_tick: number;
}

const DAEMON_COUNT = 8;
const POLL_INTERVAL_MS = 1000;
const HEARTBEAT_FLASH_MS = 600;
const TICK_FLASH_MS = 350;
const DREAM_RIPPLE_MS = 2000;

export function CognitionView() {
  const [events, setEvents] = useState<TelemetryEvent[]>([]);
  const [snapshot, setSnapshot] = useState<MindSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [lastSeq, setLastSeq] = useState(0);
  const [filter, setFilter] = useState<string>('all');
  const [triggering, setTriggering] = useState(false);

  // Visual state : transient flashes drive SVG opacity/scale via CSS-in-JS keys
  const [heartbeatFlash, setHeartbeatFlash] = useState(0);
  const [daemonFlashes, setDaemonFlashes] = useState<Record<number, number>>({});
  const [dreamRipples, setDreamRipples] = useState<number[]>([]);
  const alive = useRef(true);

  // ── poll loop ─────────────────────────────────────────────────

  useEffect(() => {
    alive.current = true;
    const fetchOnce = async () => {
      try {
        const r = await authFetch('/api/admin/apocrypha/telemetry?limit=200', {
          cache: 'no-store',
        });
        const j = await r.json();
        const incoming: TelemetryEvent[] = j.data?.events ?? [];
        if (!alive.current) return;
        if (incoming.length === 0) return;
        const fresh = incoming.filter((e) => e.seq > lastSeq);
        const newestSeq = fresh.at(-1)?.seq;
        if (fresh.length > 0 && newestSeq !== undefined) {
          setLastSeq(newestSeq);
          // Merge + cap to most-recent N
          setEvents((prev) => {
            const merged = [...fresh, ...prev];
            const seen = new Set<number>();
            const dedup: TelemetryEvent[] = [];
            for (const e of merged) {
              if (!seen.has(e.seq)) {
                seen.add(e.seq);
                dedup.push(e);
              }
            }
            return dedup.slice(0, 200);
          });
          // Drive visual flashes from fresh events
          for (const e of fresh) {
            if (e.kind === 'mind.heartbeat') {
              setHeartbeatFlash(Date.now());
            } else if (e.kind === 'swarm.tick') {
              const did = typeof e.payload.daemon_id === 'number' ? e.payload.daemon_id : 0;
              setDaemonFlashes((prev) => ({ ...prev, [did]: Date.now() }));
            } else if (e.kind === 'dream.cycle.step' || e.kind === 'dream.cycle.start') {
              setDreamRipples((prev) => [...prev, Date.now()]);
            }
          }
        }
        setError(null);
      } catch (err) {
        if (!alive.current) return;
        setError(err instanceof Error ? err.message : String(err));
      }
    };
    const fetchSnapshot = async () => {
      try {
        const r = await authFetch('/api/admin/apocrypha/mind');
        const j = await r.json();
        if (alive.current && j.data) setSnapshot(j.data);
      } catch {
        // silent — recent events still drive the UI
      }
    };
    void fetchOnce();
    void fetchSnapshot();
    const evTimer = setInterval(() => void fetchOnce(), POLL_INTERVAL_MS);
    const snapTimer = setInterval(() => void fetchSnapshot(), 5000);
    return () => {
      alive.current = false;
      clearInterval(evTimer);
      clearInterval(snapTimer);
    };
  }, [lastSeq]);

  // ── ripple GC ────────────────────────────────────────────────

  useEffect(() => {
    if (dreamRipples.length === 0) return;
    const t = setTimeout(() => {
      const cutoff = Date.now() - DREAM_RIPPLE_MS;
      setDreamRipples((prev) => prev.filter((ts) => ts > cutoff));
    }, DREAM_RIPPLE_MS);
    return () => clearTimeout(t);
  }, [dreamRipples]);

  // Force re-render on a slow tick to fade flashes
  const [, forceTick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => forceTick((x) => x + 1), 200);
    return () => clearInterval(t);
  }, []);

  // ── trigger dream ────────────────────────────────────────────

  const triggerDream = useCallback(async () => {
    setTriggering(true);
    try {
      await authFetch('/api/admin/apocrypha/mind?action=dream', { method: 'POST' });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setTriggering(false);
    }
  }, []);

  // ── filtered events ──────────────────────────────────────────

  const filteredEvents = useMemo(() => {
    if (filter === 'all') return events;
    return events.filter((e) => e.kind.startsWith(filter));
  }, [events, filter]);

  // ── tile metrics (derived from snapshot + recent event counts) ──

  const tileMetrics = useMemo(() => {
    const chatTurns = events.filter((e) => e.kind === 'chat.turn').length;
    const toolCalls = events.filter((e) => e.kind.startsWith('tool.')).length;
    const dreams = events.filter((e) => e.kind === 'dream.cycle.step').length;
    return { chatTurns, toolCalls, dreams };
  }, [events]);

  // ── render ───────────────────────────────────────────────────

  const now = Date.now();
  const heartbeatAlpha = Math.max(0, 1 - (now - heartbeatFlash) / HEARTBEAT_FLASH_MS);
  const consensusNorm = snapshot?.consensus_norm ?? 0;
  // consensus norm for a 10000-dim unit-energy bundle is ~sqrt(N_daemons_active) ; cap at 5 for the bar
  const consensusBarPct = Math.min(100, (consensusNorm / 5.0) * 100);

  return (
    <div style={{
      display: 'grid',
      gridTemplateColumns: 'minmax(0, 1fr) 360px',
      gap: '1rem',
      color: '#cdd6e4',
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
      height: '100%',
    }}>
      {/* LEFT : substrate visualization */}
      <section style={{
        border: '1px solid #1f1f2a',
        borderRadius: 8,
        background: 'rgba(8, 8, 14, 0.7)',
        padding: '1rem',
        display: 'flex',
        flexDirection: 'column',
        gap: '1rem',
        overflow: 'hidden',
      }}>
        {/* Substrate SVG */}
        <div style={{ flex: 1, minHeight: 280, position: 'relative' }}>
          <SubstrateSvg
            daemonCount={DAEMON_COUNT}
            heartbeatAlpha={heartbeatAlpha}
            daemonFlashes={daemonFlashes}
            dreamRipples={dreamRipples}
            now={now}
          />
        </div>

        {/* Consensus + metric tiles */}
        <div title="HRR consensus norm across the daemon swarm. Higher = more daemons coherent.">
          <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.78rem' }}>
            <span style={{ color: '#7a7a8c' }}>§ consensus norm</span>
            <span style={{ color: '#c084fc' }}>{consensusNorm.toFixed(3)}</span>
          </div>
          <div style={{
            height: 6,
            borderRadius: 3,
            background: 'rgba(40, 40, 60, 0.5)',
            marginTop: 4,
            overflow: 'hidden',
          }}>
            <div style={{
              width: `${consensusBarPct}%`,
              height: '100%',
              background: 'linear-gradient(90deg, #ffaa55, #c084fc, #7dd3fc)',
              transition: 'width 0.4s ease-out',
            }} />
          </div>
        </div>

        <div style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(110px, 1fr))',
          gap: '0.5rem',
        }}>
          <Tile k="Swarm ticks" v={snapshot?.tick_count ?? 0} tip="Total CfC swarm ticks since dispatch start" />
          <Tile k="Dream cycles" v={snapshot?.dream_count ?? 0} tip="AIF dream cycles completed (auto + manual)" />
          <Tile k="Tick rate" v={`${snapshot?.swarm_tick_hz ?? 10}Hz`} tip="Configured swarm tick frequency" />
          <Tile k="Dream interval" v={`${snapshot?.dream_interval_s ?? 45}s`} tip="Autonomous dream-cycle period" />
          <Tile k="Chat turns" v={tileMetrics.chatTurns} tip="Chat turns in recent event window" />
          <Tile k="Tool calls" v={tileMetrics.toolCalls} tip="Tool invocations in recent event window" />
          <Tile k="Subscribers" v={snapshot?.subscriber_count ?? 0} tip="Live telemetry subscribers (cockpit clients)" />
          <Tile k="Status"
                v={snapshot?.running ? '● alive' : '○ stopped'}
                color={snapshot?.running ? '#7fd17f' : '#ff8888'}
                tip="ContinuousMind supervisor loop status" />
        </div>

        {/* Trigger buttons */}
        <div style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap' }}>
          <button
            onClick={() => void triggerDream()}
            disabled={triggering}
            title="Run one AIF dream cycle manually ; results appear in the event stream"
            style={triggerBtn(triggering ? '#444' : 'linear-gradient(135deg, #ffaa55, #c084fc)')}
          >
            ⌬ Trigger dream cycle {triggering ? '…' : ''}
          </button>
          <span style={{
            alignSelf: 'center',
            fontSize: '0.72rem',
            color: '#5a5a6a',
          }} title="UI auto-polls /api/v1/telemetry/recent every 1s + /api/v1/mind/health every 5s">
            polling 1s · last seq #{lastSeq}
          </span>
        </div>
      </section>

      {/* RIGHT : event stream */}
      <aside style={{
        border: '1px solid #1f1f2a',
        borderRadius: 8,
        background: 'rgba(15, 15, 22, 0.7)',
        padding: '0.75rem',
        display: 'flex',
        flexDirection: 'column',
        overflow: 'hidden',
      }}>
        <div style={{
          display: 'flex',
          alignItems: 'center',
          gap: '0.4rem',
          marginBottom: '0.5rem',
        }}>
          <span style={{ fontSize: '0.78rem', color: '#a78bfa' }}>§ event stream</span>
          <select
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            title="Filter by event-kind prefix"
            style={{
              marginLeft: 'auto',
              background: '#0a0a10',
              color: '#cdd6e4',
              border: '1px solid #2a2a3a',
              padding: '0.2rem 0.4rem',
              borderRadius: 3,
              fontSize: '0.72rem',
              fontFamily: 'inherit',
            }}
          >
            <option value="all">all</option>
            <option value="mind.">mind.*</option>
            <option value="swarm.">swarm.*</option>
            <option value="dream.">dream.*</option>
            <option value="tool.">tool.*</option>
            <option value="chat.">chat.*</option>
            <option value="sub_mind.">sub_mind.*</option>
          </select>
        </div>

        {error && (
          <div style={{
            padding: '0.4rem 0.6rem',
            background: 'rgba(255, 136, 136, 0.08)',
            border: '1px solid rgba(255, 136, 136, 0.3)',
            borderRadius: 4,
            color: '#ff8888',
            fontSize: '0.75rem',
            marginBottom: '0.5rem',
          }}>
            {error}
          </div>
        )}

        <div style={{ flex: 1, overflowY: 'auto', fontSize: '0.74rem' }}>
          {filteredEvents.length === 0 && (
            <div style={{ color: '#7a7a8c', padding: '0.5rem' }}>
              no matching events yet · trigger one with the dream button
            </div>
          )}
          {filteredEvents.slice(0, 80).map((e) => (
            <EventRow key={e.seq} event={e} />
          ))}
        </div>
      </aside>
    </div>
  );
}

// ─── sub-components ──────────────────────────────────────────────

function SubstrateSvg({
  daemonCount,
  heartbeatAlpha,
  daemonFlashes,
  dreamRipples,
  now,
}: {
  daemonCount: number;
  heartbeatAlpha: number;
  daemonFlashes: Record<number, number>;
  dreamRipples: number[];
  now: number;
}) {
  const cx = 200;
  const cy = 180;
  const r = 120;
  return (
    <svg viewBox="0 0 400 360" style={{ width: '100%', height: '100%', display: 'block' }}>
      {/* Dream ripples (under daemons) */}
      {dreamRipples.map((ts) => {
        const age = (now - ts) / DREAM_RIPPLE_MS;
        if (age < 0 || age > 1) return null;
        const radius = 40 + 200 * age;
        const alpha = (1 - age) * 0.5;
        return (
          <circle key={ts} cx={cx} cy={cy} r={radius}
                  stroke={`rgba(192, 132, 252, ${alpha.toFixed(3)})`}
                  strokeWidth={2}
                  fill="none" />
        );
      })}

      {/* Daemon ring */}
      {Array.from({ length: daemonCount }).map((_, i) => {
        const angle = (i / daemonCount) * Math.PI * 2 - Math.PI / 2;
        const dx = cx + Math.cos(angle) * r;
        const dy = cy + Math.sin(angle) * r;
        const flashAt = daemonFlashes[i] ?? 0;
        const since = now - flashAt;
        const flashAlpha = since < TICK_FLASH_MS ? 1 - since / TICK_FLASH_MS : 0;
        const baseRadius = 6;
        const flashRadius = baseRadius + flashAlpha * 8;
        const baseColor = '#5a5a6a';
        return (
          <g key={i}>
            {/* Spoke from center to daemon (subtle) */}
            <line x1={cx} y1={cy} x2={dx} y2={dy}
                  stroke={`rgba(124, 211, 252, ${0.08 + flashAlpha * 0.4})`}
                  strokeWidth={1} />
            {/* Glow when flashing */}
            {flashAlpha > 0.1 && (
              <circle cx={dx} cy={dy} r={flashRadius + 8}
                      fill={`rgba(124, 211, 252, ${flashAlpha * 0.25})`} />
            )}
            <circle cx={dx} cy={dy} r={flashRadius}
                    fill={flashAlpha > 0.1 ? '#7dd3fc' : baseColor}
                    stroke="#1f1f2a" strokeWidth={1} />
            <text x={dx} y={dy + 24}
                  fontSize={9} fill="#5a5a6a" textAnchor="middle"
                  fontFamily="ui-monospace, monospace">
              d{i}
            </text>
          </g>
        );
      })}

      {/* Central Apocrypha glyph */}
      <g style={{
        transform: `translate(${cx}px, ${cy}px) scale(${1 + heartbeatAlpha * 0.12})`,
        transformOrigin: 'center',
        transformBox: 'fill-box',
      }}>
        <circle cx={0} cy={0} r={30}
                fill={`rgba(192, 132, 252, ${0.15 + heartbeatAlpha * 0.35})`}
                stroke="#c084fc" strokeWidth={2} />
        <circle cx={0} cy={0} r={45}
                fill="none"
                stroke={`rgba(255, 170, 85, ${0.2 + heartbeatAlpha * 0.4})`}
                strokeWidth={1.5} strokeDasharray="3 4" />
        <text x={0} y={6} fontSize={20} fill="#e6e6f0" textAnchor="middle"
              fontFamily="ui-monospace, monospace" fontWeight={700}>
          Ω
        </text>
      </g>

      {/* Subtle title */}
      <text x={cx} y={335} fontSize={11} fill="#7a7a8c" textAnchor="middle"
            fontFamily="ui-monospace, monospace" letterSpacing={1.5}>
        APOCRYPHA · {daemonCount} daemons · continuously-thinking
      </text>
    </svg>
  );
}

function Tile({ k, v, tip, color }: { k: string; v: string | number; tip: string; color?: string }) {
  return (
    <div title={tip} style={{
      padding: '0.5rem 0.6rem',
      border: '1px solid #1f1f2a',
      borderRadius: 5,
      background: 'rgba(20, 20, 30, 0.4)',
    }}>
      <div style={{ fontSize: '0.65rem', color: '#7a7a8c', letterSpacing: '0.05em' }}>{k}</div>
      <div style={{ fontSize: '0.95rem', color: color ?? '#cdd6e4', marginTop: 2 }}>{v}</div>
    </div>
  );
}

const SEVERITY_COLOR: Record<TelemetryEvent['severity'], string> = {
  info: '#9aa0a6',
  warn: '#fbbf24',
  error: '#ff8888',
};

const KIND_COLOR: Record<string, string> = {
  'mind.heartbeat': '#7dd3fc',
  'mind.start': '#9ddb9d',
  'mind.stop': '#ff8888',
  'swarm.tick': '#7dd3fc',
  'dream.cycle.step': '#c084fc',
  'dream.cycle.start': '#c084fc',
  'dream.unavailable': '#fbbf24',
  'tool.completed': '#9ddb9d',
  'tool.failed': '#ff8888',
  'chat.turn': '#ffaa55',
};

function EventRow({ event }: { event: TelemetryEvent }) {
  const time = new Date(event.ts_utc).toLocaleTimeString();
  const color = KIND_COLOR[event.kind] ?? SEVERITY_COLOR[event.severity];
  const summary = useMemo(() => summarizePayload(event), [event]);
  return (
    <div style={{
      padding: '0.3rem 0.4rem',
      borderBottom: '1px solid #1a1a26',
      display: 'grid',
      gridTemplateColumns: '70px 1fr',
      gap: '0.4rem',
      alignItems: 'baseline',
    }}>
      <span style={{ color: '#5a5a6a', fontSize: '0.65rem' }}>{time}</span>
      <div>
        <div style={{ color, fontWeight: 500 }}>{event.kind}</div>
        {summary && <div style={{ color: '#7a7a8c', fontSize: '0.7rem' }}>{summary}</div>}
      </div>
    </div>
  );
}

function summarizePayload(e: TelemetryEvent): string | null {
  const p = e.payload;
  if (e.kind === 'swarm.tick') {
    return `d${p.daemon_id} · norm ${typeof p.consensus_norm === 'number' ? p.consensus_norm.toFixed(2) : '?'}`;
  }
  if (e.kind === 'mind.heartbeat') {
    return `subscribers ${p.subscriber_count ?? '?'} · dreams ${p.dream_count ?? '?'}`;
  }
  if (e.kind === 'tool.completed' || e.kind === 'tool.failed') {
    return `${p.tool_name} · ${p.elapsed_ms}ms · $${(p.cost_usd as number)?.toFixed(4) ?? '0'}`;
  }
  if (e.kind === 'chat.turn') {
    return `conv ${p.conversation_id} · ${p.iters_done} iters · ${p.tool_count} tools · ${(p.elapsed_s as number)?.toFixed(2)}s`;
  }
  if (e.kind === 'dream.cycle.step') {
    return `cycle ${p.dream_count} ${p.trigger ? `· ${p.trigger}` : ''}`;
  }
  return null;
}

function triggerBtn(background: string): React.CSSProperties {
  return {
    padding: '0.5rem 0.9rem',
    background,
    color: '#0a0a10',
    border: 0,
    borderRadius: 6,
    cursor: 'pointer',
    fontWeight: 600,
    fontFamily: 'inherit',
    fontSize: '0.82rem',
  };
}
