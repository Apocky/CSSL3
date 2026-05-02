// § Akashic-Webpage-Records · error-boundary.tsx
// React ErrorBoundary · captures component-stack + cluster-signature.
// Place at the top of _app.tsx + per-page sections that want isolation.
//
// Cluster-signature : 16-char hash of normalized stack-frames (filenames +
// line-numbers ; ignores column + minified-symbol-names). This is the seed
// for KAN-pattern-clustering server-side (this 4-frame React error happened
// in 17 sessions on dpl_X).

import * as React from 'react';
import { capture, hash16 } from './client';

export interface AkashicErrorBoundaryProps {
  children: React.ReactNode;
  fallback?: React.ReactNode | ((err: Error) => React.ReactNode);
  // optional · per-section name for cluster-signature scoping
  scope?: string;
}

interface AEBState {
  err: Error | null;
}

// Normalize a stack-trace into a cluster-signature seed. Strips line-cols +
// minified symbol-names + URL query-strings. Result : 16-char hash.
//
// Frame extractor handles two common shapes :
//   "at Foo (https://x/_next/abc.js:42:13)"   → captures "abc.js:42"
//   "Foo@https://x/_next/abc.js:42:13"        → ditto (Firefox-style)
// Strategy : explicitly match :line:col?$ with file as the prefix.
export function clusterSignature(stack: string, scope: string | undefined): string {
  const frames = stack
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .slice(0, 5)
    .map((line) => {
      // Strip trailing ")" first ; then split on last ":" twice to get
      // (file, line, col?) — col is discarded. File-part may NOT contain ":"
      // so split-from-right works.
      const cleaned = line.replace(/\)$/, '');
      // Find the final two ":<digits>" groups.
      const m = /^(.*?):(\d+)(?::\d+)?$/.exec(cleaned);
      if (m === null) return line.slice(0, 80);
      const filePart = (m[1] ?? '').split('?')[0] ?? '';
      // Take just the basename + parent-dir so different deploys cluster.
      const tail = filePart.split(/[/(]/).slice(-2).join('/');
      const lineNo = m[2] ?? '0';
      return `${tail}:${lineNo}`;
    });
  return hash16(`${scope ?? 'global'}|${frames.join('|')}`);
}

export class AkashicErrorBoundary extends React.Component<AkashicErrorBoundaryProps, AEBState> {
  override state: AEBState = { err: null };

  static getDerivedStateFromError(err: Error): AEBState {
    return { err };
  }

  override componentDidCatch(err: Error, info: { componentStack?: string }): void {
    const stack = err.stack ?? '';
    const componentStack = info.componentStack ?? '';
    capture('react.error', {
      message: err.message ?? 'unknown',
      stack: stack.slice(0, 4000),
      component_stack: componentStack.slice(0, 4000),
      cluster_signature: clusterSignature(stack, this.props.scope),
      scope: this.props.scope ?? 'global',
    });
  }

  override render(): React.ReactNode {
    if (this.state.err !== null) {
      const fb = this.props.fallback;
      if (typeof fb === 'function') return fb(this.state.err);
      if (fb !== undefined) return fb;
      return defaultFallback(this.state.err, () => this.setState({ err: null }));
    }
    return this.props.children;
  }
}

// ─── default-fallback · sovereignty-respecting copy · NO blame-the-user ────
function defaultFallback(err: Error, retry: () => void): React.ReactElement {
  return (
    <div
      role="alert"
      style={{
        padding: '2rem',
        margin: '2rem auto',
        maxWidth: '40rem',
        backgroundColor: '#15151f',
        border: '1px solid #303040',
        borderRadius: '0.5rem',
        color: '#e6e6f0',
        fontFamily: 'system-ui, sans-serif',
      }}
    >
      <h2 style={{ marginTop: 0, fontSize: '1.25rem' }}>
        a spore landed in the Records
      </h2>
      <p style={{ opacity: 0.8 }}>
        Something glitched. The Akashic-Records caught it as a cell · the
        substrate will learn from it. You can retry, or refresh.
      </p>
      <pre
        style={{
          backgroundColor: '#0a0a0f',
          padding: '0.75rem',
          borderRadius: '0.25rem',
          fontSize: '0.85rem',
          overflowX: 'auto',
          color: '#aaa',
        }}
      >
        {err.message}
      </pre>
      <div style={{ display: 'flex', gap: '0.5rem', marginTop: '1rem' }}>
        <button
          onClick={retry}
          style={{
            padding: '0.5rem 1rem',
            backgroundColor: '#5a4cff',
            color: 'white',
            border: 'none',
            borderRadius: '0.25rem',
            cursor: 'pointer',
          }}
        >
          retry
        </button>
        <button
          onClick={() => {
            if (typeof window !== 'undefined') window.location.reload();
          }}
          style={{
            padding: '0.5rem 1rem',
            backgroundColor: 'transparent',
            color: '#e6e6f0',
            border: '1px solid #404050',
            borderRadius: '0.25rem',
            cursor: 'pointer',
          }}
        >
          refresh page
        </button>
      </div>
    </div>
  );
}
