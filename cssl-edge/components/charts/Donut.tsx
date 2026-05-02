// § Donut · proportional segments · text-label-center · pure-SVG · ≤150 LOC
// I> precomputed arc-paths · differential ¬ recompute @ render-stable

import { useMemo, type CSSProperties } from 'react';

export interface DonutSegment {
  label: string;
  value: number;
  color: string;
}

export interface DonutProps {
  segments: DonutSegment[];
  size?: number; // ⊑ outer-diameter px
  thickness?: number; // ⊑ ring-width px
  centerLabel?: string;
  centerSubLabel?: string;
  style?: CSSProperties;
  ariaLabel?: string;
}

// ⊑ polar→cartesian for SVG arc · 0deg = 12-o-clock
function polar(cx: number, cy: number, r: number, angleDeg: number): [number, number] {
  const rad = ((angleDeg - 90) * Math.PI) / 180;
  return [cx + r * Math.cos(rad), cy + r * Math.sin(rad)];
}

// ⊑ build SVG arc d-string · sweep from a→b at radius r · large-arc-flag if span>180
function arcPath(cx: number, cy: number, r: number, rInner: number, a: number, b: number): string {
  const [x0, y0] = polar(cx, cy, r, a);
  const [x1, y1] = polar(cx, cy, r, b);
  const [xi1, yi1] = polar(cx, cy, rInner, b);
  const [xi0, yi0] = polar(cx, cy, rInner, a);
  const large = b - a > 180 ? 1 : 0;
  return [
    `M ${x0.toFixed(1)} ${y0.toFixed(1)}`,
    `A ${r} ${r} 0 ${large} 1 ${x1.toFixed(1)} ${y1.toFixed(1)}`,
    `L ${xi1.toFixed(1)} ${yi1.toFixed(1)}`,
    `A ${rInner} ${rInner} 0 ${large} 0 ${xi0.toFixed(1)} ${yi0.toFixed(1)}`,
    'Z',
  ].join(' ');
}

export default function Donut(props: DonutProps) {
  const {
    segments,
    size = 120,
    thickness = 18,
    centerLabel,
    centerSubLabel,
    style,
    ariaLabel,
  } = props;

  const cx = size / 2;
  const cy = size / 2;
  const rOuter = size / 2 - 2;
  const rInner = rOuter - thickness;

  const total = useMemo(() => segments.reduce((a, s) => a + Math.max(0, s.value), 0), [segments]);

  const paths = useMemo(() => {
    if (total <= 0) return [] as Array<{ d: string; color: string; label: string; pct: number }>;
    let cursor = 0;
    const out: Array<{ d: string; color: string; label: string; pct: number }> = [];
    // I> pre-pass count active-segments to avoid full-circle bug
    const active = segments.filter((s) => s.value > 0);
    if (active.length === 1) {
      // ⊑ single-segment ≡ full ring · stroke trick
      const s = active[0]!;
      out.push({ d: '', color: s.color, label: s.label, pct: 100 });
      return out;
    }
    for (const s of segments) {
      if (s.value <= 0) continue;
      const pct = s.value / total;
      const startAngle = cursor * 360;
      const endAngle = (cursor + pct) * 360;
      out.push({
        d: arcPath(cx, cy, rOuter, rInner, startAngle, endAngle - 0.5),
        color: s.color,
        label: s.label,
        pct: pct * 100,
      });
      cursor += pct;
    }
    return out;
  }, [segments, total, cx, cy, rOuter, rInner]);

  if (total <= 0) {
    return (
      <div
        role="img"
        aria-label={ariaLabel ?? 'donut · no data'}
        style={{
          width: size,
          height: size,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          color: '#5a5a6a',
          fontSize: '0.7rem',
          border: '1px dashed #1f1f2a',
          borderRadius: '50%',
          ...style,
        }}
      >
        ○
      </div>
    );
  }

  // ⊑ single-segment full-ring fallback rendered as <circle stroke>
  const first = paths[0];
  if (paths.length === 1 && first && first.d === '') {
    return (
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} role="img" aria-label={ariaLabel ?? `donut · ${first.label} · 100%`} style={style}>
        <circle cx={cx} cy={cy} r={rOuter - thickness / 2} fill="none" stroke={first.color} strokeWidth={thickness} />
        {centerLabel && (
          <text x={cx} y={cy} textAnchor="middle" dominantBaseline="central" fontSize="0.85rem" fill="#cdd6e4">
            {centerLabel}
          </text>
        )}
      </svg>
    );
  }

  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} role="img" aria-label={ariaLabel ?? `donut · ${paths.length} segments`} style={style}>
      {paths.map((p, i) => (
        <path key={i} d={p.d} fill={p.color} />
      ))}
      {centerLabel && (
        <text x={cx} y={cy - (centerSubLabel ? 6 : 0)} textAnchor="middle" dominantBaseline="central" fontSize="0.95rem" fill="#cdd6e4" fontWeight={700}>
          {centerLabel}
        </text>
      )}
      {centerSubLabel && (
        <text x={cx} y={cy + 10} textAnchor="middle" dominantBaseline="central" fontSize="0.6rem" fill="#7a7a8c">
          {centerSubLabel}
        </text>
      )}
    </svg>
  );
}
