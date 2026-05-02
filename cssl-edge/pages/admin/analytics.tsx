// /admin/analytics · live LoA telemetry + apocky.com hub metrics + Mycelium federation stats
// Phone-first responsive · sovereign-cap-protected · auto-refresh 5s · stub-mode-aware
// W! ¬ external chart-libs ; pure-SVG primitives in components/charts/
// I> "always optimize and always iterate better analytics and data collection and processing" · -Apocky

import type { NextPage } from 'next';
import type { CSSProperties, ReactNode } from 'react';
import { useMemo } from 'react';
import AdminLayout from '../../components/AdminLayout';
import Sparkline from '../../components/charts/Sparkline';
import Donut from '../../components/charts/Donut';
import BarChart from '../../components/charts/BarChart';
import StackedArea from '../../components/charts/StackedArea';
import {
  useMetrics,
  fmtCompact,
  fmtMs,
  fmtPercent,
  extractSeries,
  tagHistogram,
  type MetricKind,
  type MetricsResponse,
} from '../../lib/admin-metrics';

// ─────────────────────────────────────────────
// § Palette · matches existing apocky.com admin
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

// ⊑ deterministic tag→color cycler
const TAG_COLORS = [
  C.accentBlue,
  C.accentPurple,
  C.accentAmber,
  C.accentMint,
  C.accentLavender,
  C.accentRose,
  '#fde68a',
  '#67e8f9',
];
function colorForTag(tag: string, idx: number): string {
  return TAG_COLORS[idx % TAG_COLORS.length] ?? C.accentBlue;
}

// ─────────────────────────────────────────────
// § Card primitive · headline-num + chart + tap-to-expand
// ─────────────────────────────────────────────
interface CardProps {
  glyph: string;
  title: string;
  headline: string;
  headlineColor?: string;
  subline?: string;
  chart?: ReactNode;
  detail?: ReactNode;
  stub?: boolean;
}

function Card(props: CardProps): JSX.Element {
  const { glyph, title, headline, headlineColor = C.accentBlue, subline, chart, detail, stub } = props;
  return (
    <details
      style={{
        background: C.cardBg,
        border: `1px solid ${stub ? 'rgba(251, 191, 36, 0.3)' : C.border}`,
        borderRadius: 6,
        padding: 0,
        overflow: 'hidden',
      }}
    >
      <summary
        style={{
          padding: '0.85rem 0.95rem',
          cursor: 'pointer',
          listStyle: 'none',
          display: 'grid',
          gridTemplateColumns: '1fr auto',
          gap: '0.75rem',
          alignItems: 'center',
          minHeight: 64,
        }}
      >
        <div style={{ minWidth: 0 }}>
          <div
            style={{
              fontSize: '0.65rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: C.textDim,
              marginBottom: 4,
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
            }}
          >
            <span style={{ color: C.accentLavender, marginRight: 6 }}>{glyph}</span>
            {title}
          </div>
          <div style={{ fontSize: '1.4rem', color: headlineColor, fontWeight: 700, lineHeight: 1 }}>
            {stub ? '◐ —' : headline}
          </div>
          {subline && (
            <div style={{ fontSize: '0.7rem', color: C.textDim, marginTop: 4 }}>
              {subline}
            </div>
          )}
        </div>
        <div style={{ flexShrink: 0 }}>{chart ?? null}</div>
      </summary>
      {detail && (
        <div style={{ padding: '0.85rem 0.95rem', borderTop: `1px solid ${C.border}`, fontSize: '0.78rem' }}>
          {detail}
        </div>
      )}
    </details>
  );
}

// ⊑ generic metrics-shape→card adapters, one per section
// ─────────────────────────────────────────────
// § 1 — Engine Health
// ─────────────────────────────────────────────
function EngineHealthCard(): JSX.Element {
  const { data: tick } = useMetrics('engine.frame_tick', '1min');
  const { data: mode } = useMetrics('engine.render_mode_changed', '1hr');
  const stub = (tick?.stub ?? false) && (mode?.stub ?? false);
  const fpsSeries = useMemo(() => (tick ? extractSeries(tick.events) : []), [tick]);
  const fps = tick?.rollup.avg ?? null;
  const p95 = tick?.rollup.p95 ?? null;
  const p99 = tick?.rollup.p99 ?? null;
  const modeSegments = useMemo(() => {
    if (!mode) return [];
    const hist = tagHistogram(mode.events);
    return hist.map((h, i) => ({ label: h.label, value: h.value, color: colorForTag(h.label, i) }));
  }, [mode]);
  return (
    <Card
      glyph="⊑"
      title="Engine health"
      headline={fps !== null ? `${fps.toFixed(0)} fps` : '— fps'}
      headlineColor={fps && fps >= 55 ? C.accentMint : fps && fps >= 30 ? C.accentAmber : C.accentRose}
      subline={p95 !== null ? `p95 ${fmtMs(p95)} · p99 ${fmtMs(p99)}` : 'frame-time pending'}
      chart={<Sparkline values={fpsSeries} stroke={C.accentMint} fill="rgba(52, 211, 153, 0.12)" width={110} height={36} />}
      stub={stub}
      detail={
        <div style={{ display: 'grid', gridTemplateColumns: '1fr auto', gap: '0.75rem', alignItems: 'center' }}>
          <div>
            <div style={{ color: C.textDim, fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.12em', marginBottom: 4 }}>
              render-mode distribution
            </div>
            {modeSegments.length === 0 ? (
              <div style={{ color: C.textDim, fontSize: '0.78rem' }}>○ no mode changes recorded</div>
            ) : (
              <ul style={{ margin: 0, padding: 0, listStyle: 'none', fontSize: '0.78rem' }}>
                {modeSegments.map((s) => (
                  <li key={s.label} style={{ display: 'flex', justifyContent: 'space-between', padding: '2px 0' }}>
                    <span><span style={{ color: s.color, marginRight: 6 }}>■</span>{s.label}</span>
                    <span style={{ color: s.color }}>{s.value}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
          <Donut segments={modeSegments} size={96} thickness={14} centerLabel={fmtCompact(modeSegments.reduce((a, s) => a + s.value, 0))} centerSubLabel="changes" />
        </div>
      }
    />
  );
}

// ─────────────────────────────────────────────
// § 2 — Intent Surface
// ─────────────────────────────────────────────
function IntentCard(): JSX.Element {
  const { data: cls } = useMetrics('intent.classified', '1min');
  const { data: rt } = useMetrics('intent.routed', '1min');
  const stub = (cls?.stub ?? false) && (rt?.stub ?? false);
  const histogram = useMemo(() => {
    if (!cls) return [];
    return tagHistogram(cls.events).map((h, i) => ({ ...h, color: colorForTag(h.label, i) }));
  }, [cls]);
  const latencySeries = useMemo(() => (rt ? extractSeries(rt.events) : []), [rt]);
  const fallbackRate = useMemo(() => {
    if (!cls) return null;
    const total = cls.rollup.count || cls.events.length;
    if (total === 0) return null;
    const fallback = (cls.rollup.by_tag?.['fallback'] ?? 0) + (cls.rollup.by_tag?.['unknown'] ?? 0);
    return fallback / total;
  }, [cls]);
  return (
    <Card
      glyph="≫"
      title="Intent surface"
      headline={`${fmtCompact(cls?.rollup.count ?? 0)} cls`}
      headlineColor={C.accentPurple}
      subline={fallbackRate !== null ? `fallback ${fmtPercent(fallbackRate)} · route p95 ${fmtMs(rt?.rollup.p95 ?? null)}` : 'classify→route latency pending'}
      chart={<Sparkline values={latencySeries} stroke={C.accentPurple} fill="rgba(192, 132, 252, 0.12)" width={110} height={36} />}
      stub={stub}
      detail={
        <div>
          <div style={{ color: C.textDim, fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.12em', marginBottom: 4 }}>
            top intent-kinds
          </div>
          <BarChart items={histogram} width={300} topN={6} defaultColor={C.accentPurple} />
        </div>
      }
    />
  );
}

// ─────────────────────────────────────────────
// § 3 — GM/DM Activity
// ─────────────────────────────────────────────
function GmDmCard(): JSX.Element {
  const { data: gm } = useMetrics('gm.response_emitted', '1min');
  const { data: dm } = useMetrics('dm.phase_transition', '1hr');
  const stub = (gm?.stub ?? false) && (dm?.stub ?? false);
  const ratePerMin = gm?.rollup.rate_per_sec !== undefined ? gm.rollup.rate_per_sec * 60 : null;
  const gmSeries = useMemo(() => (gm ? extractSeries(gm.events) : []), [gm]);

  // ⊑ build phase-occupancy stacked-area · top-N phases
  const phaseSeries = useMemo(() => {
    if (!dm || dm.events.length === 0) return [];
    // bucket events into 12 time-buckets · count by-phase per-bucket
    const buckets = 12;
    const first = dm.events[0];
    const last = dm.events[dm.events.length - 1];
    if (!first || !last) return [];
    const t0 = first.ts;
    const tN = last.ts;
    const span = Math.max(1, tN - t0);
    const phaseSet = new Map<string, number[]>();
    for (const ev of dm.events) {
      const tag = ev.tag ?? 'unknown';
      let arr = phaseSet.get(tag);
      if (!arr) {
        arr = new Array(buckets).fill(0);
        phaseSet.set(tag, arr);
      }
      const idx = Math.min(buckets - 1, Math.floor(((ev.ts - t0) / span) * buckets));
      arr[idx] = (arr[idx] ?? 0) + 1;
    }
    return Array.from(phaseSet.entries())
      .slice(0, 5)
      .map(([label, values], i) => ({ label, values, color: colorForTag(label, i) }));
  }, [dm]);

  return (
    <Card
      glyph="✶"
      title="GM/DM activity"
      headline={ratePerMin !== null ? `${ratePerMin.toFixed(1)}/min` : '— /min'}
      headlineColor={C.accentLavender}
      subline={`personas: ${fmtCompact(gm?.rollup.by_tag ? Object.keys(gm.rollup.by_tag).length : 0)} · phases: ${phaseSeries.length}`}
      chart={<Sparkline values={gmSeries} stroke={C.accentLavender} fill="rgba(167, 139, 250, 0.14)" width={110} height={36} />}
      stub={stub}
      detail={
        <div>
          <div style={{ color: C.textDim, fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.12em', marginBottom: 6 }}>
            arc-phase occupancy
          </div>
          <StackedArea series={phaseSeries} width={300} height={70} normalized />
          <div style={{ marginTop: 6, display: 'flex', flexWrap: 'wrap', gap: '0.5rem', fontSize: '0.7rem' }}>
            {phaseSeries.map((s) => (
              <span key={s.label} style={{ color: s.color }}>■ {s.label}</span>
            ))}
          </div>
        </div>
      }
    />
  );
}

// ─────────────────────────────────────────────
// § 4 — Procgen
// ─────────────────────────────────────────────
function ProcgenCard(): JSX.Element {
  const { data: pg } = useMetrics('procgen.scene_built', '1min');
  const stub = pg?.stub ?? false;
  const series = useMemo(() => (pg ? extractSeries(pg.events) : []), [pg]);
  const lodHist = useMemo(() => {
    if (!pg) return [];
    return tagHistogram(pg.events).map((h, i) => ({ label: h.label, value: h.value, color: colorForTag(h.label, i) }));
  }, [pg]);
  return (
    <Card
      glyph="⌬"
      title="Procgen"
      headline={fmtCompact(pg?.rollup.count ?? 0)}
      headlineColor={C.accentBlue}
      subline={`build p95 ${fmtMs(pg?.rollup.p95 ?? null)}`}
      chart={<Sparkline values={series} stroke={C.accentBlue} fill="rgba(125, 211, 252, 0.12)" width={110} height={36} />}
      stub={stub}
      detail={
        <div style={{ display: 'grid', gridTemplateColumns: '1fr auto', gap: '0.75rem', alignItems: 'center' }}>
          <BarChart items={lodHist} width={200} topN={5} defaultColor={C.accentBlue} />
          <Donut segments={lodHist.slice(0, 5)} size={96} thickness={14} centerLabel="LOD" centerSubLabel={`${lodHist.length} tiers`} />
        </div>
      }
    />
  );
}

// ─────────────────────────────────────────────
// § 5 — MCP Tools
// ─────────────────────────────────────────────
function McpCard(): JSX.Element {
  const { data: mcp } = useMetrics('mcp.tool_called', '1min');
  const stub = mcp?.stub ?? false;
  const ratePerMin = mcp?.rollup.rate_per_sec !== undefined ? mcp.rollup.rate_per_sec * 60 : null;
  const series = useMemo(() => (mcp ? extractSeries(mcp.events) : []), [mcp]);
  const toolHist = useMemo(() => {
    if (!mcp) return [];
    return tagHistogram(mcp.events).map((h, i) => ({ label: h.label, value: h.value, color: colorForTag(h.label, i) }));
  }, [mcp]);
  // ⊑ error-rate from by_tag if backend provides ; otherwise null
  const errorRate = useMemo(() => {
    if (!mcp?.rollup.by_tag) return null;
    const errs = mcp.rollup.by_tag['error'] ?? 0;
    const total = mcp.rollup.count || 1;
    return errs / total;
  }, [mcp]);
  return (
    <Card
      glyph="⊑"
      title="MCP tools"
      headline={ratePerMin !== null ? `${ratePerMin.toFixed(1)}/min` : '— /min'}
      headlineColor={errorRate !== null && errorRate > 0.02 ? C.accentRose : C.accentBlue}
      subline={errorRate !== null ? `errors ${fmtPercent(errorRate)} · ${toolHist.length} tools` : `${toolHist.length} tools active`}
      chart={<Sparkline values={series} stroke={C.accentBlue} fill="rgba(125, 211, 252, 0.12)" width={110} height={36} />}
      stub={stub}
      detail={
        <div>
          <div style={{ color: C.textDim, fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.12em', marginBottom: 4 }}>
            top-5 by call-count
          </div>
          <BarChart items={toolHist} width={300} topN={5} defaultColor={C.accentBlue} />
        </div>
      }
    />
  );
}

// ─────────────────────────────────────────────
// § 6 — KAN Classifier
// ─────────────────────────────────────────────
function KanCard(): JSX.Element {
  const { data: kan } = useMetrics('kan.classified', '1min');
  const stub = kan?.stub ?? false;
  const series = useMemo(() => (kan ? extractSeries(kan.events) : []), [kan]);
  const swapHist = useMemo(() => {
    if (!kan) return [];
    return tagHistogram(kan.events).map((h, i) => ({ label: h.label, value: h.value, color: colorForTag(h.label, i) }));
  }, [kan]);
  const fallbackRate = useMemo(() => {
    if (!kan?.rollup.by_tag) return null;
    const fb = kan.rollup.by_tag['fallback'] ?? 0;
    const total = kan.rollup.count || 1;
    return fb / total;
  }, [kan]);
  return (
    <Card
      glyph="∂"
      title="KAN classifier"
      headline={fmtCompact(kan?.rollup.count ?? 0)}
      headlineColor={C.accentAmber}
      subline={fallbackRate !== null ? `fallback ${fmtPercent(fallbackRate)} · ${swapHist.length} swap-points` : `${swapHist.length} swap-points`}
      chart={<Sparkline values={series} stroke={C.accentAmber} fill="rgba(251, 191, 36, 0.12)" width={110} height={36} />}
      stub={stub}
      detail={
        <BarChart items={swapHist} width={300} topN={6} defaultColor={C.accentAmber} />
      }
    />
  );
}

// ─────────────────────────────────────────────
// § 7 — Mycelium
// ─────────────────────────────────────────────
function MyceliumCard(): JSX.Element {
  const { data: my } = useMetrics('mycelium.sync_event', '1min');
  const stub = my?.stub ?? false;
  const series = useMemo(() => (my ? extractSeries(my.events) : []), [my]);
  const peerCount = my?.rollup.by_tag ? Object.keys(my.rollup.by_tag).length : 0;
  return (
    <Card
      glyph="⌬"
      title="Mycelium"
      headline={`${peerCount} peers`}
      headlineColor={C.accentLavender}
      subline={`${fmtCompact(my?.rollup.count ?? 0)} sync-events · rate ${(my?.rollup.rate_per_sec ?? 0).toFixed(2)}/s`}
      chart={<Sparkline values={series} stroke={C.accentLavender} fill="rgba(167, 139, 250, 0.14)" width={110} height={36} />}
      stub={stub}
      detail={
        <div style={{ fontSize: '0.78rem' }}>
          <div style={{ color: C.textDim, fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.12em', marginBottom: 4 }}>
            federation-pattern count by-peer
          </div>
          {my?.rollup.by_tag && Object.keys(my.rollup.by_tag).length > 0 ? (
            <ul style={{ margin: 0, padding: 0, listStyle: 'none' }}>
              {Object.entries(my.rollup.by_tag).slice(0, 8).map(([peer, n]) => (
                <li key={peer} style={{ display: 'flex', justifyContent: 'space-between', padding: '2px 0' }}>
                  <span><span style={{ color: C.accentLavender, marginRight: 6 }}>⌬</span>{peer}</span>
                  <span style={{ color: C.accentLavender }}>{n}</span>
                </li>
              ))}
            </ul>
          ) : (
            <div style={{ color: C.textDim }}>○ no mycelium activity yet</div>
          )}
        </div>
      }
    />
  );
}

// ─────────────────────────────────────────────
// § 8 — Consent Surface
// ─────────────────────────────────────────────
function ConsentCard(): JSX.Element {
  const { data: granted } = useMetrics('consent.cap_granted', '1hr');
  const { data: revoked } = useMetrics('consent.cap_revoked', '1hr');
  const stub = (granted?.stub ?? false) && (revoked?.stub ?? false);
  const grantedSeries = useMemo(() => (granted ? extractSeries(granted.events) : []), [granted]);
  const revokedSeries = useMemo(() => (revoked ? extractSeries(revoked.events) : []), [revoked]);

  // ⊑ Σ-mask-density-by-domain · sourced from rollup.by_tag if granted-events tagged with domain
  const domainHist = useMemo(() => {
    if (!granted) return [];
    return tagHistogram(granted.events).map((h, i) => ({ label: h.label, value: h.value, color: colorForTag(h.label, i) }));
  }, [granted]);

  return (
    <Card
      glyph="◇"
      title="Consent surface"
      headline={`+${fmtCompact(granted?.rollup.count ?? 0)} / -${fmtCompact(revoked?.rollup.count ?? 0)}`}
      headlineColor={C.accentMint}
      subline={`Σ-mask domains: ${domainHist.length}`}
      chart={
        <div style={{ display: 'flex', gap: 4 }}>
          <Sparkline values={grantedSeries} stroke={C.accentMint} fill="rgba(52, 211, 153, 0.12)" width={54} height={36} ariaLabel="grants" />
          <Sparkline values={revokedSeries} stroke={C.accentRose} fill="rgba(248, 113, 113, 0.12)" width={54} height={36} ariaLabel="revokes" />
        </div>
      }
      stub={stub}
      detail={
        <div>
          <div style={{ color: C.textDim, fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.12em', marginBottom: 4 }}>
            Σ-mask density by-domain
          </div>
          <BarChart items={domainHist} width={300} topN={6} defaultColor={C.accentMint} />
        </div>
      }
    />
  );
}

// ─────────────────────────────────────────────
// § Page · responsive grid 1/2/4 col
// ─────────────────────────────────────────────
const Analytics: NextPage = () => {
  // ⊑ aggregated stub-detection · if all-cards-stub → page-level banner
  const a = useMetrics('engine.frame_tick', '1min');
  const allStub = a.data?.stub ?? false;

  const gridStyle: CSSProperties = {
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
    gap: '0.75rem',
  };

  return (
    <AdminLayout title="∂ Analytics">
      <p style={{ color: C.textDim, fontSize: '0.82rem', marginTop: 0, marginBottom: '0.5rem' }}>
        live LoA telemetry · apocky.com hub metrics · Mycelium federation · auto-refresh 5s · tap to expand
      </p>
      <p style={{ color: '#5a5a6a', fontSize: '0.72rem', marginTop: 0, marginBottom: '1.25rem', fontStyle: 'italic' }}>
        {'I> "always optimize and always iterate better analytics and data-collection and processing"'}
      </p>

      {allStub && (
        <div
          style={{
            padding: '1rem 1.25rem',
            background: 'rgba(251, 191, 36, 0.1)',
            border: '1px solid rgba(251, 191, 36, 0.4)',
            borderRadius: 6,
            marginBottom: '1.25rem',
            fontSize: '0.82rem',
            color: C.accentAmber,
          }}
        >
          <strong>⚠ stub-mode</strong>
          <p style={{ margin: '0.4rem 0 0' }}>
            Telemetry endpoints (sibling-W11-4 analytics-pipeline-agent) not yet wired · cards render zero-state.
            LoA.exe must be running with telemetry-consent (sovereign-cap) granted for live data to populate.
          </p>
        </div>
      )}

      <div style={gridStyle}>
        <EngineHealthCard />
        <IntentCard />
        <GmDmCard />
        <ProcgenCard />
        <McpCard />
        <KanCard />
        <MyceliumCard />
        <ConsentCard />
      </div>

      <footer style={{ marginTop: '2rem', paddingTop: '1.25rem', borderTop: `1px solid ${C.border}`, fontSize: '0.7rem', color: '#5a5a6a', textAlign: 'center' }}>
        § sovereign-cap · admin-allowlist enforced server-side via /api/admin/check
      </footer>
    </AdminLayout>
  );
};

export default Analytics;
