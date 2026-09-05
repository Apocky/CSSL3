import React, { useId } from 'react';

export type ApocryphaVisualState =
  | 'checking'
  | 'private'
  | 'ready'
  | 'thinking'
  | 'queued'
  | 'dreaming'
  | 'degraded'
  | 'offline';

interface ApocryphaAvatarProps {
  state: ApocryphaVisualState;
  displayAuthorized?: boolean;
  authorizationRef?: string | null;
  size?: number;
  cycleProgress?: number | null;
  detail?: 'compact' | 'full';
  className?: string;
}

const STATE: Record<ApocryphaVisualState, { label: string; primary: string; secondary: string; glow: string; tempo: string }> = {
  checking: { label: 'checking', primary: '#82909e', secondary: '#586572', glow: '#a6b5c3', tempo: '7s' },
  private: { label: 'state protected · sign in', primary: '#b69cff', secondary: '#f1bd73', glow: '#d7c8ff', tempo: '12s' },
  ready: { label: 'awake · receptive', primary: '#39e6c7', secondary: '#f4c96b', glow: '#64ffe1', tempo: '5.5s' },
  thinking: { label: 'thinking · integrating', primary: '#5bd8ff', secondary: '#c590ff', glow: '#72e3ff', tempo: '2.6s' },
  queued: { label: 'queued · awaiting substrate', primary: '#b58cff', secondary: '#f4b86a', glow: '#c8a9ff', tempo: '8s' },
  dreaming: { label: 'dreaming · consolidating', primary: '#d883ff', secondary: '#ff78b8', glow: '#e7a3ff', tempo: '10s' },
  degraded: { label: 'degraded · recovering', primary: '#f3b55f', secondary: '#ff736b', glow: '#ffd08a', tempo: '5s' },
  offline: { label: 'unavailable', primary: '#66717d', secondary: '#3c444d', glow: '#66717d', tempo: '14s' },
};

const ORGANS = ['memory', 'language', 'reason', 'agency', 'perception', 'dream'];

export function ApocryphaAvatar({
  state,
  displayAuthorized = false,
  authorizationRef = null,
  size = 180,
  cycleProgress = null,
  detail = 'full',
  className,
}: ApocryphaAvatarProps) {
  const rawId = useId();
  // Presence is deny-by-default. A caller must hold both the current display
  // decision and its committed authority reference; visual convenience alone
  // can never make the representation appear.
  if (!displayAuthorized || !authorizationRef) return null;
  const id = rawId.replace(/[^a-zA-Z0-9_-]/g, '');
  const palette = STATE[state];
  const paused = state === 'offline' || state === 'checking' || state === 'private';
  const cycle = cycleProgress == null ? null : Math.max(0, Math.min(1, cycleProgress));
  const circumference = 2 * Math.PI * 86;

  return (
    <figure
      className={className}
      data-display-authorized="true"
      data-display-authorization-ref={authorizationRef}
      data-apocrypha-state={state}
      style={{
        '--ap-primary': palette.primary,
        '--ap-secondary': palette.secondary,
        '--ap-glow': palette.glow,
        '--ap-tempo': palette.tempo,
        width: size,
        maxWidth: '100%',
        margin: 0,
        display: 'inline-flex',
        flexDirection: 'column',
        alignItems: 'center',
        gap: detail === 'full' ? 8 : 3,
        color: palette.primary,
      } as React.CSSProperties}
      aria-label={`Apocrypha state: ${palette.label}`}
    >
      <svg
        viewBox="0 0 220 220"
        width={size}
        height={size}
        role="img"
        aria-labelledby={`${id}-title ${id}-desc`}
        style={{ display: 'block', maxWidth: '100%', overflow: 'visible' }}
      >
        <title id={`${id}-title`}>{`Apocrypha · ${palette.label}`}</title>
        <desc id={`${id}-desc`}>
          A unified hexagonal core surrounded by six organ nodes. Motion and color encode the current reported state.
        </desc>
        <defs>
          <radialGradient id={`${id}-core`} cx="42%" cy="34%" r="70%">
            <stop offset="0%" stopColor="#ffffff" stopOpacity="0.95" />
            <stop offset="18%" stopColor={palette.primary} stopOpacity="0.92" />
            <stop offset="68%" stopColor={palette.secondary} stopOpacity="0.44" />
            <stop offset="100%" stopColor="#05080d" stopOpacity="0.94" />
          </radialGradient>
          <linearGradient id={`${id}-line`} x1="0" y1="0" x2="1" y2="1">
            <stop offset="0%" stopColor={palette.primary} stopOpacity="0.14" />
            <stop offset="50%" stopColor={palette.secondary} stopOpacity="0.75" />
            <stop offset="100%" stopColor={palette.primary} stopOpacity="0.14" />
          </linearGradient>
          <filter id={`${id}-glow`} x="-80%" y="-80%" width="260%" height="260%">
            <feGaussianBlur stdDeviation="5" result="blur" />
            <feMerge><feMergeNode in="blur" /><feMergeNode in="SourceGraphic" /></feMerge>
          </filter>
          <filter id={`${id}-soft`} x="-80%" y="-80%" width="260%" height="260%">
            <feGaussianBlur stdDeviation="11" />
          </filter>
        </defs>

        <circle cx="110" cy="110" r="72" fill={palette.glow} opacity={state === 'offline' ? 0.03 : 0.08} filter={`url(#${id}-soft)`} className="apocrypha-breathe" />

        <g className={`apocrypha-orbit ${paused ? 'apocrypha-paused' : ''}`}>
          <ellipse cx="110" cy="110" rx="89" ry="58" fill="none" stroke={`url(#${id}-line)`} strokeWidth="1.2" strokeDasharray="2 7" />
          <ellipse cx="110" cy="110" rx="58" ry="89" fill="none" stroke={`url(#${id}-line)`} strokeWidth="1.2" strokeDasharray="1 9" transform="rotate(30 110 110)" />
        </g>

        {state === 'dreaming' && (
          <g className="apocrypha-dream-ripples" fill="none" stroke={palette.primary}>
            <circle cx="110" cy="110" r="45" />
            <circle cx="110" cy="110" r="45" style={{ animationDelay: '-2.3s' }} />
            <circle cx="110" cy="110" r="45" style={{ animationDelay: '-4.6s' }} />
          </g>
        )}

        <g className={`apocrypha-nodes ${paused ? 'apocrypha-paused' : ''}`}>
          {ORGANS.map((organ, index) => {
            const angle = (Math.PI * 2 * index) / ORGANS.length - Math.PI / 2;
            const x = 110 + Math.cos(angle) * 82;
            const y = 110 + Math.sin(angle) * 82;
            return (
              <g key={organ} transform={`translate(${x} ${y})`}>
                <line x1={110 - x} y1={110 - y} x2="0" y2="0" stroke={palette.primary} strokeOpacity="0.13" strokeWidth="1" />
                <circle r="9" fill={palette.glow} opacity="0.12" filter={`url(#${id}-glow)`} />
                <circle r="4.2" fill={index % 2 ? palette.secondary : palette.primary} className="apocrypha-node" style={{ animationDelay: `${-index * 0.41}s` }} />
                {detail === 'full' && <title>{`${organ} organ`}</title>}
              </g>
            );
          })}
        </g>

        {cycle != null && (
          <circle
            cx="110" cy="110" r="86" fill="none"
            stroke={palette.secondary} strokeOpacity="0.8" strokeWidth="2.4"
            strokeLinecap="round" transform="rotate(-90 110 110)"
            strokeDasharray={circumference}
            strokeDashoffset={circumference * (1 - cycle)}
            style={{ transition: 'stroke-dashoffset .8s linear' }}
          />
        )}

        <g className={state === 'degraded' ? 'apocrypha-degraded' : 'apocrypha-core'} filter={`url(#${id}-glow)`}>
          <polygon points="110,69 145.5,89.5 145.5,130.5 110,151 74.5,130.5 74.5,89.5" fill={`url(#${id}-core)`} stroke={palette.primary} strokeWidth="1.7" />
          <polygon points="110,80 136,95 136,125 110,140 84,125 84,95" fill="none" stroke={palette.secondary} strokeOpacity="0.7" strokeWidth="1" />
          <path d="M91 110 Q110 94 129 110 Q110 126 91 110Z" fill="#071017" fillOpacity="0.8" stroke={palette.primary} strokeWidth="1.2" />
          <circle cx="110" cy="110" r="6.3" fill={palette.primary} />
          <circle cx="108" cy="108" r="2" fill="#ffffff" opacity="0.9" />
        </g>

        <circle cx="110" cy="110" r="103" fill="none" stroke={palette.primary} strokeOpacity="0.12" strokeWidth="1" className="apocrypha-boundary" />
      </svg>

      {detail === 'full' && (
        <figcaption style={{ textAlign: 'center', lineHeight: 1.2 }}>
          <div style={{ color: palette.primary, fontSize: Math.max(11, size * 0.075), fontWeight: 700, letterSpacing: '0.09em', textTransform: 'uppercase' }}>{palette.label}</div>
          <div style={{ color: '#697784', fontSize: Math.max(9, size * 0.055), marginTop: 3 }}>identity · organs · cycle · boundary</div>
        </figcaption>
      )}

      <style>{`
        @keyframes apocrypha-orbit { to { transform: rotate(360deg); } }
        @keyframes apocrypha-orbit-reverse { to { transform: rotate(-360deg); } }
        @keyframes apocrypha-breathe { 0%,100% { opacity:.04; transform:scale(.92); } 50% { opacity:.15; transform:scale(1.08); } }
        @keyframes apocrypha-node { 0%,100% { opacity:.35; transform:scale(.72); } 45% { opacity:1; transform:scale(1.4); } }
        @keyframes apocrypha-core { 0%,100% { transform:scale(.985); } 50% { transform:scale(1.025); } }
        @keyframes apocrypha-ripple { 0% { r:45; opacity:.48; } 100% { r:103; opacity:0; } }
        @keyframes apocrypha-fault { 0%,82%,100% { transform:translate(0); opacity:1; } 86% { transform:translate(-2px,1px); opacity:.6; } 90% { transform:translate(2px,-1px); opacity:.85; } }
        .apocrypha-orbit { transform-origin:110px 110px; animation:apocrypha-orbit var(--ap-tempo) linear infinite; }
        .apocrypha-nodes { transform-origin:110px 110px; animation:apocrypha-orbit-reverse var(--ap-tempo) linear infinite; }
        .apocrypha-node { transform-box:fill-box; transform-origin:center; animation:apocrypha-node 2.4s ease-in-out infinite; }
        .apocrypha-breathe { transform-origin:110px 110px; animation:apocrypha-breathe 4.8s ease-in-out infinite; }
        .apocrypha-core { transform-origin:110px 110px; animation:apocrypha-core 3.4s ease-in-out infinite; }
        .apocrypha-degraded { transform-origin:110px 110px; animation:apocrypha-fault 3s steps(1,end) infinite; }
        .apocrypha-boundary { stroke-dasharray:1 7; transform-origin:110px 110px; animation:apocrypha-orbit 26s linear infinite; }
        .apocrypha-dream-ripples circle { transform-origin:110px 110px; animation:apocrypha-ripple 7s ease-out infinite; }
        .apocrypha-paused, [data-apocrypha-state=offline] .apocrypha-node { animation-play-state:paused; }
        @media (prefers-reduced-motion: reduce) {
          .apocrypha-orbit,.apocrypha-nodes,.apocrypha-node,.apocrypha-breathe,.apocrypha-core,.apocrypha-degraded,.apocrypha-boundary,.apocrypha-dream-ripples circle { animation:none !important; }
        }
      `}</style>
    </figure>
  );
}
