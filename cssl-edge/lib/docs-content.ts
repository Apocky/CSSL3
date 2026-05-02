// cssl-edge · lib/docs-content.ts
// Static docs content metadata · drives sidebar TOC + prev/next navigation.
// Hermetic: no external fetches, all content authored as TypeScript.

export type DocStatus = 'available' | 'in-progress' | 'coming-soon' | 'subject-to-change';

export interface DocPage {
  /** URL slug under /docs/<slug>. Index page uses '' (empty). */
  slug: string;
  /** Sidebar + page-title text. */
  title: string;
  /** Short blurb shown on the index page. */
  blurb: string;
  /** Status emoji-glyph mapping ✓ / ◐ / ○ / ‼ */
  status: DocStatus;
  /** Optional grouping label for sidebar sections. */
  section: string;
}

/**
 * Authoritative ordered list of docs pages. Order = sidebar order +
 * prev/next traversal order. Edit here to add/reorder pages.
 */
export const DOC_PAGES: ReadonlyArray<DocPage> = [
  // § Overview
  {
    slug: 'getting-started',
    title: 'Getting Started',
    blurb: 'Install Labyrinth of Apocalypse · launch · first chat with the GM.',
    status: 'available',
    section: 'Overview',
  },
  {
    slug: 'keyboard-shortcuts',
    title: 'Keyboard Shortcuts',
    blurb: 'Complete keymap · movement · render-modes · screenshots · burst · pause.',
    status: 'available',
    section: 'Overview',
  },
  // § In-game UI
  {
    slug: 'chat-panel',
    title: 'Chat Panel',
    blurb: 'How to talk to the GM/DM · focus · history · sample intents.',
    status: 'available',
    section: 'In-game UI',
  },
  {
    slug: 'intents',
    title: 'Intent Vocabulary',
    blurb: 'All 12 typed intents · stage-0 keyword classifier · examples per kind.',
    status: 'available',
    section: 'In-game UI',
  },
  // § Language
  {
    slug: 'cssl-language',
    title: 'CSSL Language Overview',
    blurb: 'Why a proprietary language · sample programs · spec pointer.',
    status: 'available',
    section: 'Language',
  },
  {
    slug: 'cssl-modules',
    title: 'CSSL Modules',
    blurb: 'Module declarations · cross-module imports · multi-module compile roadmap.',
    status: 'in-progress',
    section: 'Language',
  },
  {
    slug: 'cssl-ffi',
    title: 'CSSL FFI',
    blurb: 'extern "C" surface · pointer + length pairs · u32 status-code pattern.',
    status: 'available',
    section: 'Language',
  },
  // § Substrate
  {
    slug: 'substrate',
    title: 'Substrate Primitives',
    blurb: 'ω-field · Σ-mask · KAN · HDC explained for end users.',
    status: 'available',
    section: 'Substrate',
  },
  {
    slug: 'sovereignty',
    title: 'Sovereignty Model',
    blurb: 'Caps · revocation · what data leaves the machine (answer: nothing).',
    status: 'available',
    section: 'Substrate',
  },
  {
    slug: 'mycelium',
    title: 'Mycelium + Home',
    blurb: 'Pocket-dimensions · 7 archetypes · 5 modes · cross-instance learning.',
    status: 'in-progress',
    section: 'Substrate',
  },
  // § Reference
  {
    slug: 'troubleshooting',
    title: 'Troubleshooting',
    blurb: 'Common issues · log locations · how to file a bug.',
    status: 'available',
    section: 'Reference',
  },
  {
    slug: 'changelog',
    title: 'Changelog',
    blurb: 'Released versions · what landed · what is next.',
    status: 'available',
    section: 'Reference',
  },
];

/** Locate a page by slug. Returns null if not found. */
export function findDocPage(slug: string): DocPage | null {
  return DOC_PAGES.find((p) => p.slug === slug) ?? null;
}

/** Resolve previous/next pages for sequential navigation. */
export function getDocNeighbors(slug: string): { prev: DocPage | null; next: DocPage | null } {
  const idx = DOC_PAGES.findIndex((p) => p.slug === slug);
  if (idx < 0) return { prev: null, next: null };
  const prev = idx > 0 ? (DOC_PAGES[idx - 1] ?? null) : null;
  const next = idx < DOC_PAGES.length - 1 ? (DOC_PAGES[idx + 1] ?? null) : null;
  return { prev, next };
}

/** Group pages by section for sidebar rendering. */
export function getDocSections(): ReadonlyArray<{ name: string; pages: ReadonlyArray<DocPage> }> {
  const map = new Map<string, DocPage[]>();
  const order: string[] = [];
  for (const p of DOC_PAGES) {
    if (!map.has(p.section)) {
      map.set(p.section, []);
      order.push(p.section);
    }
    map.get(p.section)!.push(p);
  }
  return order.map((name) => ({ name, pages: map.get(name) ?? [] }));
}

/** Glyph + label for status badge rendering. */
export function statusBadge(s: DocStatus): { glyph: string; label: string; color: string } {
  switch (s) {
    case 'available':
      return { glyph: '✓', label: 'Available now', color: '#34d399' };
    case 'in-progress':
      return { glyph: '◐', label: 'In progress', color: '#fbbf24' };
    case 'coming-soon':
      return { glyph: '○', label: 'Coming soon', color: '#9aa0a6' };
    case 'subject-to-change':
      return { glyph: '‼', label: 'Subject to change', color: '#f472b6' };
  }
}
