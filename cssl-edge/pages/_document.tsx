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
      </Head>
      <body style={{ backgroundColor: '#0a0a0f', color: '#e6e6f0', margin: 0 }}>
        <Main />
        <NextScript />
      </body>
    </Html>
  );
}
