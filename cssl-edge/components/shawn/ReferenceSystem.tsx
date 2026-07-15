import Link from 'next/link';
import { useRouter } from 'next/router';
import {
  createContext,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import katex from 'katex';
import type { ReferenceRecord } from '@/lib/shawn/types';
import styles from './Atlas.module.css';

export type ReferenceView = 'orientation' | 'technical' | 'evidence';

const VIEWS: readonly ReferenceView[] = ['orientation', 'technical', 'evidence'];

function isReferenceView(value: unknown): value is ReferenceView {
  return typeof value === 'string' && VIEWS.includes(value as ReferenceView);
}

interface ReferenceContextValue {
  readonly openReference: (slug: string, view: ReferenceView, trigger: HTMLElement | null) => void;
}

const ReferenceContext = createContext<ReferenceContextValue | null>(null);

export interface ReferenceLinkProps {
  readonly slug: string;
  readonly children: ReactNode;
  readonly view?: ReferenceView;
  readonly className?: string;
  readonly title?: string;
}

export function ReferenceLink({
  slug,
  children,
  view = 'orientation',
  className,
  title,
}: ReferenceLinkProps): JSX.Element {
  const context = useContext(ReferenceContext);
  const href = `/shawn/reference/${encodeURIComponent(slug)}`;

  const handleClick = (event: ReactMouseEvent<HTMLAnchorElement>): void => {
    if (
      context === null ||
      event.defaultPrevented ||
      event.button !== 0 ||
      event.metaKey ||
      event.ctrlKey ||
      event.shiftKey ||
      event.altKey
    ) {
      return;
    }

    event.preventDefault();
    context.openReference(slug, view, event.currentTarget);
  };

  return (
    <Link
      href={href}
      className={className ?? styles.referenceLink}
      onClick={handleClick}
      title={title}
    >
      {children}
      <span aria-hidden="true" className={styles.referenceLinkMark}>↗</span>
    </Link>
  );
}

export interface MathExpressionProps {
  readonly tex: string;
  readonly block?: boolean;
  readonly label?: string;
}

export function MathExpression({ tex, block = false, label }: MathExpressionProps): JSX.Element {
  const rendered = useMemo(() => {
    try {
      return {
        html: katex.renderToString(tex, {
          displayMode: block,
          output: 'htmlAndMathml',
          throwOnError: true,
          trust: false,
          strict: 'warn',
        }),
        error: null,
      };
    } catch (error) {
      return {
        html: null,
        error: error instanceof Error ? error.message : 'Unable to render this expression.',
      };
    }
  }, [block, tex]);

  if (rendered.html === null) {
    return (
      <code className={styles.mathFallback} title={rendered.error ?? undefined}>
        {tex}
      </code>
    );
  }

  const Element = block ? 'div' : 'span';
  return (
    <Element
      className={block ? styles.mathBlock : styles.mathInline}
      aria-label={label}
      dangerouslySetInnerHTML={{ __html: rendered.html }}
    />
  );
}

function citationDisplay(reference: ReferenceRecord): string {
  return reference.displayCitation;
}

function cslJson(reference: ReferenceRecord): string {
  const issuedYear = Number.parseInt(reference.date.slice(0, 4), 10);
  return JSON.stringify(
    {
      id: reference.slug,
      type: 'document',
      title: reference.title,
      author: reference.creators.map((literal) => ({ literal })),
      publisher: reference.publisher,
      issued: Number.isFinite(issuedYear) ? { 'date-parts': [[issuedYear]] } : undefined,
      URL: reference.urls.canonical,
      language: reference.language,
      edition: reference.edition,
      version: reference.version,
      locator: reference.exactLocator,
      identifier: reference.identifiers.map((item) => `${item.scheme}:${item.value}`),
    },
    null,
    2,
  );
}

function bibtex(reference: ReferenceRecord): string {
  const key = reference.slug.replace(/[^a-zA-Z0-9_-]/g, '-');
  const escape = (value: string): string => value.replace(/[{}]/g, '');
  return [
    `@misc{${key},`,
    `  title = {${escape(reference.title)}},`,
    `  author = {${escape(reference.creators.join(' and '))}},`,
    `  year = {${escape(reference.date.slice(0, 4))}},`,
    `  publisher = {${escape(reference.publisher)}},`,
    `  note = {${escape(`${reference.edition}; ${reference.version}; ${reference.exactLocator}`)}},`,
    `  url = {${reference.urls.canonical}},`,
    `  urldate = {${reference.accessed}}`,
    '}',
  ].join('\n');
}

export function CitationExport({ reference }: { readonly reference: ReferenceRecord }): JSX.Element {
  const [format, setFormat] = useState<'display' | 'csl' | 'bibtex'>('display');
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle');
  const content = format === 'display' ? citationDisplay(reference) : format === 'csl' ? cslJson(reference) : bibtex(reference);

  const copy = async (): Promise<void> => {
    try {
      await navigator.clipboard.writeText(content);
      setCopyState('copied');
    } catch {
      setCopyState('failed');
    }
  };

  return (
    <section className={styles.citationExport} aria-labelledby={`citation-export-${reference.slug}`}>
      <div className={styles.subsectionHeadingRow}>
        <h3 id={`citation-export-${reference.slug}`}>Export this reference</h3>
        <div className={styles.segmented} aria-label="Citation format">
          {(['display', 'csl', 'bibtex'] as const).map((item) => (
            <button
              type="button"
              key={item}
              onClick={() => {
                setFormat(item);
                setCopyState('idle');
              }}
              aria-pressed={format === item}
            >
              {item === 'display' ? 'Display' : item === 'csl' ? 'CSL-JSON' : 'BibTeX'}
            </button>
          ))}
        </div>
      </div>
      <pre tabIndex={0}>{content}</pre>
      <button type="button" className={styles.quietButton} onClick={() => void copy()}>
        {copyState === 'copied' ? 'Copied' : copyState === 'failed' ? 'Select text to copy' : 'Copy citation'}
      </button>
    </section>
  );
}

export function CitationList({ reference }: { readonly reference: ReferenceRecord }): JSX.Element {
  const isHttps = (value: string): boolean => {
    try {
      return new URL(value).protocol === 'https:';
    } catch {
      return false;
    }
  };
  const links = [
    { label: 'Canonical source', href: reference.urls.canonical },
    ...(reference.urls.openAccess ? [{ label: 'Open-access copy', href: reference.urls.openAccess }] : []),
    ...(reference.urls.archive ? [{ label: 'Archived copy', href: reference.urls.archive }] : []),
  ].filter((link) => isHttps(link.href));

  return (
    <section aria-labelledby={`sources-${reference.slug}`}>
      <h3 id={`sources-${reference.slug}`}>Sources and identifiers</h3>
      <ul className={styles.citationList}>
        {links.map((link) => (
          <li key={link.href}>
            <a href={link.href} target="_blank" rel="noopener noreferrer" referrerPolicy="no-referrer">
              {link.label}<span aria-hidden="true"> ↗</span>
            </a>
          </li>
        ))}
      </ul>
      {reference.identifiers.length > 0 ? (
        <dl className={styles.identifierList}>
          {reference.identifiers.map((identifier) => (
            <div key={`${identifier.scheme}:${identifier.value}`}>
              <dt>{identifier.scheme}</dt>
              <dd>{identifier.value}</dd>
            </div>
          ))}
        </dl>
      ) : null}
      <p className={styles.referenceMeta}>
        Accessed {reference.accessed} · link metadata verified {reference.lastVerified} · {reference.fullRead ? 'full text reviewed' : 'full-text review pending'}
        {reference.license ? ` · ${reference.license}` : ''}
      </p>
    </section>
  );
}

export function ProofExplorer({ reference }: { readonly reference: ReferenceRecord }): JSX.Element {
  return (
    <section className={styles.proofExplorer} aria-labelledby={`proof-${reference.slug}`}>
      <div className={styles.proofLabel}>{reference.evidence.label}</div>
      <h3 id={`proof-${reference.slug}`}>Evidence account</h3>
      <p>{reference.evidence.summary}</p>
      {reference.evidence.steps.length > 0 ? (
        <ol className={styles.proofSteps}>
          {reference.evidence.steps.map((step, index) => (
            <li key={`${reference.slug}-proof-${index}`}>
              <span aria-hidden="true">{String(index + 1).padStart(2, '0')}</span>
              <p>{step}</p>
            </li>
          ))}
        </ol>
      ) : null}
    </section>
  );
}

export function PrerequisiteGraph({
  reference,
  referenceBySlug,
}: {
  readonly reference: ReferenceRecord;
  readonly referenceBySlug: (slug: string) => ReferenceRecord | undefined;
}): JSX.Element {
  if (reference.prerequisites.length === 0) {
    return (
      <section aria-labelledby={`prerequisites-${reference.slug}`}>
        <h3 id={`prerequisites-${reference.slug}`}>Prerequisites</h3>
        <p className={styles.muted}>No formal prerequisites. Begin with the orientation.</p>
      </section>
    );
  }

  const graphHeight = Math.max(132, reference.prerequisites.length * 64 + 32);
  const targetY = graphHeight / 2;

  return (
    <section aria-labelledby={`prerequisites-${reference.slug}`}>
      <h3 id={`prerequisites-${reference.slug}`}>Prerequisites</h3>
      <div className={styles.prerequisiteGraph} aria-hidden="true">
        <svg viewBox={`0 0 760 ${graphHeight}`} role="presentation">
          {reference.prerequisites.map((slug, index) => {
            const prerequisite = referenceBySlug(slug);
            const label = prerequisite?.title ?? slug.replace(/-/g, ' ');
            const y = 32 + index * 64;
            return (
              <g key={`${slug}-edge`}>
                <path d={`M 278 ${y} C 360 ${y}, 372 ${targetY}, 464 ${targetY}`} />
                <rect x="8" y={y - 20} width="270" height="40" rx="4" />
                <text x="22" y={y + 5}>{label.length > 35 ? `${label.slice(0, 34)}…` : label}</text>
              </g>
            );
          })}
          <rect className={styles.prerequisiteTarget} x="464" y={targetY - 28} width="286" height="56" rx="4" />
          <text className={styles.prerequisiteTargetText} x="482" y={targetY + 5}>
            {reference.title.length > 35 ? `${reference.title.slice(0, 34)}…` : reference.title}
          </text>
        </svg>
      </div>
      <ol className={styles.prerequisiteList}>
        {reference.prerequisites.map((slug) => {
          const prerequisite = referenceBySlug(slug);
          return (
            <li key={slug}>
              {prerequisite ? (
                <ReferenceLink slug={slug}>{prerequisite.title}</ReferenceLink>
              ) : (
                <span>{slug.replace(/-/g, ' ')}</span>
              )}
            </li>
          );
        })}
      </ol>
      <p className={styles.projectionNote}>
        Projection preserves declared learning order; it does not imply logical dependence unless the evidence account says so.
      </p>
    </section>
  );
}

export function ReferenceBacklinks({ reference }: { readonly reference: ReferenceRecord }): JSX.Element {
  return (
    <section aria-labelledby={`backlinks-${reference.slug}`}>
      <h3 id={`backlinks-${reference.slug}`}>Used in this atlas</h3>
      {reference.backlinks.length > 0 ? (
        <ul className={styles.backlinkList}>
          {reference.backlinks.map((backlink) => (
            <li key={`${backlink.kind}:${backlink.id}`}>
              <span>{backlink.kind}</span>
              <a href={`/shawn#${encodeURIComponent(backlink.id)}`}>{backlink.label}</a>
            </li>
          ))}
        </ul>
      ) : (
        <p className={styles.muted}>Catalog context only; no published atlas claim currently depends on this entry.</p>
      )}
    </section>
  );
}

function BoundaryList({ title, items, tone }: { readonly title: string; readonly items: readonly string[]; readonly tone: 'supports' | 'limits' }): JSX.Element {
  return (
    <section className={tone === 'supports' ? styles.supportsPanel : styles.limitsPanel}>
      <h3>{title}</h3>
      <ul>
        {items.map((item, index) => <li key={`${tone}-${index}`}>{item}</li>)}
      </ul>
    </section>
  );
}

export interface ReferencePageProps {
  readonly reference: ReferenceRecord;
  readonly referenceBySlug: (slug: string) => ReferenceRecord | undefined;
  readonly compact?: boolean;
  readonly activeView?: ReferenceView;
  readonly onViewChange?: (view: ReferenceView) => void;
}

export function ReferencePage({
  reference,
  referenceBySlug,
  compact = false,
  activeView = 'orientation',
  onViewChange,
}: ReferencePageProps): JSX.Element {
  const showAllViews = !compact && onViewChange === undefined;
  const Title = compact ? 'h2' : 'h1';
  return (
    <article className={compact ? styles.referenceArticleCompact : styles.referenceArticle}>
      <header className={styles.referenceHeader}>
        <div className={styles.eyebrow}>{reference.domain.replace(/-/g, ' ')} · {reference.role}</div>
        <Title>{reference.title}</Title>
        <p>{reference.orientation}</p>
        <dl className={styles.referenceFacts}>
          <div><dt>Evidence mode</dt><dd>{reference.evidenceMode}</dd></div>
          <div><dt>Authority</dt><dd>{reference.authorityScope}</dd></div>
          <div><dt>Version</dt><dd>{reference.version}</dd></div>
          <div><dt>Locator</dt><dd>{reference.exactLocator}</dd></div>
          <div><dt>Review</dt><dd>{reference.fullRead ? 'Full text reviewed' : `Metadata verified ${reference.lastVerified}; full reading pending`}</dd></div>
        </dl>
      </header>

      {showAllViews ? (
        <nav className={styles.referenceTabs} aria-label="Reference depth">
          {VIEWS.map((view) => <a key={view} href={`#reference-${view}`}>{view}</a>)}
        </nav>
      ) : (
        <nav className={styles.referenceTabs} aria-label="Reference depth">
          {VIEWS.map((view) => (
            <button
              type="button"
              key={view}
              aria-pressed={activeView === view}
              onClick={() => onViewChange?.(view)}
            >
              {view}
            </button>
          ))}
        </nav>
      )}

      {showAllViews || activeView === 'orientation' ? (
        <div className={styles.referenceView} id="reference-orientation">
          <section>
            <h3>Orientation</h3>
            <p>{reference.orientation}</p>
          </section>
          <PrerequisiteGraph reference={reference} referenceBySlug={referenceBySlug} />
          <section>
            <h3>How Shawn uses it</h3>
            <p>{reference.shawnUse}</p>
          </section>
          <div className={styles.boundaryGrid}>
            <BoundaryList title="What this supports" items={reference.supports} tone="supports" />
            <BoundaryList title="What it does not support" items={reference.doesNotSupport} tone="limits" />
          </div>
        </div>
      ) : null}

      {showAllViews || activeView === 'technical' ? (
        <div className={styles.referenceView} id="reference-technical">
          <section>
            <h3>Technical account</h3>
            <p>{reference.technical}</p>
            {reference.mathExpressions.map((expression) => (
              <MathExpression
                key={expression.tex}
                tex={expression.tex}
                block
                label={expression.label}
              />
            ))}
          </section>
          <ProofExplorer reference={reference} />
          <PrerequisiteGraph reference={reference} referenceBySlug={referenceBySlug} />
        </div>
      ) : null}

      {showAllViews || activeView === 'evidence' ? (
        <div className={styles.referenceView} id="reference-evidence">
          <ProofExplorer reference={reference} />
          <div className={styles.boundaryGrid}>
            <BoundaryList title="Supported" items={reference.supports} tone="supports" />
            <BoundaryList title="Not established" items={reference.doesNotSupport} tone="limits" />
          </div>
          <section>
            <h3>Counterpositions</h3>
            <ul>{reference.counterpositions.map((item, index) => <li key={`counter-${index}`}>{item}</li>)}</ul>
          </section>
          <section>
            <h3>What would change this account</h3>
            <ul>{reference.revisionConditions.map((item, index) => <li key={`revision-${index}`}>{item}</li>)}</ul>
          </section>
          <CitationList reference={reference} />
          <ReferenceBacklinks reference={reference} />
          <CitationExport reference={reference} />
        </div>
      ) : null}

      {compact ? (
        <Link href={`/shawn/reference/${encodeURIComponent(reference.slug)}`} className={styles.fullReferenceLink}>
          Open canonical reference page <span aria-hidden="true">→</span>
        </Link>
      ) : null}
    </article>
  );
}

export function ReferenceDialogProvider({
  children,
  referenceBySlug,
}: {
  readonly children: ReactNode;
  readonly referenceBySlug: (slug: string) => ReferenceRecord | undefined;
}): JSX.Element {
  const router = useRouter();
  const dialogRef = useRef<HTMLDialogElement>(null);
  const fallbackRef = useRef<HTMLDivElement>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  const openedFromAtlasRef = useRef(false);
  const [current, setCurrent] = useState<ReferenceRecord | null>(null);
  const [view, setView] = useState<ReferenceView>('orientation');
  const [nativeDialog, setNativeDialog] = useState(true);

  const restoreTriggerFocus = useCallback((): void => {
    const trigger = returnFocusRef.current;
    if (!trigger?.isConnected) return;
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (trigger.isConnected) trigger.focus({ preventScroll: true });
      });
    });
  }, []);

  const applyUrlState = useCallback(async (
    slug: string | null,
    nextView: ReferenceView,
    mode: 'push' | 'replace' = 'push',
  ): Promise<void> => {
    const query = { ...router.query };
    if (slug === null) {
      delete query['ref'];
      delete query['view'];
    } else {
      query['ref'] = slug;
      query['view'] = nextView;
    }
    const destination = { pathname: '/shawn', query };
    if (mode === 'replace') await router.replace(destination, undefined, { shallow: true, scroll: false });
    else await router.push(destination, undefined, { shallow: true, scroll: false });
  }, [router]);

  const openReference = useCallback((slug: string, nextView: ReferenceView, trigger: HTMLElement | null): void => {
    const reference = referenceBySlug(slug);
    if (!reference) return;
    returnFocusRef.current = trigger;
    openedFromAtlasRef.current = true;
    setCurrent(reference);
    setView(nextView);
    void applyUrlState(slug, nextView);
  }, [applyUrlState, referenceBySlug]);

  const close = useCallback((): void => {
    setCurrent(null);
    if (openedFromAtlasRef.current) {
      openedFromAtlasRef.current = false;
      router.back();
    } else {
      void applyUrlState(null, view, 'replace');
    }
  }, [applyUrlState, router, view]);

  useEffect(() => {
    if (!router.isReady) return;
    const slug = typeof router.query['ref'] === 'string' ? router.query['ref'] : null;
    const requestedView = isReferenceView(router.query['view']) ? router.query['view'] : 'orientation';
    const reference = slug ? referenceBySlug(slug) : undefined;
    setCurrent(reference ?? null);
    setView(requestedView);
  }, [referenceBySlug, router.isReady, router.query]);

  useEffect(() => {
    setNativeDialog(
      typeof HTMLDialogElement !== 'undefined' &&
      typeof HTMLDialogElement.prototype.showModal === 'function',
    );
  }, []);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!nativeDialog || !dialog) return;
    if (current && !dialog.open) dialog.showModal();
    if (!current && dialog.open) {
      dialog.close();
      restoreTriggerFocus();
    }
  }, [current, nativeDialog, restoreTriggerFocus]);

  useEffect(() => {
    if (!current) return;
    const onEscape = (event: KeyboardEvent): void => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      close();
    };
    document.addEventListener('keydown', onEscape);
    return () => document.removeEventListener('keydown', onEscape);
  }, [close, current]);

  useEffect(() => {
    if (nativeDialog || !current) return;
    const frame = fallbackRef.current;
    const focusable = (): HTMLElement[] => frame
      ? Array.from(frame.querySelectorAll<HTMLElement>('a[href], button:not([disabled]), [tabindex]:not([tabindex="-1"])'))
      : [];
    focusable()[0]?.focus();
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key !== 'Tab') return;
      const items = focusable();
      const first = items[0];
      const last = items.at(-1);
      if (!first || !last) return;
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [close, current, nativeDialog]);

  useEffect(() => {
    if (!current) restoreTriggerFocus();
  }, [current, restoreTriggerFocus]);

  useEffect(() => {
    if (!current) return;
    const previousOverflow = document.documentElement.style.overflow;
    document.documentElement.style.overflow = 'hidden';
    return () => {
      document.documentElement.style.overflow = previousOverflow;
    };
  }, [current]);

  const changeView = (nextView: ReferenceView): void => {
    setView(nextView);
    if (current) void applyUrlState(current.slug, nextView, 'replace');
  };

  const dialogContent = (
    <div className={styles.dialogFrame}>
      <div className={styles.dialogToolbar}>
        <span>Reference field note</span>
        <button type="button" onClick={close} aria-label="Close reference">
          Close <span aria-hidden="true">×</span>
        </button>
      </div>
      {current ? (
        <ReferencePage
          reference={current}
          referenceBySlug={referenceBySlug}
          compact
          activeView={view}
          onViewChange={changeView}
        />
      ) : null}
    </div>
  );

  return (
    <ReferenceContext.Provider value={{ openReference }}>
      {children}
      {nativeDialog ? (
        <dialog
          ref={dialogRef}
          className={styles.referenceDialog}
          aria-label={current ? `${current.title} reference` : 'Reference'}
          onCancel={(event) => {
            event.preventDefault();
            close();
          }}
        >
          {dialogContent}
        </dialog>
      ) : current ? (
        <div
          ref={fallbackRef}
          role="dialog"
          aria-modal="true"
          aria-label={`${current.title} reference`}
          className={`${styles.referenceDialog} ${styles.referenceDialogFallback}`}
        >
          {dialogContent}
        </div>
      ) : null}
    </ReferenceContext.Provider>
  );
}
