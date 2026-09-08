// apocky.com/docs/changelog

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

interface Release {
  version: string;
  date: string;
  highlights: string[];
  status: 'shipped' | 'in-progress' | 'planned';
}

const RELEASES: Release[] = [
  {
    version: 'Apocrypha 1.0.1',
    date: '2026-09-08 · current',
    status: 'shipped',
    highlights: [
      'Long Oracle interpretations remain saved while you refresh, leave, or return.',
      'MemPalace, Brainmonsoon, Anamnesis, Graphify, and MNEME now contribute to the same live reading.',
      'Slow memory searches receive enough time to finish instead of being cut off early.',
      'Questions with multiple lines now reach Graphify correctly, including relationship and pattern searches.',
      'Vertical and rotated mobile readings remain centered, with card rails contained inside the page.',
      'Failures, recoveries, and missing events now produce visible diagnostic evidence.',
    ],
  },
  {
    version: 'Apocrypha 1.0.0',
    date: '2026-09-07',
    status: 'shipped',
    highlights: [
      'Qwen3.5 35B-A3B became the shared first-line model for Apocrypha and Chaos Tarot.',
      'One durable conversation path replaced queued, page-bound requests.',
      'Readings stream into the page and recover from an interrupted browser session.',
      'The live service reports its real model, worker, and memory readiness.',
    ],
  },
];

const Page: NextPage = () => {
  const colorFor = (s: Release['status']) => s === 'shipped' ? '#34d399' : s === 'in-progress' ? '#fbbf24' : '#9aa0a6';
  const labelFor = (s: Release['status']) => s === 'shipped' ? '✓ shipped' : s === 'in-progress' ? '◐ in progress' : '○ planned';
  return (
    <DocsLayout
      activeSlug="changelog"
      title="Changelog · Apocky Docs"
      description="Useful changes that are available now across Apocky and Chaos Tarot."
    >
      <h1 className="docs-h1">Changelog</h1>
      <p className="docs-blurb">What changed, when it changed, and what you can use now.</p>

      <Callout kind="note" title="Truth-in-doc">
        Shipped means the change is available in the live product and has been exercised through its real user flow.
      </Callout>

      {RELEASES.map((r) => (
        <section key={r.version} style={{ marginTop: '2rem' }}>
          <h2 className="docs-h2" style={{ marginBottom: '0.2rem' }}>{r.version}</h2>
          <div style={{ fontSize: '0.78rem', color: '#7a7a8c', marginBottom: '0.6rem' }}>
            {r.date} · <span style={{ color: colorFor(r.status) }}>{labelFor(r.status)}</span>
          </div>
          <ul className="docs-ul">
            {r.highlights.map((h, i) => <li key={i}>{h}</li>)}
          </ul>
        </section>
      ))}

      <h2 className="docs-h2">Use the current releases</h2>
      <ul className="docs-ul">
        <li><a href="/apocrypha" style={{ color: '#7dd3fc' }}>Talk to Apocrypha</a></li>
        <li><a href="https://chaos-tarot.com/reading" style={{ color: '#7dd3fc' }}>Consult the Chaos Tarot Oracle</a></li>
        <li><a href="https://github.com/Apocky" style={{ color: '#7dd3fc' }}>Review the source and release history</a></li>
      </ul>

      <PrevNextNav slug="changelog" />
    </DocsLayout>
  );
};

export default Page;
