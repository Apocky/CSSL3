// § BarChart · horizontal bars · sorted-desc · top-N · pure-SVG · ≤150 LOC
// I> bit-pack : pre-compute layout-once @ memo · ¬ per-bar-allocation

import { useMemo, type CSSProperties } from 'react';

export interface BarItem {
  label: string;
  value: number;
  color?: string;
}

export interface BarChartProps {
  items: BarItem[];
  width?: number;
  topN?: number; // ⊑ default 5
  rowHeight?: number;
  defaultColor?: string;
  format?: (v: number) => string;
  style?: CSSProperties;
  ariaLabel?: string;
}

const DEFAULT_FORMAT = (v: number): string => {
  if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(1)}M`;
  if (v >= 1_000) return `${(v / 1_000).toFixed(1)}k`;
  if (Number.isInteger(v)) return String(v);
  return v.toFixed(1);
};

export default function BarChart(props: BarChartProps) {
  const {
    items,
    width = 280,
    topN = 5,
    rowHeight = 22,
    defaultColor = '#7dd3fc',
    format = DEFAULT_FORMAT,
    style,
    ariaLabel,
  } = props;

  const sorted = useMemo(() => {
    // I> single-allocation copy-then-sort · slice ≤topN
    const copy = items.slice();
    copy.sort((a, b) => b.value - a.value);
    return copy.slice(0, topN);
  }, [items, topN]);

  const max = useMemo(() => {
    let m = 0;
    for (const it of sorted) {
      if (it.value > m) m = it.value;
    }
    return m;
  }, [sorted]);

  if (sorted.length === 0 || max <= 0) {
    return (
      <div
        role="img"
        aria-label={ariaLabel ?? 'bars · no data'}
        style={{
          width,
          padding: '0.85rem',
          background: 'rgba(20, 20, 30, 0.4)',
          border: '1px solid #1f1f2a',
          borderRadius: 4,
          color: '#5a5a6a',
          fontSize: '0.7rem',
          textAlign: 'center',
          ...style,
        }}
      >
        ○ no data
      </div>
    );
  }

  // ⊑ label-column ≈ 40% · bar-column ≈ 50% · value-text ≈ 10%
  const labelW = Math.min(120, width * 0.42);
  const valueW = 50;
  const barW = width - labelW - valueW - 8;
  const totalH = sorted.length * rowHeight;

  return (
    <svg
      width={width}
      height={totalH}
      viewBox={`0 0 ${width} ${totalH}`}
      role="img"
      aria-label={ariaLabel ?? `bars · top ${sorted.length}`}
      style={{ overflow: 'visible', ...style }}
    >
      {sorted.map((it, i) => {
        const y = i * rowHeight;
        const w = (it.value / max) * barW;
        const color = it.color ?? defaultColor;
        return (
          <g key={`${it.label}-${i}`}>
            <text
              x={0}
              y={y + rowHeight / 2}
              dominantBaseline="central"
              fontSize="0.72rem"
              fill="#cdd6e4"
              style={{ fontFamily: 'inherit' }}
            >
              {it.label.length > 18 ? `${it.label.slice(0, 17)}…` : it.label}
            </text>
            <rect
              x={labelW}
              y={y + rowHeight * 0.18}
              width={Math.max(2, w)}
              height={rowHeight * 0.64}
              fill={color}
              rx={2}
              opacity={0.85}
            />
            <text
              x={labelW + barW + 4}
              y={y + rowHeight / 2}
              dominantBaseline="central"
              fontSize="0.7rem"
              fill={color}
              fontWeight={600}
              style={{ fontFamily: 'inherit' }}
            >
              {format(it.value)}
            </text>
          </g>
        );
      })}
    </svg>
  );
}
