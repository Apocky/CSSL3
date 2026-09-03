import { SUPPORT_LINKS } from './support-links';

export const PUBLIC_SURFACE_AXES = ['People', 'Meaning', 'Visibility', 'Time'] as const;

export type PublicSurfaceAxis = (typeof PUBLIC_SURFACE_AXES)[number];
export type PublicSurfaceAvailability =
  | 'public'
  | 'public_read_account_write'
  | 'design_study'
  | 'external_public';
export type PublicSurfaceKind =
  | 'orientation'
  | 'community'
  | 'archive'
  | 'reference'
  | 'cosmology'
  | 'game'
  | 'support'
  | 'study'
  | 'principles'
  | 'creative_project'
  | 'language';

export type PublicSurfaceId =
  | 'home'
  | 'atlas'
  | 'start'
  | 'quests'
  | 'status'
  | 'divination'
  | 'theory-of-everything'
  | 'clearing'
  | 'akashic-records'
  | 'words'
  | 'omnoid'
  | 'labyrinth'
  | 'support'
  | 'membership'
  | 'principles'
  | 'chaos-tarot'
  | 'cssl'
  | 'cslv3'
  | 'ko-fi'
  | 'patreon';

export interface PublicSurfaceNode {
  readonly id: PublicSurfaceId;
  readonly title: string;
  readonly shortTitle: string;
  readonly eyebrow: string;
  readonly summary: string;
  readonly href: string;
  readonly action: string;
  readonly external: boolean;
  readonly availability: PublicSurfaceAvailability;
  readonly kind: PublicSurfaceKind;
  readonly axes: readonly PublicSurfaceAxis[];
  readonly coordinates: Readonly<Record<PublicSurfaceAxis, string>>;
}

export interface PublicSurfaceEdge {
  readonly source: PublicSurfaceId;
  readonly target: PublicSurfaceId;
  readonly relation: 'opens' | 'features' | 'indexes' | 'defines' | 'supports' | 'hands_off_to';
  readonly statement: string;
}

export interface PublicSurfaceRelation extends PublicSurfaceEdge {
  readonly neighbor: PublicSurfaceNode;
}

export interface PublicSurfaceFilters {
  readonly query?: string;
  readonly axis?: PublicSurfaceAxis | 'all';
  readonly kind?: PublicSurfaceKind | 'all';
  readonly availability?: PublicSurfaceAvailability | 'all';
}

export const PUBLIC_SURFACE_KIND_LABELS: Readonly<Record<PublicSurfaceKind, string>> = {
  orientation: 'Orientation',
  community: 'Community',
  archive: 'Archive',
  reference: 'Reference',
  cosmology: 'Cosmology',
  game: 'Game',
  support: 'Support',
  study: 'Design study',
  principles: 'Principles',
  creative_project: 'Creative project',
  language: 'Language',
};

export const PUBLIC_SURFACE_AVAILABILITY_LABELS: Readonly<Record<PublicSurfaceAvailability, string>> = {
  public: 'Public',
  public_read_account_write: 'Public reading · sign-in to contribute',
  design_study: 'Design study · not enrollment',
  external_public: 'External public site',
};

const koFi = SUPPORT_LINKS[0];
const patreon = SUPPORT_LINKS[1];

export const PUBLIC_SURFACE_NODES: readonly PublicSurfaceNode[] = [
  {
    id: 'home',
    title: 'Apocky',
    shortTitle: 'Home',
    eyebrow: 'Creative home',
    summary: 'The public starting point for Shawn Apocky’s projects, writing, software, and shared spaces.',
    href: '/',
    action: 'Return home',
    external: false,
    availability: 'public',
    kind: 'orientation',
    axes: ['People', 'Meaning'],
    coordinates: {
      People: 'A visitor-led starting point',
      Meaning: 'Creative home and project index',
      Visibility: 'Public',
      Time: 'Updated as the work changes',
    },
  },
  {
    id: 'atlas',
    title: 'Constellation Atlas',
    shortTitle: 'Atlas',
    eyebrow: 'Multidimensional index',
    summary: 'A visual map, complete index, and plain-language dictionary for the public Apocky constellation.',
    href: '/atlas',
    action: 'Explore the Atlas',
    external: false,
    availability: 'public',
    kind: 'orientation',
    axes: PUBLIC_SURFACE_AXES,
    coordinates: {
      People: 'Choose a destination or relationship',
      Meaning: 'Map, index, and dictionary',
      Visibility: 'Public',
      Time: 'A living orientation layer',
    },
  },
  {
    id: 'start',
    title: 'Start here',
    shortTitle: 'Start',
    eyebrow: 'Orientation protocol',
    summary: 'A clear launch point for choosing an experience, the big picture, source material, participation, or support.',
    href: '/start',
    action: 'Choose a starting signal',
    external: false,
    availability: 'public',
    kind: 'orientation',
    axes: ['People', 'Meaning', 'Time'],
    coordinates: {
      People: 'Choose the purpose that brought you here',
      Meaning: 'Guided entry into the public constellation',
      Visibility: 'Public with no quiz or email gate',
      Time: 'A starting route you can leave at any point',
    },
  },
  {
    id: 'quests',
    title: 'Public quests',
    shortTitle: 'Quests',
    eyebrow: 'Device-local expedition',
    summary: 'Eight self-directed missions through public projects, with progress kept only in the current browser.',
    href: '/quests',
    action: 'Choose a quest',
    external: false,
    availability: 'public',
    kind: 'orientation',
    axes: ['People', 'Meaning', 'Time'],
    coordinates: {
      People: 'Choose, complete, or reset your own route',
      Meaning: 'A guided traversal of public work',
      Visibility: 'Public; progress stays in this browser',
      Time: 'Eight missions with reversible local progress',
    },
  },
  {
    id: 'status',
    title: 'System status',
    shortTitle: 'Status',
    eyebrow: 'Public observatory',
    summary: 'A privacy-bounded view of public core, identity, archive, relay, and support-route availability.',
    href: '/status',
    action: 'Check system status',
    external: false,
    availability: 'public',
    kind: 'reference',
    axes: ['Visibility', 'Time'],
    coordinates: {
      People: 'Run a same-origin health check or choose a fallback',
      Meaning: 'Public status and recovery routes',
      Visibility: 'Public, without keys or private logs',
      Time: 'Observed when the page checks the health endpoint',
    },
  },
  {
    id: 'divination',
    title: 'Chaos, Tarot, and Divination',
    shortTitle: 'Divination',
    eyebrow: 'Symbolic systems · practical boundary',
    summary: 'A grounded guide to eight symbolic systems, what reflective readings can do, and what they cannot establish.',
    href: '/divination',
    action: 'Explore the divination guide',
    external: false,
    availability: 'public',
    kind: 'reference',
    axes: ['Meaning', 'Time', 'People'],
    coordinates: {
      People: 'Learn, reflect, or choose a free external reading',
      Meaning: 'Tarot and divination with an explicit epistemic boundary',
      Visibility: 'Public on apocky.com',
      Time: 'Readings inform reflection rather than guaranteeing a future',
    },
  },
  {
    id: 'theory-of-everything',
    title: 'The Theory of Everything question',
    shortTitle: 'Theory',
    eyebrow: 'Cosmology · evidence before grandeur',
    summary: 'An evidence-typed introduction to the Omnoid’s unification-sized ambition, established mathematics, open bridges, and falsifiers.',
    href: '/theory-of-everything',
    action: 'Examine the theory-sized question',
    external: false,
    availability: 'public',
    kind: 'cosmology',
    axes: ['Meaning', 'Time', 'Visibility'],
    coordinates: {
      People: 'Examine the claims and their limits',
      Meaning: 'Evidence-typed introduction to an authored cosmology',
      Visibility: 'Public; explicitly not presented as a proven physical theory',
      Time: 'Open bridges require definition, prediction, and falsification',
    },
  },
  {
    id: 'clearing',
    title: 'The Clearing',
    shortTitle: 'Clearing',
    eyebrow: 'Shared space',
    summary: 'A public community room: reading is open, while posting and reacting require sign-in.',
    href: '/clearing',
    action: 'Enter the Clearing',
    external: false,
    availability: 'public_read_account_write',
    kind: 'community',
    axes: ['People', 'Visibility', 'Time'],
    coordinates: {
      People: 'Read publicly; sign in to contribute',
      Meaning: 'Shared conversation',
      Visibility: 'Public room',
      Time: 'Live and changing',
    },
  },
  {
    id: 'akashic-records',
    title: 'Akashic Records',
    shortTitle: 'Akashic',
    eyebrow: 'Public archive',
    summary: 'Approved public writing and public-safe conversation transcripts with stable reader pages and provenance.',
    href: '/akashic-records',
    action: 'Explore the archive',
    external: false,
    availability: 'public',
    kind: 'archive',
    axes: ['Meaning', 'Visibility', 'Time'],
    coordinates: {
      People: 'Read without an account',
      Meaning: 'Published writing and conversations',
      Visibility: 'Public approved records',
      Time: 'Stable records in a growing archive',
    },
  },
  {
    id: 'words',
    title: 'Words and symbols',
    shortTitle: 'Words',
    eyebrow: 'Plain-language reference',
    summary: 'Definitions for specialist words, abbreviations, and symbols used across public Apocky pages.',
    href: '/words',
    action: 'Open the full reference',
    external: false,
    availability: 'public',
    kind: 'reference',
    axes: ['Meaning', 'Visibility'],
    coordinates: {
      People: 'For any reader who wants a definition',
      Meaning: 'Plain-language vocabulary',
      Visibility: 'Public',
      Time: 'Updated when terminology changes',
    },
  },
  {
    id: 'omnoid',
    title: 'Omnoid Singularity',
    shortTitle: 'Omnoid',
    eyebrow: 'Authored cosmology',
    summary: 'An evolving cosmology presented with explicit boundaries between authored ideas, mathematical motifs, and open hypotheses.',
    href: '/omnoid-singularity',
    action: 'Read the cosmology',
    external: false,
    availability: 'public',
    kind: 'cosmology',
    axes: ['Meaning', 'Time'],
    coordinates: {
      People: 'Read and examine the model',
      Meaning: 'Authored cosmology with evidence labels',
      Visibility: 'Public',
      Time: 'Evolving work',
    },
  },
  {
    id: 'labyrinth',
    title: 'Labyrinth of Apocalypse',
    shortTitle: 'Labyrinth',
    eyebrow: 'Game world',
    summary: 'A game shaped by procedural history, persistent consequences, strange systems, and discovery.',
    href: '/download',
    action: 'Explore the public test build',
    external: false,
    availability: 'public',
    kind: 'game',
    axes: ['People', 'Meaning', 'Time'],
    coordinates: {
      People: 'Download and try an early build',
      Meaning: 'Playable game project',
      Visibility: 'Public test build',
      Time: 'Alpha; unfinished and changing',
    },
  },
  {
    id: 'support',
    title: 'Download or support',
    shortTitle: 'Support',
    eyebrow: 'Terms before handoff',
    summary: 'The public explanation of downloads, optional support, external providers, and what a contribution does not purchase.',
    href: '/buy',
    action: 'Read the support terms',
    external: false,
    availability: 'public',
    kind: 'support',
    axes: ['People', 'Visibility'],
    coordinates: {
      People: 'Choose freely whether to contribute',
      Meaning: 'Downloads and optional support terms',
      Visibility: 'Public before any handoff',
      Time: 'Current published terms',
    },
  },
  {
    id: 'membership',
    title: 'Membership and support',
    shortTitle: 'Membership',
    eyebrow: 'The sustaining layer',
    summary: 'A public guide to the active Patreon, Ko-fi, and Chaos Tarot paths that help sustain independent work.',
    href: '/membership',
    action: 'Choose a sustaining path',
    external: false,
    availability: 'public',
    kind: 'support',
    axes: ['People', 'Meaning', 'Visibility', 'Time'],
    coordinates: {
      People: 'Choose a product, recurring patronage, direct support, or no payment',
      Meaning: 'Active external paths that sustain the work',
      Visibility: 'Public terms before every external handoff',
      Time: 'Current provider terms apply after your click',
    },
  },
  {
    id: 'principles',
    title: 'Interface principles',
    shortTitle: 'Principles',
    eyebrow: 'Public invariants',
    summary: 'Consent, sovereignty, truthful appearance, and complete stillness beneath the public spatial interface.',
    href: '/principles',
    action: 'Read the principles',
    external: false,
    availability: 'public',
    kind: 'principles',
    axes: PUBLIC_SURFACE_AXES,
    coordinates: {
      People: 'Participation remains voluntary',
      Meaning: 'Interface truth and consent boundaries',
      Visibility: 'Public',
      Time: 'Persistent design invariants',
    },
  },
  {
    id: 'chaos-tarot',
    title: 'Chaos Tarot',
    shortTitle: 'Chaos Tarot',
    eyebrow: 'External creative project',
    summary: 'An evolving symbolic-art and tarot project built around atmosphere, reflection, and authored interpretation.',
    href: 'https://chaos-tarot.com',
    action: 'Enter Chaos Tarot',
    external: true,
    availability: 'external_public',
    kind: 'creative_project',
    axes: ['People', 'Meaning', 'Time'],
    coordinates: {
      People: 'Choose whether to continue on another site',
      Meaning: 'Symbolic art, tarot, and reflection',
      Visibility: 'External public site',
      Time: 'Evolving project',
    },
  },
  {
    id: 'cssl',
    title: 'CSSL',
    shortTitle: 'CSSL',
    eyebrow: 'Public language guide',
    summary: 'A plain introduction to the CSSL programming language, with technical examples and current status.',
    href: '/docs/cssl-language',
    action: 'Read the CSSL guide',
    external: false,
    availability: 'public',
    kind: 'language',
    axes: ['Meaning', 'Time'],
    coordinates: {
      People: 'Read the public guide without an account',
      Meaning: 'Programming language and technical examples',
      Visibility: 'Public on apocky.com',
      Time: 'Current documented status',
    },
  },
  {
    id: 'cslv3',
    title: 'CSLv3',
    shortTitle: 'CSLv3',
    eyebrow: 'Plain-language symbol key',
    summary: 'Definitions for the relationships, evidence marks, uncertainty symbols, and rules used in CSLv3 notation.',
    href: '/words#symbols',
    action: 'Open the symbol key',
    external: false,
    availability: 'public',
    kind: 'language',
    axes: ['Meaning', 'Visibility'],
    coordinates: {
      People: 'Read the public symbol definitions',
      Meaning: 'Reasoning and specification notation',
      Visibility: 'Public on apocky.com',
      Time: 'Updated with the shared glossary',
    },
  },
  {
    id: 'ko-fi',
    title: koFi.name,
    shortTitle: koFi.name,
    eyebrow: 'External support relay',
    summary: `${koFi.description} The provider’s own terms and privacy policy apply after you follow the link.`,
    href: koFi.href,
    action: koFi.label,
    external: true,
    availability: 'external_public',
    kind: 'support',
    axes: ['People', 'Visibility'],
    coordinates: {
      People: 'Optional visitor-chosen contribution',
      Meaning: 'One-time or recurring support',
      Visibility: 'External provider',
      Time: 'Only contacted after your click',
    },
  },
  {
    id: 'patreon',
    title: patreon.name,
    shortTitle: patreon.name,
    eyebrow: 'External support relay',
    summary: `${patreon.description} The provider’s own terms and privacy policy apply after you follow the link.`,
    href: patreon.href,
    action: patreon.label,
    external: true,
    availability: 'external_public',
    kind: 'support',
    axes: ['People', 'Visibility', 'Time'],
    coordinates: {
      People: 'Optional visitor-chosen contribution',
      Meaning: 'Recurring support',
      Visibility: 'External provider',
      Time: 'Only contacted after your click',
    },
  },
];

export const PUBLIC_SURFACE_EDGES: readonly PublicSurfaceEdge[] = [
  { source: 'home', target: 'start', relation: 'opens', statement: 'The public home opens Start here as a guided orientation route.' },
  { source: 'home', target: 'atlas', relation: 'opens', statement: 'The public home opens the Atlas as its orientation layer.' },
  { source: 'home', target: 'clearing', relation: 'opens', statement: 'The public home opens The Clearing for shared conversation.' },
  { source: 'home', target: 'akashic-records', relation: 'features', statement: 'The public home features the approved writing archive.' },
  { source: 'home', target: 'divination', relation: 'features', statement: 'The public home features the grounded divination guide.' },
  { source: 'home', target: 'theory-of-everything', relation: 'features', statement: 'The public home features the evidence-typed Theory of Everything question.' },
  { source: 'home', target: 'omnoid', relation: 'features', statement: 'The public home features the authored cosmology.' },
  { source: 'home', target: 'labyrinth', relation: 'features', statement: 'The public home features the game project.' },
  { source: 'home', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'The public home offers a direct handoff to Chaos Tarot.' },
  { source: 'home', target: 'cssl', relation: 'opens', statement: 'The public home opens the same-origin CSSL language guide.' },
  { source: 'home', target: 'cslv3', relation: 'defines', statement: 'The public words page defines the symbols used in CSLv3 notation.' },
  { source: 'start', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'Start here offers a direct handoff to a free Chaos Tarot reading.' },
  { source: 'start', target: 'atlas', relation: 'opens', statement: 'Start here opens the Atlas for visitors who want the big picture.' },
  { source: 'start', target: 'akashic-records', relation: 'opens', statement: 'Start here opens the archive for visitors who want the source material.' },
  { source: 'start', target: 'clearing', relation: 'opens', statement: 'Start here opens The Clearing for visitors who want to participate.' },
  { source: 'start', target: 'quests', relation: 'opens', statement: 'Start here opens the device-local public quest route.' },
  { source: 'start', target: 'membership', relation: 'supports', statement: 'Start here links to the active membership and support paths.' },
  { source: 'start', target: 'divination', relation: 'opens', statement: 'Start here opens the grounded guide for visitors who want to understand divination before practicing.' },
  { source: 'start', target: 'theory-of-everything', relation: 'opens', statement: 'Start here opens the evidence-typed overview for visitors asking the largest cosmology question.' },
  { source: 'atlas', target: 'clearing', relation: 'indexes', statement: 'The Atlas indexes The Clearing as a public shared space.' },
  { source: 'atlas', target: 'akashic-records', relation: 'indexes', statement: 'The Atlas indexes the approved public archive.' },
  { source: 'atlas', target: 'words', relation: 'defines', statement: 'The Atlas dictionary uses the public words and symbols reference.' },
  { source: 'atlas', target: 'omnoid', relation: 'indexes', statement: 'The Atlas indexes the authored cosmology.' },
  { source: 'atlas', target: 'labyrinth', relation: 'indexes', statement: 'The Atlas indexes the public game page.' },
  { source: 'atlas', target: 'support', relation: 'indexes', statement: 'The Atlas indexes the published download and support terms.' },
  { source: 'atlas', target: 'membership', relation: 'indexes', statement: 'The Atlas indexes the active membership and support guide.' },
  { source: 'atlas', target: 'quests', relation: 'indexes', statement: 'The Atlas indexes the device-local public quests.' },
  { source: 'atlas', target: 'status', relation: 'indexes', statement: 'The Atlas indexes the privacy-bounded public status view.' },
  { source: 'atlas', target: 'divination', relation: 'indexes', statement: 'The Atlas indexes the grounded guide to tarot and divination systems.' },
  { source: 'atlas', target: 'theory-of-everything', relation: 'indexes', statement: 'The Atlas indexes the evidence-typed Theory of Everything overview.' },
  { source: 'atlas', target: 'principles', relation: 'indexes', statement: 'The Atlas indexes the public interface principles.' },
  { source: 'atlas', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'The Atlas offers a direct external handoff to Chaos Tarot.' },
  { source: 'atlas', target: 'cssl', relation: 'indexes', statement: 'The Atlas indexes the same-origin CSSL language guide.' },
  { source: 'atlas', target: 'cslv3', relation: 'defines', statement: 'The Atlas dictionary links to the public CSLv3 symbol key.' },
  { source: 'support', target: 'ko-fi', relation: 'supports', statement: 'The support page offers an optional external contribution through Ko-fi.' },
  { source: 'support', target: 'patreon', relation: 'supports', statement: 'The support page offers optional recurring support through Patreon.' },
  { source: 'membership', target: 'ko-fi', relation: 'supports', statement: 'Membership offers direct support through Ko-fi under the provider’s published terms.' },
  { source: 'membership', target: 'patreon', relation: 'supports', statement: 'Membership offers recurring patronage through Patreon under the provider’s published terms.' },
  { source: 'membership', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'Membership links to current Chaos Tarot product plans on the independent site.' },
  { source: 'membership', target: 'akashic-records', relation: 'opens', statement: 'Membership keeps the public archive visible as a no-payment route.' },
  { source: 'membership', target: 'clearing', relation: 'opens', statement: 'Membership keeps public reading in The Clearing visible as a no-payment route.' },
  { source: 'membership', target: 'words', relation: 'opens', statement: 'Membership keeps the public dictionary visible as a no-payment route.' },
  { source: 'quests', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'The first public quest links to a free Chaos Tarot reading.' },
  { source: 'quests', target: 'akashic-records', relation: 'opens', statement: 'A public quest asks visitors to open an approved archive record.' },
  { source: 'quests', target: 'words', relation: 'opens', statement: 'A public quest asks visitors to learn a word or symbol.' },
  { source: 'quests', target: 'omnoid', relation: 'opens', statement: 'A public quest asks visitors to examine the Omnoid evidence boundaries.' },
  { source: 'quests', target: 'labyrinth', relation: 'opens', statement: 'A public quest asks visitors to inspect the game build before downloading.' },
  { source: 'quests', target: 'clearing', relation: 'opens', statement: 'A public quest opens The Clearing while keeping participation optional.' },
  { source: 'quests', target: 'membership', relation: 'supports', statement: 'The final public quest asks visitors to review active support paths without requiring payment.' },
  { source: 'divination', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'The divination guide offers direct handoffs to free Chaos Tarot experiences.' },
  { source: 'divination', target: 'words', relation: 'defines', statement: 'The public words reference defines specialist language used across Apocky.' },
  { source: 'theory-of-everything', target: 'omnoid', relation: 'opens', statement: 'The Theory of Everything overview opens the full Omnoid synthesis.' },
  { source: 'theory-of-everything', target: 'words', relation: 'defines', statement: 'The Theory of Everything overview links to plain-language definitions for its technical terms.' },
  { source: 'status', target: 'home', relation: 'opens', statement: 'The public status view keeps the home route available as a fallback.' },
  { source: 'status', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'The public status view links to the independent Chaos Tarot recovery path.' },
  { source: 'atlas', target: 'ko-fi', relation: 'hands_off_to', statement: 'The Atlas exposes Ko-fi as an optional external support relay.' },
  { source: 'atlas', target: 'patreon', relation: 'hands_off_to', statement: 'The Atlas exposes Patreon as an optional external support relay.' },
];

const NODE_BY_ID = new Map(PUBLIC_SURFACE_NODES.map((node) => [node.id, node]));

export function getPublicSurfaceNode(id: string): PublicSurfaceNode | undefined {
  return NODE_BY_ID.get(id as PublicSurfaceId);
}

export function getPublicSurfaceRelations(id: PublicSurfaceId): readonly PublicSurfaceRelation[] {
  return PUBLIC_SURFACE_EDGES.flatMap((edge) => {
    if (edge.source !== id && edge.target !== id) return [];
    const neighborId = edge.source === id ? edge.target : edge.source;
    const neighbor = NODE_BY_ID.get(neighborId);
    return neighbor ? [{ ...edge, neighbor }] : [];
  });
}

export function findPublicSurfaceNodeForPath(pathOrHref: string): PublicSurfaceNode | undefined {
  const normalized = pathOrHref.split(/[?#]/, 1)[0] || '/';
  const exactExternal = PUBLIC_SURFACE_NODES.find((node) => node.external && node.href === normalized);
  if (exactExternal) return exactExternal;

  return [...PUBLIC_SURFACE_NODES]
    .filter((node) => !node.external)
    .sort((left, right) => right.href.length - left.href.length)
    .find((node) => (
      node.href === '/'
        ? normalized === '/'
        : normalized === node.href || normalized.startsWith(`${node.href}/`)
    ));
}

export function filterPublicSurfaceNodes(filters: PublicSurfaceFilters = {}): readonly PublicSurfaceNode[] {
  const query = filters.query?.trim().toLocaleLowerCase() ?? '';
  const axis = filters.axis ?? 'all';
  const kind = filters.kind ?? 'all';
  const availability = filters.availability ?? 'all';

  return PUBLIC_SURFACE_NODES.filter((node) => {
    if (axis !== 'all' && !node.axes.includes(axis)) return false;
    if (kind !== 'all' && node.kind !== kind) return false;
    if (availability !== 'all' && node.availability !== availability) return false;
    if (!query) return true;

    const searchable = [
      node.title,
      node.shortTitle,
      node.eyebrow,
      node.summary,
      PUBLIC_SURFACE_KIND_LABELS[node.kind],
      PUBLIC_SURFACE_AVAILABILITY_LABELS[node.availability],
      ...PUBLIC_SURFACE_AXES.map((item) => node.coordinates[item]),
    ].join(' ').toLocaleLowerCase();
    return searchable.includes(query);
  });
}

export function getExternalRelayNodes(): readonly PublicSurfaceNode[] {
  return PUBLIC_SURFACE_NODES.filter((node) => node.external);
}
