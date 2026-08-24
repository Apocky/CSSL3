import type { NextPage } from 'next';
import Head from 'next/head';

const TERMS = [
  {
    id: 'cssl',
    term: 'CSSL',
    meaning:
      'Short for Conscious Substrate System Language. It is a programming language: a written language used to give a computer instructions.',
  },
  {
    id: 'cslv3',
    term: 'CSLv3',
    meaning:
      'Version 3 of CSL. It is a compact notation for recording relationships, evidence, uncertainty, decisions, and rules.',
  },
  {
    id: 'loa',
    term: 'LoA',
    meaning:
      'Short for Labyrinth of Apocalypse, an unfinished game project.',
  },
  {
    id: 'alpha',
    term: 'Alpha or early test build',
    meaning:
      'An unfinished version released so people can try it and report problems. Features may be missing or change later.',
  },
  {
    id: 'account',
    term: 'Account and session',
    meaning:
      'An account is the identity used to sign in. A session is the temporary signed-in connection kept by a browser until it ends or is revoked.',
  },
  {
    id: 'consent',
    term: 'Consent',
    meaning:
      'A freely made, informed choice. Consent must be specific, can be withdrawn, and is not inferred from silence.',
  },
  {
    id: 'diagnostics',
    term: 'Diagnostics',
    meaning:
      'Information used to find and fix a problem, such as an error message or a page-speed measurement.',
  },
  {
    id: 'telemetry',
    term: 'Telemetry',
    meaning:
      'Diagnostic information that software sends automatically to another computer. On this site, optional telemetry is called optional site data and is off until a visitor saves a sharing choice.',
  },
  {
    id: 'local',
    term: 'Local',
    meaning:
      'Running or stored on the computer in front of you instead of on a remote computer reached through the internet.',
  },
  {
    id: 'self-hosted',
    term: 'Self-hosted',
    meaning:
      'Run on computers controlled by the person or organization operating the software, rather than handed to a separate hosted service.',
  },
  {
    id: 'api',
    term: 'API',
    meaning:
      'Short for application programming interface. It is a documented way for one piece of software to request information or an action from another.',
  },
  {
    id: 'runtime',
    term: 'Runtime',
    meaning:
      'The part of a program that is active while the program is running.',
  },
  {
    id: 'compiler',
    term: 'Compiler',
    meaning:
      'Software that translates source code written by a person into a form a computer can run.',
  },
  {
    id: 'language-model',
    term: 'Language model',
    meaning:
      'Software trained on text so it can work with language, such as continuing, classifying, or generating text.',
  },
  {
    id: 'drm',
    term: 'DRM',
    meaning:
      'Short for digital rights management. It is software that restricts how a digital product can be copied, opened, or used.',
  },
  {
    id: 'eula',
    term: 'EULA',
    meaning:
      'Short for End-User License Agreement. It states the terms under which someone may install or use a piece of software.',
  },
  {
    id: 'open-source',
    term: 'Open source and proprietary',
    meaning:
      'Open-source software makes its source code available under a license that permits stated forms of use and change. Proprietary software is distributed under more limited terms set by its owner.',
  },
  {
    id: 'permission',
    term: 'Permission or capability',
    meaning:
      'An explicit grant that allows a particular action. In technical pages, capability may refer to a permission represented in code.',
  },
  {
    id: 'provenance',
    term: 'Provenance',
    meaning:
      'A record of where information or an artifact came from and how it changed.',
  },
  {
    id: 'substrate',
    term: 'Substrate',
    meaning:
      'In Apocky project documents, this means a shared technical foundation used by several systems. It is a project-specific design term, not a claim that every visitor must accept.',
  },
] as const;

const SYMBOLS = [
  ['§', 'Section. It marks the start of a named part of a technical document.'],
  ['¬', 'Not or no. For example, “¬ harm” means “no harm.”'],
  ['t∞', 'Intended to remain true for the lifetime of the system. Technical specifications call this an invariant.'],
  ['✓', 'Available or verified in the specific context where it appears.'],
  ['◐', 'Partly complete or still in progress.'],
  ['○', 'Planned, not yet available, or not yet verified.'],
  ['‼', 'Important warning or requirement.'],
  ['→', 'Leads to, produces, or points toward.'],
  ['Σ', 'The Greek capital letter sigma. In “Σ-mask,” it names a project-specific permission design.'],
  ['ω', 'The Greek lowercase letter omega. In “ω-field,” it names a project-specific data design.'],
] as const;

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
      <h1 style={{ margin: 0, fontSize: 'clamp(2.5rem, 7vw, 5.5rem)', lineHeight: 0.98, letterSpacing: '-0.055em' }}>
        Words and symbols used here
      </h1>
      <p style={{ maxWidth: 720, color: 'var(--apx-copy)', fontSize: '1.05rem', lineHeight: 1.75, margin: '24px 0 0' }}>
        Public pages should make sense without this reference. When a specialist
        word is useful, its ordinary-language meaning appears here and should
        also be introduced near its first use.
      </p>

      <section id="technical-terms" aria-labelledby="terms-title" style={{ marginTop: 70 }}>
        <h2 id="terms-title" style={{ fontSize: 'clamp(1.8rem, 4vw, 3rem)', margin: '0 0 24px' }}>Words and abbreviations</h2>
        <dl style={{ margin: 0, display: 'grid', gap: 12 }}>
          {TERMS.map(({ id, term, meaning }) => (
            <div id={id} key={id} style={{ border: '1px solid var(--apx-line)', borderRadius: 14, background: 'var(--apx-panel)', padding: '20px 22px', scrollMarginTop: 90 }}>
              <dt style={{ color: 'var(--apx-ink)', fontWeight: 760 }}>{term}</dt>
              <dd style={{ margin: '8px 0 0', color: 'var(--apx-copy)', lineHeight: 1.65 }}>{meaning}</dd>
            </div>
          ))}
        </dl>
      </section>

      <section id="symbols" aria-labelledby="symbols-title" style={{ marginTop: 70 }}>
        <h2 id="symbols-title" style={{ fontSize: 'clamp(1.8rem, 4vw, 3rem)', margin: '0 0 12px' }}>Symbol key</h2>
        <p style={{ maxWidth: 720, color: 'var(--apx-copy)', lineHeight: 1.7 }}>
          General public pages avoid these symbols. They may still appear in
          clearly marked technical specifications or code examples.
        </p>
        <dl style={{ margin: '24px 0 0', display: 'grid', gap: 12 }}>
          {SYMBOLS.map(([symbol, meaning]) => (
            <div key={symbol} style={{ display: 'grid', gridTemplateColumns: 'minmax(64px, 0.18fr) 1fr', gap: 18, borderTop: '1px solid var(--apx-line)', paddingTop: 16 }}>
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
