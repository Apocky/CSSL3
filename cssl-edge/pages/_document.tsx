// _document.tsx · sets html-level background-color so mobile PWAs never see a
// white flash before page CSS hydrates. Route-aware manifest metadata lives in _app.

import Document, {
  Html,
  Head,
  Main,
  NextScript,
  type DocumentContext,
  type DocumentInitialProps,
} from 'next/document';
import { AKASHIC_PRE_HYDRATION_SCRIPT } from '@/lib/akashic-telemetry/pre-hydration';

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
          <meta name="msapplication-TileColor" content="#000000" />
          <meta name="msapplication-TileImage" content="/icons/apocky-v3-192.png" />
          <meta name="format-detection" content="telephone=no" />
          <link rel="icon" type="image/png" sizes="32x32" href="/icons/apocky-v3-32.png" />
          <link rel="icon" type="image/png" sizes="16x16" href="/icons/apocky-v3-16.png" />
          <link rel="icon" sizes="any" href="/favicon.ico" />
          <link rel="mask-icon" href="/brand/apocky-monochrome.svg" color="#6366f1" />
          <link rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png" />
          <link rel="apple-touch-icon" sizes="167x167" href="/apple-touch-icon-167x167.png" />
          <link rel="apple-touch-icon" sizes="152x152" href="/apple-touch-icon-152x152.png" />
          <meta property="og:image" content="https://www.apocky.com/og/apocky-default-v3.png" />
          <meta property="og:image:width" content="1200" />
          <meta property="og:image:height" content="630" />
          <meta property="og:image:alt" content="Luminous blue and violet Apocky infinity tree on an AMOLED black field" />
          <meta name="twitter:image" content="https://www.apocky.com/og/apocky-default-v3.png" />
          <meta name="twitter:image:alt" content="Luminous blue and violet Apocky infinity tree on an AMOLED black field" />
          <style nonce={nonce}>{`
            html, body { background-color: #000000; color: #e6e6f0; }
            html { color-scheme: dark; }
          `}</style>
          {/* Install the in-memory early buffer only after a prior positive
              choice, and never on authentication/clinical blackout routes. */}
          <script
            nonce={nonce}
            dangerouslySetInnerHTML={{
              __html: AKASHIC_PRE_HYDRATION_SCRIPT,
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
