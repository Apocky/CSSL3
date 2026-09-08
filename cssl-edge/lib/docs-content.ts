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
  /** Publication status shown as ordinary words. */
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
    blurb: 'Install Labyrinth of Apocalypse, launch it, and begin a game.',
    status: 'available',
    section: 'Overview',
  },
  {
    slug: 'keyboard-shortcuts',
    title: 'Keyboard Shortcuts',
    blurb: 'Keyboard controls for movement, display modes, screenshots, and pausing.',
    status: 'available',
    section: 'Overview',
  },
  // § In-game UI
  {
    slug: 'chat-panel',
    title: 'Chat Panel',
    blurb: 'How to use the game’s conversation panel, history, and example requests.',
    status: 'available',
    section: 'In-game UI',
  },
  {
    slug: 'intents',
    title: 'How the game reads requests',
    blurb: 'The twelve kinds of request the current game can recognize, with examples.',
    status: 'available',
    section: 'In-game UI',
  },
  // § Language
  {
    slug: 'cssl-language',
    title: 'CSSL Language Overview',
    blurb: 'What the CSSL programming language is, why it exists, and short examples.',
    status: 'available',
    section: 'Language',
  },
  {
    slug: 'cssl-modules',
    title: 'CSSL Modules',
    blurb: 'How CSSL programs are divided into reusable files and what remains unfinished.',
    status: 'in-progress',
    section: 'Language',
  },
  {
    slug: 'cssl-ffi',
    title: 'How CSSL calls other code',
    blurb: 'How CSSL exchanges data with code written in other programming languages.',
    status: 'available',
    section: 'Language',
  },
  // § Substrate
  {
    slug: 'substrate',
    title: 'Technical foundations',
    blurb: 'Plain introductions to the experimental computing ideas used in the project.',
    status: 'available',
    section: 'Substrate',
  },
  {
    slug: 'sovereignty',
    title: 'Permissions and data sharing',
    blurb: 'What the current software does with permissions and data, separated from future plans.',
    status: 'available',
    section: 'Substrate',
  },
  {
    slug: 'mycelium',
    title: 'Mycelium and Home',
    blurb: 'A plain-language introduction to a planned network and personal-space design.',
    status: 'in-progress',
    section: 'Substrate',
  },
  // § Reference
  {
    slug: 'troubleshooting',
    title: 'Troubleshooting',
    blurb: 'Common problems, where to find logs, and how to report a bug.',
    status: 'available',
    section: 'Reference',
  },
  {
    slug: 'changelog',
    title: 'Changelog',
    blurb: 'Released versions, work in progress, and future plans.',
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

/** Plain-language label for status badge rendering. */
export function statusBadge(s: DocStatus): { label: string; color: string } {
  switch (s) {
    case 'available':
      return { label: 'Available now', color: '#34d399' };
    case 'in-progress':
      return { label: 'In progress', color: '#fbbf24' };
    case 'coming-soon':
      return { label: 'Coming soon', color: '#9aa0a6' };
    case 'subject-to-change':
      return { label: 'Subject to change', color: '#f472b6' };
  }
}
