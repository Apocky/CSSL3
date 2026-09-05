import { useId } from 'react';
import styles from './HomeVisual.module.css';

type Props = {
  /** `stage` fills the desktop mark column; `compact` is a small lockup mark for narrow layouts. */
  readonly variant?: 'stage' | 'compact';
};

function OrbitGradient({ id }: { id: string }): JSX.Element {
  return (
    <defs>
      <linearGradient id={id} x1="72" y1="72" x2="440" y2="440" gradientUnits="userSpaceOnUse">
        <stop className={styles.stopBlue} offset="0" />
        <stop className={styles.stopIndigo} offset=".5" />
        <stop className={styles.stopViolet} offset="1" />
      </linearGradient>
    </defs>
  );
}

// Decorative identity graphic: concentric orbits around the brand mark. It carries no data,
// status, or measurement meaning, so it stays aria-hidden and never reads runtime state.
export default function HomeVisual({ variant = 'stage' }: Props): JSX.Element {
  const gradient = useId();
  const paint = `url(#${gradient})`;
  const mark = <img className={styles.mark} src="/icons/apocky-v3-512.png" width="512" height="512" alt="" decoding="async" />;

  if (variant === 'compact') {
    return (
      <span className={styles.compact} aria-hidden="true">
        <svg className={styles.orbits} viewBox="0 0 512 512" fill="none" focusable="false">
          <OrbitGradient id={gradient} />
          <g className={styles.rings} stroke={paint} vectorEffect="non-scaling-stroke">
            <circle cx="256" cy="256" r="236" opacity=".42" />
            <path d="M23.6 215A236 236 0 0 1 336.7 34.2" strokeWidth="1.5" opacity=".85" />
          </g>
          <g className={styles.nodes} fill={paint}>
            <circle cx="23.6" cy="215" r="16" opacity=".22" /><circle cx="23.6" cy="215" r="7" />
            <circle cx="336.7" cy="34.2" r="16" opacity=".22" /><circle cx="336.7" cy="34.2" r="7" />
          </g>
        </svg>
        {mark}
      </span>
    );
  }

  return (
    <div className={`apx-home-mark ${styles.visual}`} aria-hidden="true">
      <div className={styles.field}>
        <svg className={styles.orbits} viewBox="0 0 512 512" fill="none" focusable="false">
          <OrbitGradient id={gradient} />
          <g className={styles.rings} stroke={paint} vectorEffect="non-scaling-stroke">
            <circle cx="256" cy="256" r="238" opacity=".22" />
            <circle cx="256" cy="256" r="214" strokeWidth="1.5" strokeLinecap="round" strokeDasharray="0 11" opacity=".38" />
            <circle cx="256" cy="256" r="180" opacity=".16" />
            <path d="M33.4 216.7A226 226 0 0 1 333.3 43.6M478.6 295.3A226 226 0 0 1 178.7 468.4" strokeWidth="1.5" opacity=".72" />
          </g>
          <g className={styles.nodes} fill={paint}>
            <circle cx="33.4" cy="216.7" r="9" opacity=".22" /><circle cx="33.4" cy="216.7" r="3.5" />
            <circle cx="333.3" cy="43.6" r="9" opacity=".22" /><circle cx="333.3" cy="43.6" r="3.5" />
            <circle cx="478.6" cy="295.3" r="9" opacity=".22" /><circle cx="478.6" cy="295.3" r="3.5" />
            <circle cx="178.7" cy="468.4" r="9" opacity=".22" /><circle cx="178.7" cy="468.4" r="3.5" />
          </g>
        </svg>
        {mark}
      </div>
    </div>
  );
}
