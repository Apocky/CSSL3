// cssl-edge · components/Callout.tsx
// Boxed inline note · note / warn / success / coming-soon variants.

import type { ReactNode } from 'react';

export type CalloutKind = 'note' | 'warn' | 'success' | 'coming-soon';

interface CalloutProps {
  kind?: CalloutKind;
  /** Optional bold heading rendered before the body. */
  title?: string;
  children: ReactNode;
}

const STYLES: Record<CalloutKind, { label: string; color: string; bg: string; border: string }> = {
  note: { label: 'Note', color: '#7dd3fc', bg: 'rgba(125, 211, 252, 0.06)', border: 'rgba(125, 211, 252, 0.25)' },
  warn: { label: 'Important', color: '#fbbf24', bg: 'rgba(251, 191, 36, 0.06)', border: 'rgba(251, 191, 36, 0.25)' },
  success: { label: 'Available now', color: '#34d399', bg: 'rgba(52, 211, 153, 0.06)', border: 'rgba(52, 211, 153, 0.25)' },
  'coming-soon': { label: 'Planned', color: '#a78bfa', bg: 'rgba(167, 139, 250, 0.06)', border: 'rgba(167, 139, 250, 0.25)' },
};

const Callout = ({ kind = 'note', title, children }: CalloutProps) => {
  const s = STYLES[kind];
  return (
    <div
      style={{
        margin: '1.2rem 0',
        padding: '0.85rem 1.1rem',
        background: s.bg,
        border: `1px solid ${s.border}`,
        borderLeft: `3px solid ${s.color}`,
        borderRadius: 6,
        fontSize: '0.9rem',
        lineHeight: 1.6,
      }}
    >
      <div style={{ display: 'flex', gap: '0.6rem', alignItems: 'baseline' }}>
        <span style={{ color: s.color, fontWeight: 700, fontSize: '0.72rem', lineHeight: 1.4 }}>{s.label}</span>
        <div style={{ flex: 1, minWidth: 0 }}>
          {title !== undefined && title !== '' ? (
            <div style={{ fontWeight: 600, color: '#e6e6f0', marginBottom: '0.3rem' }}>{title}</div>
          ) : null}
          <div style={{ color: '#cdd6e4' }}>{children}</div>
        </div>
      </div>
    </div>
  );
};

export default Callout;
