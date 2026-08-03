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
      <Html lang="en" style={{ backgroundColor: '#0a0a0f' }}>
        <Head nonce={nonce}>
          <link rel="manifest" href="/manifest.json" />
          <meta name="theme-color" content="#0a0a0f" />
          <link rel="icon" type="image/svg+xml" href="/icon-192.svg" />
          <link rel="apple-touch-icon" href="/icon-192.svg" />
          <style nonce={nonce}>{`
            html, body { background-color: #0a0a0f; color: #e6e6f0; }
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
        <body style={{ backgroundColor: '#0a0a0f', color: '#e6e6f0', margin: 0 }}>
          <Main />
          <NextScript nonce={nonce} />
        </body>
      </Html>
    );
  }
}
