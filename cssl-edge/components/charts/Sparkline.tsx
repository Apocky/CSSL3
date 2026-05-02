// § Sparkline · single-line time-series · pure-SVG · no-deps · ≤150 LOC
// I> bit-pack-mindset : pre-allocate path-string · differential-update on data-change
// W! ¬ per-frame-allocation · memoize path between renders w/ useMemo

import { useMemo, type CSSProperties } from 'react';

export interface SparklineProps {
  values: number[]; // ⊑ time-series ; oldest-first
  width?: number; // px
  height?: number; // px
  stroke?: string; // accent-color
  fill?: string; // optional gradient-fill
  strokeWidth?: number;
  showLast?: boolean; // ⊑ tail-dot
  ariaLabel?: string;
  style?: CSSProperties;
  // ⊑ optional explicit y-domain · else auto-fit
  yMin?: number;
  yMax?: number;
}

// ∀ values → SVG path-string `M x0,y0 L x1,y1 …`
function buildPath(values: number[], width: number, height: number, yMin: number, yMax: number): string {
  if (values.length === 0) return '';
  if (values.length === 1) {
    const x = width / 2;
    const y = height / 2;
    return `M ${x.toFixed(1)},${y.toFixed(1)}`;
  }
  const range = yMax - yMin || 1;
  const step = width / (values.length - 1);
  // I> Sawyer-style : single-pass · stringbuilder · no intermediate-arrays
  let out = '';
  for (let i = 0; i < values.length; i++) {
    const x = i * step;
    const v = values[i] ?? 0;
    const norm = (v - yMin) / range;
    const y = height - norm * height;
    out += `${i === 0 ? 'M' : 'L'} ${x.toFixed(1)},${y.toFixed(1)} `;
  }
  return out.trimEnd();
}

export default function Sparkline(props: SparklineProps) {
  const {
    values,
    width = 120,
    height = 36,
    stroke = '#7dd3fc',
    fill,
    strokeWidth = 1.5,
    showLast = true,
    ariaLabel,
    style,
    yMin: yMinProp,
    yMax: yMaxProp,
  } = props;

  const { path, fillPath, lastX, lastY, dMin, dMax } = useMemo(() => {
    if (values.length === 0) {
      return { path: '', fillPath: '', lastX: 0, lastY: 0, dMin: 0, dMax: 0 };
    }
    let mn = yMinProp ?? Infinity;
    let mx = yMaxProp ?? -Infinity;
    if (yMinProp === undefined || yMaxProp === undefined) {
      for (const v of values) {
        if (v < mn) mn = v;
        if (v > mx) mx = v;
      }
    }
    if (mn === mx) {
      mn = mn - 1;
      mx = mx + 1;
    }
    const p = buildPath(values, width, height, mn, mx);
    // I> fill-path = line-path + bottom-corners (closed-region)
    const fp = p ? `${p} L ${width.toFixed(1)},${height.toFixed(1)} L 0,${height.toFixed(1)} Z` : '';
    const lastIdx = values.length - 1;
    const step = values.length > 1 ? width / (values.length - 1) : 0;
    const lx = lastIdx * step;
    const lastV = values[lastIdx] ?? 0;
    const ly = height - ((lastV - mn) / (mx - mn || 1)) * height;
    return { path: p, fillPath: fp, lastX: lx, lastY: ly, dMin: mn, dMax: mx };
  }, [values, width, height, yMinProp, yMaxProp]);

  if (values.length === 0) {
    return (
      <div
        role="img"
        aria-label={ariaLabel ?? 'sparkline · no data'}
        style={{
          width,
          height,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          color: '#5a5a6a',
          fontSize: '0.65rem',
          ...style,
        }}
      >
        ○
      </div>
    );
  }

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label={ariaLabel ?? `sparkline · ${values.length} pts · range ${dMin.toFixed(1)}–${dMax.toFixed(1)}`}
      style={{ overflow: 'visible', ...style }}
    >
      {fill && fillPath && <path d={fillPath} fill={fill} />}
      <path d={path} fill="none" stroke={stroke} strokeWidth={strokeWidth} strokeLinejoin="round" strokeLinecap="round" />
      {showLast && (
        <circle cx={lastX} cy={lastY} r={2.25} fill={stroke} />
      )}
    </svg>
  );
}
