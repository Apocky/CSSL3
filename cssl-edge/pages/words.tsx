import type { NextPage } from 'next';
import Head from 'next/head';
import { PUBLIC_GLOSSARY_SYMBOLS, PUBLIC_GLOSSARY_TERMS } from '../lib/public-glossary';

const Words: NextPage = () => (
  <>
    <Head>
      <title>Words and symbols · Apocky</title>
      <meta
        name="description"
        content="Plain-language definitions for specialist words, abbreviations, and symbols used on apocky.com."
      />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      <link rel="canonical" href="https://www.apocky.com/words" />
    </Head>

    <main style={{ width: 'min(900px, calc(100% - 36px))', margin: '0 auto', padding: '72px 0 100px' }}>
      <p className="apx-kicker">Reference</p>
      <h1 style={{ margin: 0, fontSize: 'var(--apx-fs-h1)', lineHeight: 1.05, letterSpacing: '-0.035em' }}>
        Words and symbols used here
      </h1>
      <p style={{ maxWidth: 720, color: 'var(--apx-copy)', fontSize: '1.05rem', lineHeight: 1.75, margin: '24px 0 0' }}>
        Public pages should make sense without this reference. When a specialist
        word is useful, its ordinary-language meaning appears here and should
        also be introduced near its first use.
      </p>

      <section id="technical-terms" aria-labelledby="terms-title" style={{ marginTop: 70 }}>
        <h2 id="terms-title" style={{ fontSize: 'var(--apx-fs-h2)', margin: '0 0 16px' }}>Words and abbreviations</h2>
        <dl style={{ margin: 0, display: 'grid', gap: 12 }}>
          {PUBLIC_GLOSSARY_TERMS.map(({ id, term, meaning }) => (
            <div id={id} key={id} style={{ border: '1px solid var(--apx-line)', borderRadius: 14, background: 'var(--apx-panel)', padding: '20px 22px', scrollMarginTop: 90 }}>
              <dt style={{ color: 'var(--apx-ink)', fontWeight: 760 }}>{term}</dt>
              <dd style={{ margin: '8px 0 0', color: 'var(--apx-copy)', lineHeight: 1.65 }}>{meaning}</dd>
            </div>
          ))}
        </dl>
      </section>

      <section id="symbols" aria-labelledby="symbols-title" style={{ marginTop: 70 }}>
        <h2 id="symbols-title" style={{ fontSize: 'var(--apx-fs-h2)', margin: '0 0 12px' }}>Symbol key</h2>
        <p style={{ maxWidth: 720, color: 'var(--apx-copy)', lineHeight: 1.7 }}>
          General public pages avoid these symbols. They may still appear in
          clearly marked technical specifications or code examples.
        </p>
        <dl style={{ margin: '24px 0 0', display: 'grid', gap: 12 }}>
          {PUBLIC_GLOSSARY_SYMBOLS.map(({ id, symbol, meaning }) => (
            <div id={`symbol-${id}`} key={id} style={{ display: 'grid', gridTemplateColumns: 'minmax(64px, 0.18fr) 1fr', gap: 18, borderTop: '1px solid var(--apx-line)', paddingTop: 16, scrollMarginTop: 90 }}>
              <dt style={{ color: 'var(--apx-mint-bright)', fontFamily: 'var(--apx-mono)', fontSize: '1.1rem', fontWeight: 760 }}>{symbol}</dt>
              <dd style={{ margin: 0, color: 'var(--apx-copy)', lineHeight: 1.65 }}>{meaning}</dd>
            </div>
          ))}
        </dl>
      </section>
    </main>
  </>
);

export default Words;
