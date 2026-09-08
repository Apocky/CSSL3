import { createContext, useCallback, useContext, useEffect, useId, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useRouter } from 'next/router';
import styles from './Feedback.module.css';

const ToastContext = createContext<(message: string) => void>(() => undefined);

export function FeedbackProvider({ children }: { children: React.ReactNode }): JSX.Element {
  const [message, setMessage] = useState('');
  const [paused, setPaused] = useState(false);
  const router = useRouter();
  const notify = useCallback((value: string) => setMessage(value.slice(0, 300)), []);
  useEffect(() => {
    if (!message || paused) return;
    const timer = window.setTimeout(() => setMessage(''), 6000);
    return () => window.clearTimeout(timer);
  }, [message, paused]);
  useEffect(() => { setMessage(''); setPaused(false); }, [router.asPath]);
  return <ToastContext.Provider value={notify}>{children}<div className={styles.toastRegion} aria-live="polite" aria-atomic="true">
    {message ? <div className={styles.toast} onMouseEnter={() => setPaused(true)} onMouseLeave={() => setPaused(false)} onFocus={() => setPaused(true)} onBlur={() => setPaused(false)}><span>{message}</span><button type="button" aria-label="Dismiss notification" onClick={() => { setMessage(''); setPaused(false); }}>×</button></div> : null}
  </div></ToastContext.Provider>;
}

export const useToast = (): ((message: string) => void) => useContext(ToastContext);

export function HelpTip({ label }: { label: string }): JSX.Element {
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const [position, setPosition] = useState<{ top: number; left: number } | null>(null);
  const show = (): void => {
    const box = trigger.current?.getBoundingClientRect();
    if (box) setPosition({ top: Math.min(box.bottom + 8, window.innerHeight - 180), left: Math.max(12, Math.min(box.left, window.innerWidth - Math.min(280, window.innerWidth - 24) - 12)) });
  };
  useEffect(() => {
    if (!position) return;
    const close = (): void => setPosition(null);
    const escape = (event: KeyboardEvent): void => { if (event.key === 'Escape') close(); };
    const outside = (event: PointerEvent): void => { if (!trigger.current?.contains(event.target as Node)) close(); };
    window.addEventListener('keydown', escape);
    window.addEventListener('scroll', close, true);
    window.addEventListener('resize', close);
    window.addEventListener('pointerdown', outside);
    return () => { window.removeEventListener('keydown', escape); window.removeEventListener('scroll', close, true); window.removeEventListener('resize', close); window.removeEventListener('pointerdown', outside); };
  }, [position]);
  return <><button ref={trigger} type="button" className={styles.help} aria-label={label} aria-describedby={position ? id : undefined} onMouseEnter={show} onMouseLeave={() => setPosition(null)} onFocus={show} onBlur={() => setPosition(null)} onClick={show}>?</button>{position ? createPortal(<span id={id} role="tooltip" className={styles.tooltip} style={position}>{label}</span>, document.body) : null}</>;
}
