// A tooltip every control in the room shares: the title attribute plus a visible, accessible tip.
// Shows after a short hover delay (not instantly, so passing the cursor over a toolbar is quiet),
// at once on keyboard focus, and on a touch long-press -- where the click that would otherwise
// follow is swallowed, so pressing and holding explains a control without triggering it.

import { cloneElement, useEffect, useId, useRef, useState, type ReactElement, type SyntheticEvent } from 'react';

import styles from './Room.module.css';

const HOVER_DELAY_MS = 450;
const LONG_PRESS_MS = 450;
const TOUCH_SHOW_MS = 2_200;

type Align = 'start' | 'center' | 'end';
type Side = 'top' | 'bottom';

export default function Tip({ label, children, align = 'center', side = 'top' }: {
  readonly label: string;
  readonly children: ReactElement;
  readonly align?: Align;
  readonly side?: Side;
}): JSX.Element {
  const id = useId();
  const [open, setOpen] = useState(false);
  const hoverTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pressTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pressed = useRef(false);

  const clear = (timer: { current: ReturnType<typeof setTimeout> | null }) => {
    if (timer.current) { clearTimeout(timer.current); timer.current = null; }
  };
  useEffect(() => () => { clear(hoverTimer); clear(pressTimer); clear(hideTimer); }, []);

  const child = children as ReactElement<Record<string, unknown>>;
  const call = (name: string, event: SyntheticEvent) => {
    const handler = child.props[name];
    if (typeof handler === 'function') (handler as (e: SyntheticEvent) => void)(event);
  };

  const trigger = cloneElement(child, {
    title: label,
    'aria-describedby': id,
    onPointerEnter: (event: React.PointerEvent) => {
      call('onPointerEnter', event);
      if (event.pointerType !== 'mouse') return;
      clear(hoverTimer);
      hoverTimer.current = setTimeout(() => setOpen(true), HOVER_DELAY_MS);
    },
    onPointerLeave: (event: React.PointerEvent) => {
      call('onPointerLeave', event);
      if (event.pointerType !== 'mouse') return;
      clear(hoverTimer);
      setOpen(false);
    },
    onFocus: (event: React.FocusEvent) => { call('onFocus', event); if ((event.target as HTMLElement).matches(':focus-visible')) setOpen(true); },
    onBlur: (event: React.FocusEvent) => { call('onBlur', event); setOpen(false); },
    onTouchStart: (event: React.TouchEvent) => {
      call('onTouchStart', event);
      pressed.current = false;
      clear(pressTimer);
      pressTimer.current = setTimeout(() => {
        pressed.current = true;
        setOpen(true);
        clear(hideTimer);
        hideTimer.current = setTimeout(() => setOpen(false), TOUCH_SHOW_MS);
      }, LONG_PRESS_MS);
    },
    onTouchEnd: (event: React.TouchEvent) => { call('onTouchEnd', event); clear(pressTimer); },
    onTouchMove: (event: React.TouchEvent) => { call('onTouchMove', event); clear(pressTimer); },
    onContextMenu: (event: React.MouseEvent) => { if (pressed.current) event.preventDefault(); call('onContextMenu', event); },
    onClickCapture: (event: React.MouseEvent) => {
      if (pressed.current) { pressed.current = false; event.preventDefault(); event.stopPropagation(); return; }
      setOpen(false);
      call('onClickCapture', event);
    },
  });

  return <span className={styles.tipWrap}>
    {trigger}
    <span
      id={id}
      role="tooltip"
      className={`${styles.tip} ${styles[`tip_${align}`]} ${styles[`tip_${side}`]} ${open ? styles.tipOpen : ''}`}
    >{label}</span>
  </span>;
}
