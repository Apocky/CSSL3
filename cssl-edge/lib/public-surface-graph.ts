import { SUPPORT_LINKS } from './support-links';

export const PUBLIC_SURFACE_AXES = ['People', 'Meaning', 'Visibility', 'Time'] as const;

export type PublicSurfaceAxis = (typeof PUBLIC_SURFACE_AXES)[number];
export type PublicSurfaceAvailability =
  | 'public'
  | 'public_read_account_write'
  | 'design_study'
  | 'external_public'
  | 'account_required';
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
  | 'tools'
  | 'codex-apockalypsis'
  | 'conversations'
  | 'apocrypha'
  | 'apps'
  | 'atlas'
  | 'showcase'
  | 'start'
  | 'quests'
  | 'status'
  | 'now'
  | 'labs'
  | 'memory-tools'
  | 'divination'
  | 'oracle'
  | 'spellcraft'
  | 'sigils'
  | 'spellbook'
  | 'theory-of-everything'
  | 'clearing'
  | 'akashic-records'
  | 'words'
  | 'omnoid'
  | 'labyrinth'
  | 'support'
  | 'membership'
  | 'principles'
  | 'documentation'
  | 'infinity-engine'
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
  readonly keywords?: readonly string[];
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
  design_study: 'An idea in development',
  external_public: 'Opens another site',
  account_required: 'Sign-in required',
};

const koFi = SUPPORT_LINKS[0];
const patreon = SUPPORT_LINKS[1];

export const PUBLIC_SURFACE_NODES: readonly PublicSurfaceNode[] = [
  {
    "id": "tools",
    "title": "Tools to try",
    "shortTitle": "Tools",
    "eyebrow": "Make, reflect, explore",
    "summary": "Choose a tool to make a sigil, build an intention, get a reading, or find a meaning.",
    "href": "/tools",
    "action": "Choose a tool",
    "external": false,
    "availability": "public",
    "kind": "creative_project",
    "axes": [
      "People",
      "Meaning",
      "Time"
    ],
    "coordinates": {
      "People": "Choose a tool",
      "Meaning": "Choose a tool to make a sigil, build an intention, get a reading, or find a meaning.",
      "Visibility": "Public",
      "Time": "Available to explore"
    }
  },
  {
    "id": "codex-apockalypsis",
    keywords: ['story', 'stories', 'novel', 'novels', 'fiction', 'bible', 'scripture'],
    "title": "Codex Apockalypsis · The Good Book",
    "shortTitle": "Codex Apockalypsis",
    "eyebrow": "A different telling",
    "summary": "Read The Good Book as dark fantasy and dark comedy. Explore the Omnoid, Lilith, characters, maps, and companion Codex.",
    "href": "/codex-apockalypsis",
    "action": "Begin reading",
    "external": false,
    "availability": "public",
    "kind": "archive",
    "axes": [
      "People",
      "Meaning",
      "Time"
    ],
    "coordinates": {
      "People": "Begin reading",
      "Meaning": "Read The Good Book as dark fantasy and dark comedy. Explore the Omnoid, Lilith, characters, maps, and companion Codex.",
      "Visibility": "Public",
      "Time": "Opening available; the series is being written"
    }
  },
  {
    "id": "conversations",
    "title": "Thoughts & conversations",
    "shortTitle": "Thoughts",
    "eyebrow": "Follow an idea",
    "summary": "Read short pieces about myth, meaning, creativity, consciousness, and being human.",
    "href": "/conversations",
    "action": "Explore the ideas",
    "external": false,
    "availability": "public",
    "kind": "archive",
    "axes": [
      "People",
      "Meaning",
      "Time"
    ],
    "coordinates": {
      "People": "Explore the ideas",
      "Meaning": "Read short pieces about myth, meaning, creativity, consciousness, and being human.",
      "Visibility": "Public",
      "Time": "Available to explore"
    }
  },
  {
    "id": "apocrypha",
    "title": "Talk with Apocrypha",
    "shortTitle": "Apocrypha",
    "eyebrow": "A conversation of your own",
    "summary": "Sign in to ask a question and keep your conversations together.",
    "href": "/apocrypha",
    "action": "Talk with Apocrypha",
    "external": false,
    "availability": "account_required",
    "kind": "community",
    "axes": [
      "People",
      "Meaning",
      "Time"
    ],
    "coordinates": {
      "People": "Talk with Apocrypha",
      "Meaning": "Sign in to ask a question and keep your conversations together.",
      "Visibility": "Sign-in required",
      "Time": "Available to explore"
    }
  },
  {
    "id": "apps",
    "title": "Get the Apocrypha app",
    "shortTitle": "Apocrypha app",
    "eyebrow": "Take the conversation with you",
    "summary": "Find the available app download and installation instructions.",
    "href": "/download/apocrypha",
    "action": "Get the app",
    "external": false,
    "availability": "public",
    "kind": "reference",
    "axes": [
      "People",
      "Meaning",
      "Time"
    ],
    "coordinates": {
      "People": "Get the app",
      "Meaning": "Find the available app download and installation instructions.",
      "Visibility": "Public",
      "Time": "Available to explore"
    }
  },
  {
    id: 'home',
    title: 'Apocky',
    shortTitle: 'Home',
    eyebrow: 'Creative home',
    summary: "Tools to try, words to understand, and thoughts and stories from Shawn Apocky.",
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
    title: "Browse Apocky",
    shortTitle: 'Atlas',
    eyebrow: "Find your next stop",
    summary: "Find every tool, reading, and project in one searchable directory.",
    href: '/atlas',
    action: "Browse everything",
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
    id: 'showcase',
    title: 'Apocky + Chaos Tarot Showcase',
    shortTitle: 'Showcase',
    eyebrow: 'Two doors · one constellation',
    summary: 'A captioned 23-second illustrated passage between the Apocky Atlas and the free Chaos Tarot reading experience.',
    href: '/showcase',
    action: 'Watch the showcase',
    external: false,
    availability: 'public',
    kind: 'creative_project',
    axes: PUBLIC_SURFACE_AXES,
    coordinates: {
      People: 'Watch, read the transcript, or choose a public destination',
      Meaning: 'An illustrated orientation between two independent live sites',
      Visibility: 'Public; concept art is explicitly labeled and playback is visitor-controlled',
      Time: 'A 23-second captioned film with landscape and portrait formats',
    },
  },
  {
    id: 'start',
    title: 'Start here',
    shortTitle: 'Start',
    eyebrow: "Start here",
    summary: "Pick something to make, read, or explore.",
    href: '/start',
    action: "Find a starting point",
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
    title: "Things to try",
    shortTitle: "Things to try",
    eyebrow: "Choose an adventure",
    summary: "Follow a small challenge through the tools and ideas here. Keep your progress in this browser.",
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
      Time: 'Eleven missions with reversible local progress',
    },
  },
  {
    id: 'status',
    title: "Service status",
    shortTitle: 'Status',
    eyebrow: "Help",
    summary: "Check whether a service is available and find help if something is not working.",
    href: '/status',
    action: "Check availability",
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
    id: 'oracle',
    title: 'Yes / No Oracle',
    shortTitle: 'Oracle',
    eyebrow: "Chaos Tarot · Sign-in required",
    summary: "Ask a yes-or-no question on Chaos Tarot. Sign-in is required.",
    href: "https://chaos-tarot.com/yes-no",
    action: "Open Yes / No Oracle",
    external: true,
    availability: "account_required",
    kind: 'game',
    axes: PUBLIC_SURFACE_AXES,
    coordinates: {
      People: 'You ask; the generated signal does not take decision authority',
      Meaning: 'Yes or no as reflective friction, not guaranteed prediction',
      Visibility: 'Opens Chaos Tarot, which requires its own sign-in',
      Time: 'Use a reading as a prompt to consider your next choice',
    },
  },
  {
    id: 'spellcraft',
    title: "Build an intention",
    shortTitle: "Spellcraft",
    eyebrow: "Words for what matters",
    summary: "Choose words for growth, protection, change, and more. Create an intention and turn it into a sigil.",
    href: '/spellcraft',
    action: "Build an intention",
    external: false,
    availability: 'public',
    kind: 'language',
    axes: PUBLIC_SURFACE_AXES,
    coordinates: {
      People: 'You author and inspect every term before choosing to save',
      Meaning: 'Language becomes a non-executable symbolic plan and interpretation',
      Visibility: 'Input stays local; unknown and ambiguous forms are quarantined',
      Time: 'Versioned engine, vocabulary, hashes, and reproducible receipt',
    },
  },
  {
    id: 'sigils',
    title: "Make a sigil",
    shortTitle: "Sigil maker",
    eyebrow: "Make a symbol",
    summary: "Choose a meaning, create your symbol, and download the image.",
    href: '/sigils',
    action: "Make a sigil",
    external: false,
    availability: 'public',
    kind: 'creative_project',
    axes: PUBLIC_SURFACE_AXES,
    coordinates: {
      People: 'Choose the text and visible variant; download only when ready',
      Meaning: 'Morphemes map to disclosed rings, paths, and nodes',
      Visibility: 'No hidden payload, remote generation, or private publication',
      Time: 'The same validated program and variant reproduce the same image',
    },
  },
  {
    id: 'spellbook',
    title: "Your spellbook",
    shortTitle: "Spellbook",
    eyebrow: "Your saved creations",
    summary: "Keep and revisit the symbols and intentions you saved in this browser.",
    href: '/spellbook',
    action: "Open your spellbook",
    external: false,
    availability: 'public',
    kind: 'archive',
    axes: PUBLIC_SURFACE_AXES,
    coordinates: {
      People: 'You choose each save, import, export, and deletion',
      Meaning: 'A personal collection of versioned symbolic artifacts',
      Visibility: 'Private to this browser unless you export a file',
      Time: 'Receipts preserve engine and vocabulary lineage across versions',
    },
  },
  {
    id: 'theory-of-everything',
    title: 'The Theory of Everything question',
    shortTitle: 'Theory',
    eyebrow: "Big questions",
    summary: "Explore a question about how everything might fit together, with examples and ideas to consider.",
    href: '/theory-of-everything',
    action: "Explore the question",
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
    shortTitle: "Community",
    eyebrow: "The Clearing",
    summary: "Read the community conversation. Sign in when you want to join in.",
    href: '/clearing',
    action: "Visit the community",
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
    title: "Essays & writing",
    shortTitle: "Essays & writing",
    eyebrow: "Shawn’s writing",
    summary: "Search the published essays and writing by Shawn Apocky, or browse the conversation archive.",
    href: '/akashic-records',
    action: "Read the essays",
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
    title: "Words & meanings",
    shortTitle: "Words & meanings",
    eyebrow: "Find a meaning",
    summary: "Look up a word or symbol and get a plain-language definition.",
    href: '/words',
    action: "Find a definition",
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
    eyebrow: "Worlds and possibilities",
    summary: "Explore Shawn’s Omnoid cosmology through stories, concepts, and illustrations.",
    href: '/omnoid-singularity',
    action: "Explore the Omnoid",
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
    eyebrow: "A game in progress",
    summary: "Explore Labyrinth of Apocalypse and the available game download.",
    href: '/download',
    action: "Explore the game",
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
    title: "Downloads & support",
    shortTitle: "Downloads & support",
    eyebrow: "Help the work grow",
    summary: "Find the game download and ways to support its development.",
    href: '/buy',
    action: "See your options",
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
    shortTitle: "Support the work",
    eyebrow: "Optional support",
    summary: "Help fund new writing, art, tools, and experiments.",
    href: '/membership',
    action: "Support the work",
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
    eyebrow: "What matters here",
    summary: "Read the principles behind the site and the choices you can make here.",
    href: '/principles',
    action: "Read the principles",
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
    id: 'now',
    title: "What’s new",
    shortTitle: "What’s new",
    eyebrow: "Around here",
    summary: "See what is ready to try and what Shawn is working on.",
    href: '/now',
    action: "See what’s new",
    external: false,
    availability: 'public',
    kind: 'orientation',
    axes: PUBLIC_SURFACE_AXES,
    coordinates: {
      People: 'Choose a currently usable path without decoding a roadmap',
      Meaning: 'Public capability and maturity ledger',
      Visibility: 'Public; plans and live behavior remain distinct',
      Time: 'A current-state orientation surface',
    },
  },
  {
    id: 'labs',
    title: "Experiments",
    shortTitle: "Experiments",
    eyebrow: "Try something different",
    summary: "Find small creative experiments and tools you can try.",
    href: '/labs',
    action: "Explore experiments",
    external: false,
    availability: 'public',
    kind: 'orientation',
    axes: PUBLIC_SURFACE_AXES,
    coordinates: {
      People: 'Try, inspect, or leave each experiment freely',
      Meaning: 'Usable experiments and design studies',
      Visibility: 'Public interfaces with explicit maturity labels',
      Time: 'Experiments may change or report a degraded dependency',
    },
  },
  {
    id: 'memory-tools',
    title: "Find your saved work",
    shortTitle: "Saved work",
    eyebrow: "Pick up where you left off",
    summary: "Find your saved creations, conversations, and reading.",
    href: '/memory-tools',
    action: "Find your saved work",
    external: false,
    availability: 'public',
    kind: 'orientation',
    axes: PUBLIC_SURFACE_AXES,
    coordinates: {
      People: 'Choose a task first; signed-in controls bind only to your server-derived profile',
      Meaning: 'Plain-language previews, public knowledge, local tools, and private recall',
      Visibility: 'Public directory; device-local saves stay local; Mneme requires verified sign-in',
      Time: 'Current routing and capability checks, with export, correction, and forgetting when ready',
    },
  },
  {
    id: 'documentation',
    title: "Guides",
    shortTitle: "Guides",
    eyebrow: "A little help",
    summary: "Find explanations and guides for the projects and tools here.",
    href: '/docs',
    action: "Read a guide",
    external: false,
    availability: 'public',
    kind: 'reference',
    axes: ['Meaning', 'Visibility', 'Time'],
    coordinates: {
      People: 'Begin with ordinary explanations and choose deeper detail',
      Meaning: 'Guides, status labels, and technical references',
      Visibility: 'Public documentation',
      Time: 'Each guide carries its current status',
    },
  },
  {
    id: 'infinity-engine',
    title: 'Infinity Engine research',
    shortTitle: 'Infinity',
    eyebrow: "An idea in development",
    summary: "Read about an experimental approach to connecting worlds and creative projects.",
    href: '/infinity-engine',
    action: "Explore the idea",
    external: false,
    availability: 'design_study',
    kind: 'study',
    axes: ['Meaning', 'Time', 'Visibility'],
    coordinates: {
      People: 'Inspect the evidence forms without treating plans as products',
      Meaning: 'Shared architecture research',
      Visibility: 'Public design study',
      Time: 'Incomplete research connected to current projects',
    },
  },
  {
    id: 'chaos-tarot',
    title: 'Chaos Tarot',
    shortTitle: 'Chaos Tarot',
    eyebrow: 'External creative project',
    summary: "Get a free tarot reading, explore the cards, or find a new way to reflect on a question.",
    href: "https://chaos-tarot.com/free-reading?source=apocky-directory",
    action: "Get a free reading",
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
    summary: "A guide to the CSSL programming language, for readers who want to explore its technical details.",
    href: '/docs/cssl-language',
    action: "Read the language guide",
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
    summary: "Look up the symbols used in Shawn’s compact notation.",
    href: '/words#symbols',
    action: "See the symbol key",
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
  {"source":"home","target":"tools","relation":"opens","statement":"The home page links to Tools."},
  {"source":"atlas","target":"tools","relation":"indexes","statement":"The directory lists Tools."},
  {"source":"home","target":"codex-apockalypsis","relation":"opens","statement":"The home page links to Codex Apockalypsis."},
  {"source":"atlas","target":"codex-apockalypsis","relation":"indexes","statement":"The directory lists Codex Apockalypsis."},
  {"source":"home","target":"conversations","relation":"opens","statement":"The home page links to Thoughts."},
  {"source":"atlas","target":"conversations","relation":"indexes","statement":"The directory lists Thoughts."},
  {"source":"home","target":"apocrypha","relation":"opens","statement":"The home page links to Apocrypha."},
  {"source":"atlas","target":"apocrypha","relation":"indexes","statement":"The directory lists Apocrypha."},
  {"source":"home","target":"apps","relation":"opens","statement":"The home page links to Apocrypha app."},
  {"source":"atlas","target":"apps","relation":"indexes","statement":"The directory lists Apocrypha app."},
  {"source":"codex-apockalypsis","target":"omnoid","relation":"opens","statement":"The Codex offers a story set in the Omnoid."},
  {"source":"conversations","target":"akashic-records","relation":"opens","statement":"The ideas page links to Shawn’s published writing."},
  { source: 'home', target: 'start', relation: 'opens', statement: 'The public home opens Start here as a guided orientation route.' },
  { source: 'home', target: 'atlas', relation: 'opens', statement: 'The public home opens the Atlas as its orientation layer.' },
  { source: 'home', target: 'showcase', relation: 'features', statement: 'The public home features the captioned Apocky and Chaos Tarot connected-worlds showcase.' },
  { source: 'home', target: 'clearing', relation: 'opens', statement: 'The public home opens The Clearing for shared conversation.' },
  { source: 'home', target: 'akashic-records', relation: 'features', statement: 'The public home features the approved writing archive.' },
  { source: 'home', target: 'divination', relation: 'features', statement: 'The public home features the grounded divination guide.' },
  { source: 'home', target: 'oracle', relation: 'features', statement: 'The home directory links to the Yes / No Oracle on Chaos Tarot.' },
  { source: 'home', target: 'spellcraft', relation: 'features', statement: 'The public home features the symbolic compiler, sigil, and local Spellbook loop.' },
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
  { source: 'start', target: 'oracle', relation: 'opens', statement: 'Start here links to the Yes / No Oracle on Chaos Tarot.' },
  { source: 'start', target: 'spellcraft', relation: 'opens', statement: 'Start here opens the symbolic creation workbench.' },
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
  { source: 'atlas', target: 'oracle', relation: 'indexes', statement: 'The Atlas indexes the private, device-local Yes / No Oracle.' },
  { source: 'atlas', target: 'spellcraft', relation: 'indexes', statement: 'The Atlas indexes the deterministic symbolic spellcraft workbench.' },
  { source: 'atlas', target: 'sigils', relation: 'indexes', statement: 'The Atlas indexes the deterministic visible sigil studio.' },
  { source: 'atlas', target: 'spellbook', relation: 'indexes', statement: 'The Atlas indexes the route to the private local Spellbook without indexing its entries.' },
  { source: 'atlas', target: 'theory-of-everything', relation: 'indexes', statement: 'The Atlas indexes the evidence-typed Theory of Everything overview.' },
  { source: 'atlas', target: 'principles', relation: 'indexes', statement: 'The Atlas indexes the public interface principles.' },
  { source: 'atlas', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'The Atlas offers a direct external handoff to Chaos Tarot.' },
  { source: 'atlas', target: 'cssl', relation: 'indexes', statement: 'The Atlas indexes the same-origin CSSL language guide.' },
  { source: 'atlas', target: 'cslv3', relation: 'defines', statement: 'The Atlas dictionary links to the public CSLv3 symbol key.' },
  { source: 'atlas', target: 'showcase', relation: 'indexes', statement: 'The Atlas indexes the public video showcase and its explicit visual truth boundary.' },
  { source: 'showcase', target: 'oracle', relation: 'opens', statement: 'The showcase opens the private, device-local Yes / No Oracle as a visitor-chosen next step.' },
  { source: 'showcase', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'The showcase offers a direct handoff to a free Chaos Tarot reading.' },
  { source: 'showcase', target: 'membership', relation: 'supports', statement: 'The showcase links to active support paths only after presenting useful no-payment routes.' },
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
  { source: 'quests', target: 'oracle', relation: 'opens', statement: 'A public quest asks visitors to use a bounded Yes / No signal without ceding decision authority.' },
  { source: 'quests', target: 'spellcraft', relation: 'opens', statement: 'A public quest asks visitors to compile and inspect one symbolic working.' },
  { source: 'quests', target: 'sigils', relation: 'opens', statement: 'A public quest asks visitors to craft and download deterministic visible geometry.' },
  { source: 'divination', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'The divination guide offers direct handoffs to free Chaos Tarot experiences.' },
  { source: 'divination', target: 'words', relation: 'defines', statement: 'The public words reference defines specialist language used across Apocky.' },
  { source: 'divination', target: 'oracle', relation: 'opens', statement: 'The divination guide opens the bounded Yes / No Oracle as a quick reflective tool.' },
  { source: 'divination', target: 'spellcraft', relation: 'opens', statement: 'The divination guide opens symbolic spellcraft with the same epistemic boundary.' },
  { source: 'oracle', target: 'spellcraft', relation: 'opens', statement: 'The Oracle can hand a reaction onward to the more inspectable symbolic language workbench.' },
  { source: 'oracle', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'The Oracle offers a visitor-chosen handoff to a deeper free Chaos Tarot reading.' },
  { source: 'spellcraft', target: 'sigils', relation: 'defines', statement: 'A validated symbolic program deterministically defines visible sigil geometry.' },
  { source: 'spellcraft', target: 'spellbook', relation: 'opens', statement: 'Spellcraft offers explicit local saving into the private browser Spellbook.' },
  { source: 'spellcraft', target: 'membership', relation: 'supports', statement: 'After delivering the tool, Spellcraft offers optional routes to sustain its development.' },
  { source: 'sigils', target: 'spellbook', relation: 'opens', statement: 'The Sigil Studio offers explicit local saving of validated source artifacts.' },
  { source: 'sigils', target: 'ko-fi', relation: 'supports', statement: 'After generating and downloading a sigil, the studio offers an optional Ko-fi handoff.' },
  { source: 'spellbook', target: 'membership', relation: 'supports', statement: 'The local Spellbook offers an optional path to sustain continued engine development.' },
  { source: 'spellbook', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'The local Spellbook offers a visitor-chosen handoff to deeper live divination.' },
  { source: 'theory-of-everything', target: 'omnoid', relation: 'opens', statement: 'The Theory of Everything overview opens the full Omnoid synthesis.' },
  { source: 'theory-of-everything', target: 'words', relation: 'defines', statement: 'The Theory of Everything overview links to plain-language definitions for its technical terms.' },
  { source: 'status', target: 'home', relation: 'opens', statement: 'The public status view keeps the home route available as a fallback.' },
  { source: 'status', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'The public status view links to the independent Chaos Tarot recovery path.' },
  { source: 'atlas', target: 'ko-fi', relation: 'hands_off_to', statement: 'The Atlas exposes Ko-fi as an optional external support relay.' },
  { source: 'atlas', target: 'patreon', relation: 'hands_off_to', statement: 'The Atlas exposes Patreon as an optional external support relay.' },
  { source: 'home', target: 'now', relation: 'opens', statement: 'The public home opens the current-state ledger for a fast truthful orientation.' },
  { source: 'home', target: 'labs', relation: 'opens', statement: 'The public home opens the experiment deck for visitors who want to operate the machinery.' },
  { source: 'atlas', target: 'now', relation: 'indexes', statement: 'The Atlas indexes the current-state and maturity ledger.' },
  { source: 'atlas', target: 'labs', relation: 'indexes', statement: 'The Atlas indexes the public experiment deck.' },
  { source: 'atlas', target: 'documentation', relation: 'indexes', statement: 'The Atlas indexes the public documentation library.' },
  { source: 'atlas', target: 'infinity-engine', relation: 'indexes', statement: 'The Atlas indexes the shared-architecture design study.' },
  { source: 'now', target: 'status', relation: 'opens', statement: 'The current-state ledger opens a live bounded health probe.' },
  { source: 'now', target: 'labs', relation: 'opens', statement: 'The current-state ledger separates experimental work into the public lab.' },
  { source: 'now', target: 'akashic-records', relation: 'features', statement: 'The current-state ledger identifies the public archive as available now.' },
  { source: 'now', target: 'labyrinth', relation: 'features', statement: 'The current-state ledger identifies the downloadable Labyrinth alpha as available now.' },
  { source: 'now', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'The current-state ledger hands off to the independent live Chaos Tarot product.' },
  { source: 'now', target: 'membership', relation: 'supports', statement: 'The current-state ledger opens truthful active support paths.' },
  { source: 'labs', target: 'quests', relation: 'features', statement: 'The lab features the device-local quest engine.' },
  { source: 'labs', target: 'status', relation: 'features', statement: 'The lab features the public status probe.' },
  { source: 'labs', target: 'infinity-engine', relation: 'features', statement: 'The lab features shared-architecture research as a design study.' },
  { source: 'labs', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'The lab links to Chaos Tarot as an independent live interactive product.' },
  { source: 'labs', target: 'oracle', relation: 'features', statement: 'The lab features the private Yes / No Oracle.' },
  { source: 'labs', target: 'spellcraft', relation: 'features', statement: 'The lab features the deterministic symbolic compiler and interpreter.' },
  { source: 'labs', target: 'sigils', relation: 'features', statement: 'The lab features reproducible morphology-derived sigil geometry.' },
  { source: 'labs', target: 'spellbook', relation: 'features', statement: 'The lab features the explicit device-local Spellbook.' },
  { source: 'documentation', target: 'cssl', relation: 'defines', statement: 'The documentation library contains the public CSSL guide.' },
  { source: 'documentation', target: 'cslv3', relation: 'defines', statement: 'The documentation library points to the shared CSLv3 notation key.' },
  { source: 'documentation', target: 'infinity-engine', relation: 'opens', statement: 'The documentation library opens the shared-architecture research overview.' },
  { source: 'documentation', target: 'labyrinth', relation: 'opens', statement: 'The documentation library explains the public Labyrinth build.' },
  { source: 'infinity-engine', target: 'cssl', relation: 'opens', statement: 'Infinity Engine research identifies CSSL as a connected language project.' },
  { source: 'infinity-engine', target: 'labyrinth', relation: 'opens', statement: 'Infinity Engine research identifies Labyrinth as a connected game and engine test.' },
  { source: 'home', target: 'memory-tools', relation: 'opens', statement: 'The public home opens the memory-bank and tool directory.' },
  { source: 'atlas', target: 'memory-tools', relation: 'indexes', statement: 'The Atlas indexes a task-first guide to public, device-local, and signed-in private memory.' },
  { source: 'memory-tools', target: 'akashic-records', relation: 'indexes', statement: 'The directory identifies Akashic Records as approved public memory.' },
  { source: 'memory-tools', target: 'words', relation: 'indexes', statement: 'The directory identifies the dictionary as shared semantic memory.' },
  { source: 'memory-tools', target: 'documentation', relation: 'indexes', statement: 'The directory identifies documentation as reference memory.' },
  { source: 'memory-tools', target: 'quests', relation: 'features', statement: 'The directory identifies quests as a tool with device-local reversible progress.' },
  { source: 'memory-tools', target: 'status', relation: 'features', statement: 'The directory identifies the public status probe as an observable tool.' },
  { source: 'memory-tools', target: 'divination', relation: 'features', statement: 'The directory identifies the divination comparator as a bounded interpretive tool.' },
  { source: 'memory-tools', target: 'spellbook', relation: 'indexes', statement: 'The directory identifies the Spellbook as private, user-controlled device memory.' },
  { source: 'memory-tools', target: 'spellcraft', relation: 'features', statement: 'The directory identifies Spellcraft as a deterministic symbolic instrument.' },
  { source: 'memory-tools', target: 'clearing', relation: 'opens', statement: 'The directory distinguishes public dialogue in the Clearing from private profile memory.' },
  { source: 'memory-tools', target: 'chaos-tarot', relation: 'hands_off_to', statement: 'The directory offers an intentional handoff to the independent live Chaos Tarot system.' },
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
      ...(node.keywords ?? []),
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
