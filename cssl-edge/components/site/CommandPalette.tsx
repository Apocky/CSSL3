import Link from 'next/link';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  PUBLIC_SURFACE_AVAILABILITY_LABELS,
  PUBLIC_SURFACE_KIND_LABELS,
  filterPublicSurfaceNodes,
} from '../../lib/public-surface-graph';

export default function CommandPalette(): JSX.Element {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  const close = useCallback((): void => {
    setOpen(false);
    setQuery('');
    window.setTimeout(() => triggerRef.current?.focus(), 0);
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      const target = event.target as HTMLElement | null;
      const typing = target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA' || target?.isContentEditable;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        if (open) close();
        else setOpen(true);
      } else if (!typing && event.key === '/') {
        event.preventDefault();
        setOpen(true);
      } else if (open && event.key === 'Escape') {
        event.preventDefault();
        close();
      } else if (open && event.key === 'Tab') {
        const dialog = dialogRef.current;
        if (!dialog) return;
        const focusable = [...dialog.querySelectorAll<HTMLElement>(
          'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
        )].filter((element) => !element.hasAttribute('hidden') && element.getAttribute('aria-hidden') !== 'true');
        const first = focusable[0];
        const last = focusable.at(-1);
        if (!first || !last) return;

        if (!dialog.contains(document.activeElement)) {
          event.preventDefault();
          first.focus();
        } else if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first.focus();
        }
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [close, open]);

  useEffect(() => {
    if (!open) return;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    const focusTimer = window.setTimeout(() => inputRef.current?.focus(), 0);
    return () => {
      window.clearTimeout(focusTimer);
      document.body.style.overflow = previousOverflow;
    };
  }, [open]);

  const results = useMemo(() => filterPublicSurfaceNodes({ query }).slice(0, 14), [query]);

  return (
    <>
      <button
        ref={triggerRef}
        className="apx-command-trigger"
        type="button"
        aria-label="Find anything in the Apocky neural index"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen(true)}
      >
        <span aria-hidden="true">⌕</span>
        <span className="apx-command-trigger-label">Find anything</span>
        <kbd>⌘K</kbd>
      </button>

      {open ? (
        <div className="apx-command-backdrop" role="presentation" onMouseDown={(event) => {
          if (event.target === event.currentTarget) close();
        }}>
          <section ref={dialogRef} className="apx-command" role="dialog" aria-modal="true" aria-labelledby="apx-command-title">
            <header className="apx-command-head">
              <div>
                <p>NEURAL INDEX</p>
                <h2 id="apx-command-title">Find any public signal</h2>
              </div>
              <button type="button" onClick={close} aria-label="Close neural index">Esc</button>
            </header>
            <label className="apx-command-search">
              <span className="sr-only">Search projects, concepts, and destinations</span>
              <span aria-hidden="true">⌕</span>
              <input
                ref={inputRef}
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search projects, concepts, states…"
                autoComplete="off"
              />
            </label>
            <div className="apx-command-results" role="list" aria-label={`${results.length} matching destinations`}>
              {results.map((node) => {
                const contents = (
                  <>
                    <span className="apx-command-node" aria-hidden="true" />
                    <span className="apx-command-result-copy">
                      <strong>{node.title}</strong>
                      <small>{node.summary}</small>
                      <em>{PUBLIC_SURFACE_KIND_LABELS[node.kind]} · {PUBLIC_SURFACE_AVAILABILITY_LABELS[node.availability]}</em>
                    </span>
                    <span className="apx-command-go" aria-hidden="true">{node.external ? '↗' : '→'}</span>
                  </>
                );
                return node.external ? (
                  <a key={node.id} className="apx-command-result" role="listitem" href={node.href} target="_blank" rel="noopener noreferrer" onClick={close}>{contents}</a>
                ) : (
                  <Link key={node.id} className="apx-command-result" role="listitem" href={node.href} onClick={close}>{contents}</Link>
                );
              })}
              {results.length === 0 ? (
                <div className="apx-command-empty" role="status">
                  <strong>No mapped signal found.</strong>
                  <span>Try a project name, “support,” “community,” “meaning,” or “time.”</span>
                </div>
              ) : null}
            </div>
            <footer className="apx-command-foot">
              <span><kbd>/</kbd> open</span><span><kbd>Esc</kbd> close</span><Link href="/atlas" onClick={close}>Full Atlas →</Link>
            </footer>
          </section>
        </div>
      ) : null}
    </>
  );
}
