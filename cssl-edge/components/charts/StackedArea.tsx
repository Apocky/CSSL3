// § StackedArea · multi-series time-series · normalized-to-100% · pure-SVG · ≤150 LOC
// I> Sawyer-style : sum-once · cumulate-once · build-paths-once @ useMemo

import { useMemo, type CSSProperties } from 'react';

export interface StackedSeries {
  label: string;
  values: number[]; // ⊑ same-length-as siblings ; oldest-first
  color: string;
}

export interface StackedAreaProps {
  series: StackedSeries[];
  width?: number;
  height?: number;
  normalized?: boolean; // ⊑ 100%-stack vs absolute · default true
  style?: CSSProperties;
  ariaLabel?: string;
}

// ⊑ build closed-region path-string for a series-band
function bandPath(
  topYs: number[],
  bottomYs: number[],
  width: number,
): string {
  const n = topYs.length;
  if (n === 0) return '';
  const step = n > 1 ? width / (n - 1) : 0;
  let out = '';
  // ⊑ top-edge L→R
  for (let i = 0; i < n; i++) {
    const x = i * step;
    out += `${i === 0 ? 'M' : 'L'} ${x.toFixed(1)},${topYs[i].toFixed(1)} `;
  }
  // ⊑ bottom-edge R→L
  for (let i = n - 1; i >= 0; i--) {
    const x = i * step;
    out += `L ${x.toFixed(1)},${bottomYs[i].toFixed(1)} `;
  }
  return `${out.trimEnd()} Z`;
}

export default function StackedArea(props: StackedAreaProps) {
  const {
    series,
    width = 280,
    height = 80,
    normalized = true,
    style,
    ariaLabel,
  } = props;

  const { paths, totalCount } = useMemo(() => {
    if (series.length === 0) return { paths: [] as Array<{ d: string; color: string; label: string }>, totalCount: 0 };
    // ∀ series : assume equal-length else clip to-min
    let n = Infinity;
    for (const s of series) n = Math.min(n, s.values.length);
    if (!Number.isFinite(n) || n === 0) return { paths: [], totalCount: 0 };

    // I> sum-per-x for normalization
    const sums = new Float32Array(n);
    if (normalized) {
      for (const s of series) {
        for (let i = 0; i < n; i++) sums[i] += Math.max(0, s.values[i]);
      }
    } else {
      // ⊑ y-domain = max-stack
      let mx = 0;
      for (let i = 0; i < n; i++) {
        let total = 0;
        for (const s of series) total += Math.max(0, s.values[i]);
        if (total > mx) mx = total;
      }
      for (let i = 0; i < n; i++) sums[i] = mx || 1;
    }

    // I> running-cumulative bottom→top
    const cumBottom = new Float32Array(n).fill(0);
    const cumTop = new Float32Array(n);

    const out: Array<{ d: string; color: string; label: string }> = [];
    for (const s of series) {
      // ⊑ cumTop = cumBottom + s.values  (in % or absolute)
      for (let i = 0; i < n; i++) {
        const v = Math.max(0, s.values[i]);
        cumTop[i] = cumBottom[i] + v;
      }
      // ⊑ map y-axis : top of band = cumTop ; bottom = cumBottom · invert (svg y-down)
      const topYs = new Array(n);
      const bottomYs = new Array(n);
      for (let i = 0; i < n; i++) {
        const denom = sums[i] || 1;
        topYs[i] = height - (cumTop[i] / denom) * height;
        bottomYs[i] = height - (cumBottom[i] / denom) * height;
      }
      out.push({ d: bandPath(topYs, bottomYs, width), color: s.color, label: s.label });
      // ⊑ shift bottom up
      for (let i = 0; i < n; i++) cumBottom[i] = cumTop[i];
    }
    return { paths: out, totalCount: n };
  }, [series, width, height, normalized]);

  if (totalCount === 0) {
    return (
      <div
        role="img"
        aria-label={ariaLabel ?? 'stacked-area · no data'}
        style={{
          width,
          height,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          color: '#5a5a6a',
          fontSize: '0.7rem',
          border: '1px dashed #1f1f2a',
          borderRadius: 4,
          ...style,
        }}
      >
        ○ no data
      </div>
    );
  }

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label={ariaLabel ?? `stacked-area · ${series.length} series · ${totalCount} pts`}
      style={style}
    >
      {paths.map((p, i) => (
        <path key={`${p.label}-${i}`} d={p.d} fill={p.color} opacity={0.85} />
      ))}
    </svg>
  );
}
