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
          {/* Clinical loads intentionally skip even the in-memory early error buffer. */}
          <script
            nonce={nonce}
            dangerouslySetInnerHTML={{
              __html: `(function(){if(location.pathname.indexOf('/shawn/clinical')===0)return;window.__akashic_pre_init=[];function p(e){try{window.__akashic_pre_init.push({message:(e&&e.message)||'pre-hydrate',source:(e&&e.filename)||'',line:(e&&e.lineno)||0,col:(e&&e.colno)||0,stack:(e&&e.error&&e.error.stack)||'',ts:Date.now()})}catch(_){}}window.addEventListener('error',p);window.addEventListener('unhandledrejection',function(e){p({message:(e&&e.reason&&e.reason.message)||String(e&&e.reason),filename:'',lineno:0,colno:0,error:(e&&e.reason)||null})});})();`,
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
