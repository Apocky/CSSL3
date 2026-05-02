// _document.tsx · sets html-level background-color so mobile-PWAs never see white-flash
// before page-CSS hydrates. Also injects the manifest + theme-color baseline.

import { Html, Head, Main, NextScript } from 'next/document';

export default function Document() {
  return (
    <Html lang="en" style={{ backgroundColor: '#0a0a0f' }}>
      <Head>
        <link rel="manifest" href="/manifest.json" />
        <meta name="theme-color" content="#0a0a0f" />
        <link rel="icon" type="image/svg+xml" href="/icon-192.svg" />
        <link rel="apple-touch-icon" href="/icon-192.svg" />
        <style>{`
          html, body { background-color: #0a0a0f; color: #e6e6f0; }
          html { color-scheme: dark; }
        `}</style>
        {/* § Akashic-Webpage-Records · pre-hydrate error catcher.
            Catches errors BEFORE React mounts (white-screen-of-death detection).
            Errors land in window.__akashic_pre_init ; _app.tsx drains them on mount. */}
        <script
          dangerouslySetInnerHTML={{
            __html: `(function(){window.__akashic_pre_init=[];function p(e){try{window.__akashic_pre_init.push({message:(e&&e.message)||'pre-hydrate',source:(e&&e.filename)||'',line:(e&&e.lineno)||0,col:(e&&e.colno)||0,stack:(e&&e.error&&e.error.stack)||'',ts:Date.now()})}catch(_){}}window.addEventListener('error',p);window.addEventListener('unhandledrejection',function(e){p({message:(e&&e.reason&&e.reason.message)||String(e&&e.reason),filename:'',lineno:0,colno:0,error:(e&&e.reason)||null})});})();`,
          }}
        />
        {/* Termly resource-blocker · auto-blocks tracking-cookies until consent */}
        <script src="https://app.termly.io/resource-blocker/cff27b66-fa74-4275-b18b-c019f8cc372f?autoBlock=on" />
      </Head>
      <body style={{ backgroundColor: '#0a0a0f', color: '#e6e6f0', margin: 0 }}>
        <Main />
        <NextScript />
      </body>
    </Html>
  );
}
