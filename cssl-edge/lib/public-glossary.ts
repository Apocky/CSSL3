export interface PublicGlossaryTerm {
  readonly id: string;
  readonly term: string;
  readonly meaning: string;
}

export interface PublicGlossarySymbol {
  readonly id: string;
  readonly symbol: string;
  readonly meaning: string;
}

export const PUBLIC_GLOSSARY_TERMS: readonly PublicGlossaryTerm[] = [
  {
    id: 'cssl',
    term: 'CSSL',
    meaning: 'Short for Conscious Substrate System Language. It is a programming language: a written language used to give a computer instructions.',
  },
  {
    id: 'cslv3',
    term: 'CSLv3',
    meaning: 'Version 3 of CSL. It is a compact notation for recording relationships, evidence, uncertainty, decisions, and rules.',
  },
  {
    id: 'loa',
    term: 'LoA',
    meaning: 'Short for Labyrinth of Apocalypse, an unfinished game project.',
  },
  {
    id: 'alpha',
    term: 'Alpha or early test build',
    meaning: 'An unfinished version released so people can try it and report problems. Features may be missing or change later.',
  },
  {
    id: 'account',
    term: 'Account and session',
    meaning: 'An account is the identity used to sign in. A session is the temporary signed-in connection kept by a browser until it ends or is revoked.',
  },
  {
    id: 'consent',
    term: 'Consent',
    meaning: 'A freely made, informed choice. Consent must be specific, can be withdrawn, and is not inferred from silence.',
  },
  {
    id: 'diagnostics',
    term: 'Diagnostics',
    meaning: 'Information used to find and fix a problem, such as an error message or a page-speed measurement.',
  },
  {
    id: 'telemetry',
    term: 'Telemetry',
    meaning: 'Diagnostic information that software sends automatically to another computer. On this site, optional telemetry is called optional site data and is off until a visitor saves a sharing choice.',
  },
  {
    id: 'local',
    term: 'Local',
    meaning: 'Running or stored on the computer in front of you instead of on a remote computer reached through the internet.',
  },
  {
    id: 'self-hosted',
    term: 'Self-hosted',
    meaning: 'Run on computers controlled by the person or organization operating the software, rather than handed to a separate hosted service.',
  },
  {
    id: 'api',
    term: 'API',
    meaning: 'Short for application programming interface. It is a documented way for one piece of software to request information or an action from another.',
  },
  {
    id: 'runtime',
    term: 'Runtime',
    meaning: 'The part of a program that is active while the program is running.',
  },
  {
    id: 'compiler',
    term: 'Compiler',
    meaning: 'Software that translates source code written by a person into a form a computer can run.',
  },
  {
    id: 'language-model',
    term: 'Language model',
    meaning: 'Software trained on text so it can work with language, such as continuing, classifying, or generating text.',
  },
  {
    id: 'drm',
    term: 'DRM',
    meaning: 'Short for digital rights management. It is software that restricts how a digital product can be copied, opened, or used.',
  },
  {
    id: 'eula',
    term: 'EULA',
    meaning: 'Short for End-User License Agreement. It states the terms under which someone may install or use a piece of software.',
  },
  {
    id: 'open-source',
    term: 'Open source and proprietary',
    meaning: 'Open-source software makes its source code available under a license that permits stated forms of use and change. Proprietary software is distributed under more limited terms set by its owner.',
  },
  {
    id: 'permission',
    term: 'Permission or capability',
    meaning: 'An explicit grant that allows a particular action. In technical pages, capability may refer to a permission represented in code.',
  },
  {
    id: 'provenance',
    term: 'Provenance',
    meaning: 'A record of where information or an artifact came from and how it changed.',
  },
  {
    id: 'substrate',
    term: 'Substrate',
    meaning: 'In Apocky project documents, this means a shared technical foundation used by several systems. It is a project-specific design term, not a claim that every visitor must accept.',
  },
];

export const PUBLIC_GLOSSARY_SYMBOLS: readonly PublicGlossarySymbol[] = [
  { id: 'section', symbol: '§', meaning: 'Section. It marks the start of a named part of a technical document.' },
  { id: 'not', symbol: '¬', meaning: 'Not or no. For example, “¬ harm” means “no harm.”' },
  { id: 'invariant', symbol: 't∞', meaning: 'Intended to remain true for the lifetime of the system. Technical specifications call this an invariant.' },
  { id: 'verified', symbol: '✓', meaning: 'Available or verified in the specific context where it appears.' },
  { id: 'partial', symbol: '◐', meaning: 'Partly complete or still in progress.' },
  { id: 'planned', symbol: '○', meaning: 'Planned, not yet available, or not yet verified.' },
  { id: 'important', symbol: '‼', meaning: 'Important warning or requirement.' },
  { id: 'leads-to', symbol: '→', meaning: 'Leads to, produces, or points toward.' },
  { id: 'sigma', symbol: 'Σ', meaning: 'The Greek capital letter sigma. In “Σ-mask,” it names a project-specific permission design.' },
  { id: 'omega', symbol: 'ω', meaning: 'The Greek lowercase letter omega. In “ω-field,” it names a project-specific data design.' },
];

export function filterPublicGlossary(query: string): {
  readonly terms: readonly PublicGlossaryTerm[];
  readonly symbols: readonly PublicGlossarySymbol[];
} {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return { terms: PUBLIC_GLOSSARY_TERMS, symbols: PUBLIC_GLOSSARY_SYMBOLS };

  return {
    terms: PUBLIC_GLOSSARY_TERMS.filter(({ term, meaning }) => `${term} ${meaning}`.toLocaleLowerCase().includes(normalized)),
    symbols: PUBLIC_GLOSSARY_SYMBOLS.filter(({ symbol, meaning }) => `${symbol} ${meaning}`.toLocaleLowerCase().includes(normalized)),
  };
}
