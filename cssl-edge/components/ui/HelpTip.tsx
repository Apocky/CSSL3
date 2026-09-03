import { useId, useRef, useState } from 'react';

import styles from './HelpTip.module.css';

export default function HelpTip({ label, children }: { label: string; children: React.ReactNode }): JSX.Element {
  const [open, setOpen] = useState(false);
  const id = useId();
  const rootRef = useRef<HTMLSpanElement>(null);

  return (
    <span
      ref={rootRef}
      className={styles.root}
      onMouseLeave={() => setOpen(false)}
      onBlur={(event) => {
        if (!rootRef.current?.contains(event.relatedTarget)) setOpen(false);
      }}
    >
      <button
        type="button"
        className={styles.trigger}
        aria-label={label}
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((value) => !value)}
        onFocus={() => setOpen(true)}
        onMouseEnter={() => setOpen(true)}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.preventDefault();
            setOpen(false);
            event.currentTarget.blur();
          }
        }}
      >
        <span aria-hidden="true">?</span>
      </button>
      <span id={id} className={styles.tip} role="tooltip" hidden={!open}>
        {children}
      </span>
    </span>
  );
}
