import { useId, useState } from 'react';

interface AdminTooltipProps {
  label: string;
}

export default function AdminTooltip({ label }: AdminTooltipProps) {
  const id = useId();
  const [open, setOpen] = useState(false);

  return (
    <span
      style={{ position: 'relative', display: 'inline-flex', alignItems: 'center' }}
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
      onFocus={() => setOpen(true)}
      onBlur={() => setOpen(false)}
    >
      <button
        type="button"
        aria-label={label}
        aria-describedby={id}
        style={{
          width: 22,
          height: 22,
          borderRadius: '50%',
          border: '1px solid #2a2a3a',
          background: 'rgba(10, 10, 16, 0.9)',
          color: '#7dd3fc',
          cursor: 'help',
          fontSize: '0.72rem',
          lineHeight: '20px',
          padding: 0,
          textAlign: 'center',
        }}
      >
        ?
      </button>
      <span
        id={id}
        role="tooltip"
        style={{
          position: 'absolute',
          zIndex: 30,
          left: '50%',
          bottom: 'calc(100% + 8px)',
          width: 260,
          maxWidth: 'min(260px, calc(100vw - 32px))',
          transform: 'translateX(-50%)',
          padding: '0.65rem 0.75rem',
          borderRadius: 6,
          border: '1px solid rgba(124, 211, 252, 0.28)',
          background: 'rgba(8, 8, 13, 0.98)',
          boxShadow: '0 18px 50px rgba(0, 0, 0, 0.45)',
          color: '#dbe7f3',
          fontSize: '0.76rem',
          lineHeight: 1.45,
          opacity: open ? 1 : 0,
          pointerEvents: 'none',
          transition: 'opacity 120ms ease',
          visibility: open ? 'visible' : 'hidden',
          whiteSpace: 'normal',
          textTransform: 'none',
          letterSpacing: 0,
        }}
      >
        {label}
      </span>
    </span>
  );
}