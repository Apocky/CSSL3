// _document.tsx · sets html-level background-color so mobile-PWAs never see white-flash
// before page-CSS hydrates. Also injects the manifest + theme-color baseline.

import Document, {
  Html,
  Head,
  Main,
  NextScript,
  type DocumentContext,
  type DocumentInitialProps,
} from 'next/document';

interface DocumentProps extends DocumentInitialProps {
  nonce?: string;
}

export default class ApockyDocument extends Document<DocumentProps> {
  static override async getInitialProps(ctx: DocumentContext): Promise<DocumentProps> {
    const initialProps = await Document.getInitialProps(ctx);
    const header = ctx.req?.headers['x-nonce'];
    const nonce = Array.isArray(header) ? header[0] : header;
    return { ...initialProps, ...(nonce !== undefined ? { nonce } : {}) };
  }

  override render(): JSX.Element {
    const { nonce } = this.props;
    return (
      <Html lang="en" style={{ backgroundColor: '#000000' }}>
        <Head nonce={nonce}>
          <link rel="manifest" href="/manifest.json" />
          <meta name="application-name" content="Apocky" />
          <meta name="theme-color" content="#000000" />
          <meta name="mobile-web-app-capable" content="yes" />
          <meta name="apple-mobile-web-app-capable" content="yes" />
          <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent" />
          <meta name="apple-mobile-web-app-title" content="Apocky" />
          <meta name="format-detection" content="telephone=no" />
          <link rel="icon" type="image/svg+xml" href="/favicon.svg" />
          <link rel="icon" sizes="any" href="/favicon.ico" />
          <link rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png" />
          <link rel="apple-touch-icon" sizes="167x167" href="/apple-touch-icon-167x167.png" />
          <link rel="apple-touch-icon" sizes="152x152" href="/apple-touch-icon-152x152.png" />
          <meta property="og:image" content="https://www.apocky.com/og/apocky-default-v2.png" />
          <meta property="og:image:width" content="1200" />
          <meta property="og:image:height" content="630" />
          <meta property="og:image:alt" content="Apocky section constellation on an AMOLED black field" />
          <meta name="twitter:image" content="https://www.apocky.com/og/apocky-default-v2.png" />
          <meta name="twitter:image:alt" content="Apocky section constellation on an AMOLED black field" />
          <style nonce={nonce}>{`
            html, body { background-color: #000000; color: #e6e6f0; }
            html { color-scheme: dark; }
          `}</style>
          {/* Install the in-memory early buffer only after a prior positive
              choice, and never on authentication/clinical blackout routes. */}
          <script
            nonce={nonce}
            dangerouslySetInnerHTML={{
              __html: `(function(){var pth=location.pathname||'/';if(pth==='/login'||pth==='/register'||pth==='/auth'||pth.indexOf('/auth/')===0||pth==='/shawn/clinical'||pth.indexOf('/shawn/clinical/')===0)return;var tier=null;try{tier=window.localStorage.getItem('akashic.consent.tier.v1')}catch(_){return}if(tier!=='spore'&&tier!=='mycelium'&&tier!=='akashic')return;var q=[];window.__akashic_pre_init=q;function onError(e){try{q.push({message:(e&&e.message)||'pre-hydrate',source:(e&&e.filename)||'',line:(e&&e.lineno)||0,col:(e&&e.colno)||0,stack:(e&&e.error&&e.error.stack)||'',ts:Date.now()})}catch(_){}}function onRejection(e){onError({message:(e&&e.reason&&e.reason.message)||String(e&&e.reason),filename:'',lineno:0,colno:0,error:(e&&e.reason)||null})}window.addEventListener('error',onError);window.addEventListener('unhandledrejection',onRejection);window.__akashic_pre_init_cleanup=function(){window.removeEventListener('error',onError);window.removeEventListener('unhandledrejection',onRejection);window.__akashic_pre_init_cleanup=undefined;};})();`,
            }}
          />
        </Head>
        <body style={{ backgroundColor: '#000000', color: '#e6e6f0', margin: 0 }}>
          <Main />
          <NextScript nonce={nonce} />
        </body>
      </Html>
    );
  }
}
