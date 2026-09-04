import { createHash } from 'node:crypto';
import { mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { sanitizePublicConversationText } from './snapshot-akashic-records.mjs';

const SCHEMA = 'apocky.public-conversation-corpus.v1';
const EXPECTED_CHATGPT = 1_230;
const EXPECTED_CLAUDE = 156;
const EXPECTED_UNIQUE = EXPECTED_CHATGPT + EXPECTED_CLAUDE;
const GENERATED_AT = '2026-09-03';
const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const outputRoot = join(repositoryRoot, 'public', 'conversation-corpus');
const approvedRecordRoot = join(outputRoot, 'approved-records');

const REALMS = Object.freeze({
  'Spiritual life': ['The Violet Archive', 'lantern'],
  'Myth and meaning': ['The Unwritten Pantheon', 'mask'],
  Consciousness: ['The Hall of Many Minds', 'mirror'],
  Divination: ['The Signal Sea', 'oracle-bone'],
  'Creative practice': ['The Forge of Names', 'ember'],
  Sovereignty: ['The Gate Without a Keeper', 'key'],
  'Ordinary life': ['The Hearth of Small Gods', 'keepsake'],
  Relationships: ['The Constellation of Near Things', 'thread'],
  'Games and worlds': ['The Labyrinth Between', 'map-fragment'],
  'Science and systems': ['The Omnoid Engine', 'gear'],
  Unclassified: ['The Deep Index', 'unread shard'],
});

const LORE_FRAGMENTS = Object.freeze({
  'Spiritual life': [
    'The lantern would not answer. It only made the next honest step visible, which was answer enough.',
    'Pilgrims brought names for the light; the light kept none of them and warmed every hand the same.',
    'At the shrine with no keeper, belief was offered as a question and returned as a practice.',
  ],
  'Myth and meaning': [
    'Every mask in the archive had an open back. The face beneath remained free to leave the story.',
    'The old gods argued over the map until a mortal folded it into a new shape and found another road.',
    'A legend survived by changing meaning whenever someone brave enough read it without obedience.',
  ],
  Consciousness: [
    'The mirror remembered each witness differently, yet the silver beneath every image remained continuous.',
    'Many small voices kept the observatory alive; unity was the rhythm of their listening, not their silence.',
    'The mind crossed its own threshold and found the watcher had always been one room among many.',
  ],
  Divination: [
    'The oracle refused certainty. It offered a pattern, a pause, and the responsibility of choosing afterward.',
    'A card turned itself face-up in the tide, but the hand that interpreted it still belonged to the seeker.',
    'The omen pointed in two directions so the traveler would remember that judgment cannot be outsourced.',
  ],
  'Creative practice': [
    'The forge accepted unfinished names. Each strike removed a borrowed shape and revealed a possible one.',
    'An ember learned the difference between consuming a world and illuminating the workbench.',
    'The maker left one seam visible so every future hand could tell where invention had entered the artifact.',
  ],
  Sovereignty: [
    'The gate had no keeper because permission could not be granted by anyone standing outside the self.',
    'A key was made that opened nothing without the holder turning it; this was its greatest enchantment.',
    'The covenant fit in one line: every power stops where another will begins.',
  ],
  'Ordinary life': [
    'The smallest god lived beside the kettle and measured miracles in meals, sleep, and returning tomorrow.',
    'A receipt became a save point because someone chose to remember enoughness while it was present.',
    'No prophecy arrived that morning. The hearth was lit anyway, and the day became inhabitable.',
  ],
  Relationships: [
    'A thread joined two distant lights without claiming either star as property.',
    'The constellation changed whenever one person moved; care meant redrawing the map without erasing them.',
    'Two voices crossed the dark. What endured was not agreement, but the space each left for the other.',
  ],
  'Games and worlds': [
    'The labyrinth recorded every abandoned route as knowledge, not failure.',
    'A map fragment showed no destination; held beside another fragment, it revealed a choice of worlds.',
    'The player found the hidden rule written beneath the quest: the world changes when attention does.',
  ],
  'Science and systems': [
    'The engine kept a ledger of what it knew, what it inferred, and what one failed test could still undo.',
    'A gear refused to turn on confidence alone. Evidence gave it teeth; revision kept it from breaking.',
    'The machine became intelligible when every signal could trace its path home.',
  ],
  Unclassified: [
    'The unread shard was not empty. It waited for a neighboring idea to reveal which edge could connect.',
    'An index without a shelf kept wandering until someone asked it a human question.',
    'The archive preserved the fragment without pretending the missing context had been recovered.',
  ],
});

const CATEGORY_THEME = Object.freeze({
  'Spirituality & Mysticism': 'Spiritual life',
  'Philosophy & Psychology': 'Consciousness',
  'Relationships & Personal': 'Relationships',
  'Creative Writing & Worldbuilding': 'Creative practice',
  'Gaming & Game Dev': 'Games and worlds',
  'Mythology & Folklore': 'Myth and meaning',
  'Physics & Mathematics': 'Science and systems',
  'Religion & Sacred Texts': 'Spiritual life',
  'Health & Wellness': 'Ordinary life',
  'Numerology & Astrology': 'Divination',
  'Practical & Daily Life': 'Ordinary life',
  'AI & Technology': 'Science and systems',
  'Science & Nature': 'Science and systems',
  'Elder Scrolls Lore (TES)': 'Games and worlds',
  Uncategorized: 'Unclassified',
});

const THEME_PATTERNS = [
  ['Spiritual life', /\b(?:spirit|soul|sacred|mystic|gnostic|divine|ritual|prayer|deity|god|goddess|religion|meditat)/i],
  ['Divination', /\b(?:tarot|oracle|rune|astrolog|numerolog|divination|omen|synchronicit|sigil|i ching)/i],
  ['Myth and meaning', /\b(?:myth|archetype|allegor|symbol|legend|folklore|story|narrative|pantheon)/i],
  ['Consciousness', /\b(?:conscious|mind|identity|selfhood|reality|perception|cognition|dream|psyche)/i],
  ['Sovereignty', /\b(?:consent|sovereign|autonomy|freedom|agency|boundary|control|coerc)/i],
  ['Creative practice', /\b(?:create|writing|novel|art|music|poem|design|worldbuild|imagin)/i],
  ['Games and worlds', /\b(?:game|quest|labyrinth|player|level|deck|card|rpg|dungeon)/i],
  ['Science and systems', /\b(?:physics|math|system|machine|code|software|network|theory|model|engine|computer)/i],
  ['Relationships', /\b(?:relationship|partner|wife|husband|friend|family|daughter|son|mother|father|love)/i],
  ['Ordinary life', /\b(?:work|food|home|sleep|day|daily|money|job|health|body|morning|night)/i],
];

const WARNING_PATTERNS = [
  ['health or medication', /\b(?:diagnos|medication|medicine|doctor|therap|psychiatr|hospital|symptom|dose|withdrawal|suicid|self[- ]?harm)/i],
  ['substance use', /\b(?:cocaine|meth|heroin|fentanyl|psychedelic|marijuana|cannabis|drug use|getting high|overdose)/i],
  ['relationship privacy', /\b(?:my wife|my husband|my girlfriend|my boyfriend|my partner|my daughter|my son|custody|divorce|breakup)/i],
  ['sexual content', /\b(?:explicit sex|pornograph|nudes?|sexual assault|rape)\b/i],
  ['violence or death', /\b(?:murder|kill myself|suicide|blood for|torture|assault|dead body|death threat)/i],
  ['financial details', /\b(?:credit card|debit card|bank account|routing number|account number|paid with|card ends)/i],
];

const THIRD_PARTY_CONTEXT = /\b(?:(?:my|our)\s+(?:wife|husband|partner|girlfriend|boyfriend|daughter|son|mother|father|sister|brother|friend|coworker|boss|doctor|therapist|lawyer|neighbor)|(?:texts?|messages?|conversation|email|letter)\s+(?:with|from|to)|(?:person|friend|woman|man|girl|guy|coworker|neighbor)\s+(?:named|called))\b/i;
const SAFE_PROPER_NAMES = new Set([
  'AI', 'Apocky', 'ChatGPT', 'Claude', 'Codex', 'Lirael', 'Solaris', 'Shawn',
  'Hecate', 'Morrigan', 'Morrígan', 'Jesus', 'Yeshua', 'Sophia', 'Lilith',
  'Babel', 'Ouroboros', 'Ouroboroid', 'Omnoid', 'Tarot', 'I', 'The',
]);

const MANUAL_RIGHTS_HOLDS = new Map([
  ['2fcef8d9181c426e5848:1', 'timestamped third-party transcript candidate'],
  ['9627131654a58687f3dc:1', 'timestamped third-party transcript candidate'],
  ['cbf3244d23b8ef21a788:3', 'timestamped transcript candidate'],
  ['15d54f61a9ce3443f536:792', 'duplicated timestamped transcript candidate'],
  ['15d54f61a9ce3443f536:793', 'duplicated timestamped transcript candidate'],
  ['c821a6ac812846d89306:5', 'attributed third-party book passage'],
  ['527e251bd8cf70830bf5:1', 'third-party song quotation corpus candidate'],
]);

const MANUAL_PRIVACY_HOLDS = new Set([
  '8da4cf79ce27e0b0f751:1', '8da4cf79ce27e0b0f751:5', '8da4cf79ce27e0b0f751:6', '8da4cf79ce27e0b0f751:8',
  '207359f54040216cdd4f:1', '207359f54040216cdd4f:2', '207359f54040216cdd4f:3', '207359f54040216cdd4f:4',
  ...Array.from({ length: 16 }, (_, index) => `2412b5bb87c4187657c4:${index + 1}`),
  '13757f7d9e4728a1da62:1', '13757f7d9e4728a1da62:3',
]);

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function parseArgs(argv) {
  const result = { check: false };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--check') result.check = true;
    else if (argument === '--chatgpt-dir') result.chatgptDir = argv[++index];
    else if (argument === '--claude-json') result.claudeJson = argv[++index];
    else if (argument === '--categories') result.categories = argv[++index];
    else throw new Error(`Unknown argument: ${argument}`);
  }
  result.chatgptDir ??= process.env.APOCKY_CHATGPT_EXPORT_DIR;
  result.claudeJson ??= process.env.APOCKY_CLAUDE_CONVERSATIONS_JSON;
  result.categories ??= process.env.APOCKY_CHATGPT_CATEGORIES_CSV;
  for (const [key, label] of [
    ['chatgptDir', '--chatgpt-dir'],
    ['claudeJson', '--claude-json'],
    ['categories', '--categories'],
  ]) {
    if (typeof result[key] !== 'string' || result[key].length === 0) throw new Error(`Provide ${label}`);
  }
  return result;
}

function csvRows(input) {
  const rows = [];
  let row = [];
  let cell = '';
  let quoted = false;
  for (let index = 0; index < input.length; index += 1) {
    const char = input[index];
    if (quoted) {
      if (char === '"' && input[index + 1] === '"') {
        cell += '"';
        index += 1;
      } else if (char === '"') quoted = false;
      else cell += char;
    } else if (char === '"') quoted = true;
    else if (char === ',') {
      row.push(cell);
      cell = '';
    } else if (char === '\n') {
      row.push(cell.replace(/\r$/u, ''));
      rows.push(row);
      row = [];
      cell = '';
    } else cell += char;
  }
  if (cell.length > 0 || row.length > 0) {
    row.push(cell);
    rows.push(row);
  }
  return rows;
}

function categoryLookup(csv) {
  const rows = csvRows(csv);
  const header = rows.shift() ?? [];
  const indexes = Object.fromEntries(header.map((value, index) => [value, index]));
  const byTitle = new Map();
  for (const row of rows) {
    const category = row[indexes.Category] ?? 'Uncategorized';
    const date = row[indexes.Date] ?? '';
    const title = (row[indexes.Title] ?? '').trim();
    const key = title.toLocaleLowerCase();
    const queue = byTitle.get(key) ?? [];
    queue.push({ category, date, claimed: false });
    byTitle.set(key, queue);
  }
  return byTitle;
}

function takeCategory(categories, title, dates) {
  const queue = categories.get(title.trim().toLocaleLowerCase()) ?? [];
  const exact = queue.find((candidate) => !candidate.claimed && dates.includes(candidate.date));
  const selected = exact ?? queue.find((candidate) => !candidate.claimed);
  if (!selected) return { category: 'Uncategorized', provenance: 'category fallback after unmatched owner CSV title' };
  selected.claimed = true;
  return {
    category: selected.category,
    provenance: exact
      ? 'owner-maintained categorized export CSV · date + title match'
      : 'owner-maintained categorized export CSV · unique/fallback title match',
  };
}

function dateFromEpoch(value) {
  if (!Number.isFinite(value) || value <= 0) return '';
  return new Date(value * 1_000).toISOString().slice(0, 10);
}

function extractText(content) {
  if (content === null || typeof content !== 'object') return '';
  const parts = Array.isArray(content.parts) ? content.parts : [];
  const text = parts.flatMap((part) => {
    if (typeof part === 'string') return [part];
    if (part && typeof part === 'object') {
      if (typeof part.text === 'string') return [part.text];
      if (typeof part.caption === 'string') return [part.caption];
    }
    return [];
  }).filter((part) => part.trim().length > 0).join('\n\n');
  if (text.length > 0) return text;
  return typeof content.text === 'string' ? content.text : '';
}

function redactMatches(text, pattern, label) {
  let count = 0;
  const value = text.replace(pattern, () => {
    count += 1;
    return `[redacted:${label}]`;
  });
  return { value, count };
}

function redactContextualNames(text) {
  if (!THIRD_PARTY_CONTEXT.test(text)) return { value: text, count: 0 };
  let count = 0;
  const value = text.replace(/\b[A-Z][a-zÀ-ÖØ-öø-ÿ'’-]{2,}\b/gu, (candidate, offset) => {
    if (SAFE_PROPER_NAMES.has(candidate)) return candidate;
    const context = text.slice(Math.max(0, offset - 96), Math.min(text.length, offset + candidate.length + 96));
    if (!THIRD_PARTY_CONTEXT.test(context)) return candidate;
    count += 1;
    return '[redacted:third-party-name]';
  });
  return { value, count };
}

function sanitize(value) {
  let source = String(value);
  let redactionCount = 0;
  const prePatterns = [
    [/(?<![A-Za-z])[A-Za-z]:[\\/](?:[^\s<>"'`|?*]+[\\/])*[^\s<>"'`|?*]*/gi, 'local-path'],
    [/(?:\/Users\/|\/home\/|\/root\/|\/mnt\/[a-z]\/|\/tmp\/)[^\s<>"'`]*/gi, 'local-path'],
    [/\\\\[A-Za-z0-9._-]+\\(?:[^\s\\<>"'`|?*]+\\?)+/g, 'local-path'],
  ];
  for (const [pattern, label] of prePatterns) {
    const redacted = redactMatches(source, pattern, label);
    source = redacted.value;
    redactionCount += redacted.count;
  }
  const base = sanitizePublicConversationText(source);
  let text = base.text;
  redactionCount += base.redactionCount;
  const patterns = [
    [/\b\d{3}-\d{2}-\d{4}\b/g, 'government-id'],
    [/\b(?:\d[ -]*?){13,19}\b/g, 'payment-number'],
    [/\b(?:card|account)\s+(?:ending|ends|last four|suffix)\s+(?:in\s+)?\d{3,6}\b/gi, 'payment-suffix'],
    [/\b\d{1,6}\s+(?:[A-Z][\w'’-]+\s+){1,5}(?:Street|St|Avenue|Ave|Road|Rd|Boulevard|Blvd|Drive|Dr|Lane|Ln|Court|Ct|Way|Place|Pl|Circle|Cir)\.?\b/gi, 'street-address'],
    [/(?<![\w@])@[A-Za-z0-9_][A-Za-z0-9_.-]{2,30}\b/g, 'social-handle'],
  ];
  for (const [pattern, label] of patterns) {
    const redacted = redactMatches(text, pattern, label);
    text = redacted.value;
    redactionCount += redacted.count;
  }
  text = text.replace(/https?:\/\/[^\s<>"'`]+/gi, (candidate) => {
    try {
      const punctuation = candidate.match(/[),.;!?]+$/u)?.[0] ?? '';
      const raw = punctuation.length > 0 ? candidate.slice(0, -punctuation.length) : candidate;
      const url = new URL(raw);
      const host = url.hostname.toLocaleLowerCase();
      const capabilityPath = /\/(?:share|chat|c|design|file|document|task|tasks|download)(?:\/|$)/i.test(url.pathname)
        || url.pathname.split('/').some((segment) => /^[A-Za-z0-9_-]{24,}$/u.test(segment));
      const accountScopedHost = /(?:^|\.)(?:chatgpt\.com|chat\.openai\.com|claude\.ai|gemini\.google\.com|g\.co|canva\.com|drive\.google\.com|docs\.google\.com|notion\.so|dropbox\.com|figma\.com)$/i.test(host);
      if (accountScopedHost && capabilityPath) {
        redactionCount += 1;
        return `[redacted:capability-url]${punctuation}`;
      }
      if (url.search.length === 0 && url.hash.length === 0) return candidate;
      redactionCount += 1;
      return `${url.origin}${url.pathname}[redacted:url-parameters]${punctuation}`;
    } catch {
      return candidate;
    }
  });
  const names = redactContextualNames(text);
  text = names.value;
  redactionCount += names.count;
  return { text: text.replace(/\u0000/g, ''), redactionCount };
}

function safeTitle(value) {
  const sanitized = sanitize(value).text.replace(/\s+/g, ' ').trim();
  return sanitized.length > 0 ? sanitized.slice(0, 180) : 'Untitled conversation';
}

function timestampDensity(text) {
  return (text.match(/(?:^|\n)\s*(?:\d{1,2}:)?\d{1,2}:\d{2}\b/gm) ?? []).length;
}

function rightsAssessment({ id, sequence, role, title, text }) {
  const manualReason = MANUAL_RIGHTS_HOLDS.get(`${id}:${sequence}`);
  const timestamps = timestampDensity(text);
  const signal = `${title}\n${text.slice(0, 2_000)}`;
  const likelyTranscript = text.length > 2_500 && timestamps >= 15;
  const likelyBulkRequest = text.length > 1_000
    && /\b(?:lyrics?|full text|entire (?:book|article|chapter|script)|verbatim transcript|transcribe(?:d| this)?|copyrighted text)\b/i.test(signal);
  const held = manualReason !== undefined || likelyTranscript || likelyBulkRequest;
  const sourceKind = manualReason?.includes('song') ? 'quoted-song-corpus'
    : manualReason?.includes('book') ? 'quoted-book-passage'
      : likelyTranscript || manualReason?.includes('transcript') ? 'timestamped-transcript'
        : likelyBulkRequest ? 'bulk-third-party-text-candidate'
          : 'conversation-dialogue';
  return {
    rightsStatus: held ? 'withheld-pending-rights-review' : 'owner-authorized-dialogue-no-bulk-third-party-signal',
    sourceKind,
    sourceAttribution: held ? 'unverified-or-third-party' : role === 'user' ? 'exported human turn' : 'exported provider-visible turn',
    reviewState: held ? 'manual-review-required' : 'automated-screen-pass',
    reviewReason: manualReason ?? (likelyTranscript ? `${timestamps} timestamp cues` : likelyBulkRequest ? 'bulk-text request cue' : 'no bulk third-party signal detected'),
  };
}

function privacyAssessment(id, sequence) {
  const held = MANUAL_PRIVACY_HOLDS.has(`${id}:${sequence}`);
  return {
    privacyStatus: held ? 'withheld-pending-third-party-review' : 'automated-public-safety-screen-pass',
    reviewState: held ? 'manual-review-required' : 'automated-screen-pass',
    reviewReason: held ? 'independent reviewer flagged likely third-party identity or intimate context' : 'no deterministic hold matched',
  };
}

function slugify(value) {
  return value.normalize('NFKD').replace(/[\u0300-\u036f]/g, '').toLowerCase()
    .replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 72) || 'untitled';
}

function excerpt(value, limit = 300) {
  const normalized = value.replace(/\s+/g, ' ').trim()
    .replace(/\b(?:\d[ -]*?){13,19}\b/g, '[redacted:payment-number]');
  if (normalized.length <= limit) return normalized;
  const prefix = normalized.slice(0, limit - 1);
  const boundary = prefix.lastIndexOf(' ');
  return `${prefix.slice(0, boundary > limit * 0.66 ? boundary : prefix.length).trim()}…`;
}

function themesFor(category, title, messages) {
  const themes = [];
  const mapped = CATEGORY_THEME[category];
  if (mapped) themes.push(mapped);
  const sample = `${title}\n${messages.slice(0, 8).map((message) => message.text.slice(0, 2_000)).join('\n')}`;
  for (const [theme, pattern] of THEME_PATTERNS) {
    if (pattern.test(sample) && !themes.includes(theme)) themes.push(theme);
  }
  return (themes.length > 0 ? themes : ['Unclassified']).slice(0, 4);
}

function inferredCategory(title, messages) {
  const sample = `${title}\n${messages.slice(0, 12).map((message) => message.text.slice(0, 2_000)).join('\n')}`;
  if (/\b(?:tarot|oracle|rune|astrolog|numerolog|divination|omen|synchronicit|sigil|i ching)\b/i.test(sample)) return 'Numerology & Astrology';
  if (/\b(?:relationship|partner|wife|husband|friend|family|daughter|son|mother|father|love|breakup)\b/i.test(sample)) return 'Relationships & Personal';
  if (/\b(?:game|quest|player|level|deck|card|rpg|dungeon|worldbuild)\b/i.test(sample)) return 'Gaming & Game Dev';
  if (/\b(?:myth|archetype|legend|folklore|pantheon|deity|goddess)\b/i.test(sample)) return 'Mythology & Folklore';
  if (/\b(?:spirit|soul|sacred|mystic|gnostic|divine|ritual|prayer|religion|meditat)\b/i.test(sample)) return 'Spirituality & Mysticism';
  if (/\b(?:conscious|mind|identity|selfhood|reality|perception|cognition|psyche|philosoph)\b/i.test(sample)) return 'Philosophy & Psychology';
  if (/\b(?:physics|mathemat|equation|quantum|cosmolog)\b/i.test(sample)) return 'Physics & Mathematics';
  if (/\b(?:machine learning|artificial intelligence|\bAI\b|software|network|model|computer|code)\b/i.test(sample)) return 'AI & Technology';
  if (/\b(?:write|writing|novel|art|music|poem|design|create|creative)\b/i.test(sample)) return 'Creative Writing & Worldbuilding';
  if (/\b(?:health|body|sleep|food|medicine|doctor|symptom|wellness)\b/i.test(sample)) return 'Health & Wellness';
  if (/\b(?:work|home|day|daily|money|job|morning|night)\b/i.test(sample)) return 'Practical & Daily Life';
  return 'Uncategorized';
}

function warningsFor(title, messages) {
  const sample = `${title}\n${messages.map((message) => message.text).join('\n')}`;
  return WARNING_PATTERNS.filter(([, pattern]) => pattern.test(sample)).map(([label]) => label);
}

function countAlternations(messages) {
  let count = 0;
  for (let index = 1; index < messages.length; index += 1) {
    if (messages[index - 1].role !== messages[index].role) count += 1;
  }
  return count;
}

function signalScore(message, role) {
  const length = message.text.length;
  let score = Math.min(18, Math.log2(Math.max(2, length)) * 2);
  if (role === 'user' && /\?/u.test(message.text)) score += 5;
  if (role === 'user' && /\b(?:I think|I feel|I mean|my view|what if|why|how|could)\b/i.test(message.text)) score += 4;
  if (role === 'assistant' && /\b(?:because|however|boundary|evidence|means|distinction|pattern)\b/i.test(message.text)) score += 4;
  if (/```|<script|function\s*\(/i.test(message.text)) score -= 4;
  if (/\[redacted:/i.test(message.text)) score -= 3;
  if (length > 12_000) score -= 5;
  return score;
}

function bestSignal(messages, role, fallback = '') {
  const ranked = messages.filter((message) => message.role === role)
    .map((message) => ({ message, score: signalScore(message, role) }))
    .sort((left, right) => right.score - left.score || left.message.sequence - right.message.sequence);
  return excerpt(ranked[0]?.message.text ?? fallback, 620);
}

function distillConversation(title, messages, themes, warnings) {
  const humanSignal = bestSignal(messages, 'user', title);
  const aiSignal = bestSignal(messages, 'assistant');
  const correction = messages.find((message) => message.role === 'user'
    && /\b(?:no[,— -]|not quite|that(?:'s| is) not|I (?:mean|disagree)|actually|you(?:'re| are) (?:missing|assuming|over))\b/i.test(message.text));
  const questions = messages.filter((message) => message.role === 'user' && /\?/u.test(message.text))
    .slice(0, 3).map((message) => excerpt(message.text, 240));
  const evidenceBoundary = warnings.includes('health or medication')
    ? 'This record may describe health or distress. It is a historical conversation, not diagnosis or medical advice.'
    : themes.includes('Science and systems')
      ? 'Technical and scientific claims remain historical dialogue until independently sourced and tested.'
      : themes.some((theme) => ['Spiritual life', 'Divination', 'Myth and meaning', 'Consciousness'].includes(theme))
        ? 'Lived experience and metaphor are preserved as reported meaning; neither automatically establishes a physical or supernatural mechanism.'
        : 'The full dialogue is the source. This distillation is a navigation aid, not a replacement or endorsement of every claim.';
  return {
    humanSignal,
    aiSignal,
    correctionSignal: correction ? excerpt(correction.text, 420) : '',
    questions,
    arc: messages.length === 0
      ? 'empty export record'
      : countAlternations(messages) >= 3
        ? correction ? 'question → response → correction → revision' : 'question → response → iterative development'
        : 'prompt → response',
    evidenceBoundary,
  };
}

function qualityScore({ title, messages, themes, warnings, redactionCount, distillation }) {
  const human = messages.filter((message) => message.role === 'user').length;
  const assistant = messages.filter((message) => message.role === 'assistant').length;
  const characters = messages.reduce((total, message) => total + message.text.length, 0);
  const dimensions = {
    dialogueCompleteness: Math.min(25, (human > 0 && assistant > 0 ? 12 : 0) + Math.min(7, Math.floor(messages.length / 3)) + Math.min(6, countAlternations(messages))),
    substance: Math.min(20, characters === 0 ? 0 : Math.max(2, Math.round(Math.log10(characters + 1) * 5))),
    thematicResonance: Math.min(20, (themes[0] !== 'Unclassified' ? 8 : 1) + Math.min(12, themes.length * 4)),
    interpretability: Math.min(20, (/^(?:untitled conversation|new chat|okay|yes|no|save point|chat)$/i.test(title.trim()) ? 2 : 10) + (distillation.questions.length > 0 ? 5 : 0) + (distillation.aiSignal ? 5 : 0)),
    dialogicDepth: Math.min(15, Math.min(9, countAlternations(messages)) + (distillation.correctionSignal ? 6 : 0)),
    privacyRisk: Math.min(35, warnings.length * 6 + Math.min(11, Math.round((redactionCount / Math.max(1, messages.length)) * 2))),
  };
  const score = Math.max(0, Math.min(100,
    dimensions.dialogueCompleteness + dimensions.substance + dimensions.thematicResonance
    + dimensions.interpretability + dimensions.dialogicDepth - dimensions.privacyRisk));
  const selectionReasons = [
    dimensions.dialogueCompleteness >= 18 ? 'sustained two-sided dialogue' : 'limited dialogue structure',
    dimensions.thematicResonance >= 12 ? `strong ${themes.slice(0, 2).join(' + ')} signal` : 'weak or uncategorized theme signal',
    distillation.correctionSignal ? 'contains a human correction or resistance point' : 'no clear correction signal detected',
    warnings.length > 0 ? `${warnings.length} content-risk gate${warnings.length === 1 ? '' : 's'}` : 'no automated content-risk gate',
  ];
  return { score, dimensions, selectionReasons };
}

function loreFor(id, title, themes, distillation) {
  const primary = themes[0] ?? 'Unclassified';
  const [realm, artifact] = REALMS[primary] ?? REALMS.Unclassified;
  const variants = ['Recovered', 'Unbound', 'Recursive', 'Violet', 'Many-Voiced', 'Liminal', 'Unwritten', 'Living'];
  const variant = variants[Number.parseInt(id.slice(0, 4), 16) % variants.length];
  const fragments = LORE_FRAGMENTS[primary] ?? LORE_FRAGMENTS.Unclassified;
  return {
    realm,
    artifact,
    fragmentTitle: `${variant} ${artifact}: ${title}`,
    invocation: excerpt(distillation.humanSignal || title, 220),
    fragment: fragments[Number.parseInt(id.slice(4, 8), 16) % fragments.length],
    reading: `Filed in ${realm}. This shard is indexed through ${themes.join(', ')}; its full dialogue remains the authority, while lore is a navigational layer rather than a factual claim.`,
    truthStatus: 'original-editorial-allegory',
  };
}

function makeRecord({ provider, sourceId, sourceFingerprint, exportFingerprint, title: rawTitle, createdAt, updatedAt, category, categoryProvenance, rawMessages, extra = {} }) {
  const title = safeTitle(rawTitle);
  const identity = sha256(`${provider}\0${sourceId}`);
  const id = identity.slice(0, 20);
  let redactionCount = 0;
  const messages = rawMessages.map((message, index) => {
    const sanitized = sanitize(message.text);
    const sequence = index + 1;
    const rights = rightsAssessment({ id, sequence, role: message.role, title, text: sanitized.text });
    const privacy = privacyAssessment(id, sequence);
    const rightsWithheld = rights.rightsStatus === 'withheld-pending-rights-review';
    const privacyWithheld = privacy.privacyStatus === 'withheld-pending-third-party-review';
    const withheld = rightsWithheld || privacyWithheld;
    redactionCount += sanitized.redactionCount + (withheld ? 1 : 0);
    return {
      sequence,
      role: message.role,
      branch: message.branch ?? 'primary',
      ...(message.createdAt ? { createdAt: message.createdAt } : {}),
      text: privacyWithheld ? '[withheld:third-party-privacy-review]'
        : rightsWithheld ? `[withheld:rights-review:${rights.sourceKind}]`
          : sanitized.text,
      contentSha256: sha256(sanitized.text),
      sourceBytes: Buffer.byteLength(sanitized.text, 'utf8'),
      rights,
      privacy,
    };
  }).filter((message) => message.text.trim().length > 0);
  let slug = `${slugify(title)}-${identity.slice(0, 10)}`;
  const resolvedCategory = category === 'infer' ? inferredCategory(title, messages) : category;
  const themes = themesFor(resolvedCategory, title, messages);
  const rightsHoldCount = messages.filter((message) => message.rights.reviewState === 'manual-review-required').length;
  const privacyHoldCount = messages.filter((message) => message.privacy.reviewState === 'manual-review-required').length;
  const warnings = [
    ...warningsFor(title, messages),
    ...(rightsHoldCount > 0 ? ['third-party rights review'] : []),
    ...(privacyHoldCount > 0 ? ['third-party privacy review'] : []),
  ];
  if (warnings.length > 0) slug = `protected-conversation-${identity.slice(0, 10)}`;
  const distillation = distillConversation(title, messages, themes, warnings);
  const quality = qualityScore({ title, messages, themes, warnings, redactionCount, distillation });
  const disqualifyingWarnings = ['health or medication', 'relationship privacy', 'sexual content', 'financial details', 'third-party rights review', 'third-party privacy review'];
  const indexable = quality.score >= 60
    && !warnings.some((warning) => disqualifyingWarnings.includes(warning))
    && redactionCount / Math.max(1, messages.length) < 3;
  const record = {
    schema: SCHEMA,
    id,
    slug,
    title,
    provider,
    sourceReference: `${provider === 'ChatGPT' ? 'GPT' : 'CLAUDE'}-${identity.slice(0, 12).toUpperCase()}`,
    sourceFingerprint,
    exportFingerprint,
    createdAt,
    updatedAt,
    category: resolvedCategory,
    categoryProvenance,
    themes,
    contentWarnings: warnings,
    messageCount: messages.length,
    bodyState: messages.length > 0 ? 'present' : 'absent-in-export',
    userMessageCount: messages.filter((message) => message.role === 'user').length,
    assistantMessageCount: messages.filter((message) => message.role === 'assistant').length,
    alternateMessageCount: messages.filter((message) => message.branch === 'alternate').length,
    redactionCount,
    rightsHoldCount,
    privacyHoldCount,
    qualityScore: quality.score,
    qualityDimensions: quality.dimensions,
    selectionReasons: quality.selectionReasons,
    automatedFeatureCandidate: quality.score >= 76 && warnings.length === 0 && messages.length >= 4,
    featureEligible: false,
    editorialReviewState: 'unreviewed',
    indexable: false,
    excerpt: excerpt(distillation.humanSignal || title),
    humanSignal: distillation.humanSignal,
    aiSignal: distillation.aiSignal,
    distillation,
    lore: loreFor(identity, title, themes, distillation),
    publication: {
      state: 'review-held-local-candidate',
      policy: 'Local candidate only. Automated redaction is not publication approval; a record stays outside public assets until explicit privacy and rights review marks it approved.',
    },
    messages,
    ...extra,
  };
  return record;
}

async function chatGptRecords(directory, categories) {
  const names = (await readdir(directory)).filter((name) => /^conversations-\d+\.json$/u.test(name)).sort();
  if (names.length === 0) throw new Error('No ChatGPT conversation shards found');
  const records = [];
  const seen = new Set();
  const exclusions = { structuralRoleMessages: 0, hiddenMessages: 0, toolDirectedMessages: 0, reasoningOrToolBodies: 0, emptyVisibleBodies: 0 };
  for (const name of names) {
    const bytes = await readFile(join(directory, name));
    const fingerprint = sha256(bytes);
    const conversations = JSON.parse(bytes.toString('utf8'));
    if (!Array.isArray(conversations)) throw new Error(`ChatGPT shard is not an array: ${name}`);
    for (const conversation of conversations) {
      const sourceId = conversation.conversation_id ?? conversation.id;
      if (typeof sourceId !== 'string' || sourceId.length === 0 || seen.has(sourceId)) continue;
      seen.add(sourceId);
      const mapping = conversation.mapping && typeof conversation.mapping === 'object' ? conversation.mapping : {};
      const primary = new Set();
      let cursor = conversation.current_node;
      while (typeof cursor === 'string' && mapping[cursor] && !primary.has(cursor)) {
        primary.add(cursor);
        cursor = mapping[cursor].parent;
      }
      const nodes = Object.values(mapping).flatMap((node) => {
        const message = node?.message;
        const role = message?.author?.role;
        if (!['user', 'assistant'].includes(role)) {
          if (message) exclusions.structuralRoleMessages += 1;
          return [];
        }
        if (message.metadata?.is_visually_hidden_from_conversation === true) {
          exclusions.hiddenMessages += 1;
          return [];
        }
        if (message.recipient !== undefined && message.recipient !== 'all') {
          exclusions.toolDirectedMessages += 1;
          return [];
        }
        if (!['text', 'multimodal_text'].includes(message.content?.content_type)) {
          exclusions.reasoningOrToolBodies += 1;
          return [];
        }
        const text = extractText(message.content).trim();
        if (text.length === 0) {
          exclusions.emptyVisibleBodies += 1;
          return [];
        }
        return [{
          nodeId: node.id,
          role,
          text,
          createdAt: Number.isFinite(message.create_time) ? new Date(message.create_time * 1_000).toISOString() : undefined,
          branch: primary.has(node.id) ? 'primary' : 'alternate',
          time: message.create_time ?? 0,
        }];
      });
      nodes.sort((left, right) => {
        if (left.branch !== right.branch) return left.branch === 'primary' ? -1 : 1;
        return left.time - right.time || String(left.nodeId).localeCompare(String(right.nodeId));
      });
      const createdAt = dateFromEpoch(conversation.create_time) || dateFromEpoch(nodes[0]?.time) || 'unknown';
      const updatedAt = dateFromEpoch(conversation.update_time) || createdAt;
      const categoryMatch = takeCategory(categories, String(conversation.title ?? ''), [createdAt, updatedAt]);
      records.push(makeRecord({
        provider: 'ChatGPT',
        sourceId,
        sourceFingerprint: sha256(JSON.stringify(conversation)),
        exportFingerprint: fingerprint,
        title: conversation.title ?? 'Untitled conversation',
        createdAt,
        updatedAt,
        category: categoryMatch.category,
        categoryProvenance: categoryMatch.provenance,
        rawMessages: nodes,
        extra: { sourceShard: basename(name) },
      }));
    }
  }
  return { records, exclusions };
}

async function claudeRecords(path) {
  const bytes = await readFile(path);
  const fingerprint = sha256(bytes);
  const conversations = JSON.parse(bytes.toString('utf8'));
  if (!Array.isArray(conversations)) throw new Error('Claude conversations export is not an array');
  const exclusions = { thinkingBlocks: 0, toolUseBlocks: 0, toolResultBlocks: 0, flagBlocks: 0, emptyVisibleBodies: 0 };
  const records = conversations.flatMap((conversation) => {
    const sourceId = conversation.uuid;
    if (typeof sourceId !== 'string' || sourceId.length === 0) return [];
    const messages = (conversation.chat_messages ?? []).flatMap((message) => {
      const role = message.sender === 'human' ? 'user' : message.sender === 'assistant' ? 'assistant' : undefined;
      if (role === undefined) return [];
      const blocks = Array.isArray(message.content) ? message.content : [];
      exclusions.thinkingBlocks += blocks.filter((block) => block?.type === 'thinking').length;
      exclusions.toolUseBlocks += blocks.filter((block) => block?.type === 'tool_use').length;
      exclusions.toolResultBlocks += blocks.filter((block) => block?.type === 'tool_result').length;
      exclusions.flagBlocks += blocks.filter((block) => block?.type === 'flag').length;
      const text = blocks.filter((block) => block?.type === 'text' && typeof block.text === 'string')
        .map((block) => block.text.trim()).filter(Boolean).join('\n\n');
      const attachments = [...(message.attachments ?? []), ...(message.files ?? [])].length;
      const body = [text, attachments > 0 ? `[${attachments} private attachment${attachments === 1 ? '' : 's'} omitted]` : '']
        .filter(Boolean).join('\n\n');
      if (body.length === 0) {
        exclusions.emptyVisibleBodies += 1;
        return [];
      }
      return [{ role, text: body, branch: 'primary', createdAt: message.created_at }];
    });
    const createdAt = String(conversation.created_at ?? '').slice(0, 10) || 'unknown';
    return [makeRecord({
      provider: 'Claude',
      sourceId,
      sourceFingerprint: sha256(JSON.stringify(conversation)),
      exportFingerprint: fingerprint,
      title: conversation.name ?? 'Untitled conversation',
      createdAt,
      updatedAt: String(conversation.updated_at ?? '').slice(0, 10) || createdAt,
      category: 'infer',
      categoryProvenance: 'deterministic public-corpus classifier v1',
      rawMessages: messages,
      extra: { attachmentCount: (conversation.chat_messages ?? []).reduce((total, message) => total + (message.attachments ?? []).length + (message.files ?? []).length, 0) },
    })];
  });
  return { records, exclusions };
}

function lowEntropy(text) {
  if (text.length < 500 || text.startsWith('[withheld:')) return false;
  const compact = text.replace(/\s+/g, '');
  if (compact.length < 500) return false;
  const frequencies = new Map();
  for (const character of compact) frequencies.set(character, (frequencies.get(character) ?? 0) + 1);
  const dominant = Math.max(...frequencies.values(), 0) / compact.length;
  return dominant >= 0.78 || frequencies.size <= 4 || /(.)\1{399,}/su.test(compact);
}

function annotateCorpusQuality(records) {
  const firstByHash = new Map();
  const duplicateHashes = new Set();
  const affectedRecords = new Set();
  let duplicateMessages = 0;
  let lowEntropyMessages = 0;
  for (const record of records) {
    let duplicateBytes = 0;
    let recordLowEntropy = 0;
    for (const message of record.messages) {
      if (lowEntropy(message.text)) {
        recordLowEntropy += 1;
        lowEntropyMessages += 1;
      }
      if (message.sourceBytes < 500 || message.text.startsWith('[withheld:')) continue;
      const first = firstByHash.get(message.contentSha256);
      if (first) {
        message.repeatOf = first;
        duplicateMessages += 1;
        duplicateBytes += message.sourceBytes;
        duplicateHashes.add(message.contentSha256);
        affectedRecords.add(record.id);
        affectedRecords.add(first.recordId);
      } else firstByHash.set(message.contentSha256, { recordId: record.id, sequence: message.sequence });
    }
    const totalBytes = record.messages.reduce((total, message) => total + message.sourceBytes, 0);
    record.duplicateMessageCount = record.messages.filter((message) => message.repeatOf !== undefined).length;
    record.duplicateByteRatio = totalBytes === 0 ? 0 : Number((duplicateBytes / totalBytes).toFixed(4));
    record.lowEntropyMessageCount = recordLowEntropy;
    const duplicatePenalty = Math.min(24, Math.round(record.duplicateByteRatio * 30));
    const lowEntropyPenalty = Math.min(30, recordLowEntropy * 12);
    record.qualityDimensions = { ...record.qualityDimensions, duplicatePenalty, lowEntropyPenalty };
    record.qualityScore = Math.max(0, record.qualityScore - duplicatePenalty - lowEntropyPenalty);
    if (duplicatePenalty > 0) record.selectionReasons.push(`${duplicatePenalty}-point repeated-body penalty`);
    if (lowEntropyPenalty > 0) record.selectionReasons.push(`${lowEntropyPenalty}-point low-entropy penalty`);
    record.automatedFeatureCandidate = record.qualityScore >= 76
      && record.contentWarnings.length === 0
      && record.messages.length >= 4
      && record.duplicateByteRatio < 0.2
      && record.lowEntropyMessageCount === 0;
    record.indexable = record.indexable
      && record.rightsHoldCount === 0
      && record.privacyHoldCount === 0
      && record.duplicateByteRatio < 0.45
      && record.lowEntropyMessageCount === 0;
  }
  return {
    duplicateGroups: duplicateHashes.size,
    duplicateMessages,
    recordsAffected: affectedRecords.size,
    lowEntropyMessages,
  };
}

function titleTokens(title) {
  const stop = new Set(['about', 'after', 'again', 'also', 'another', 'could', 'from', 'have', 'into', 'just', 'more', 'that', 'this', 'what', 'when', 'where', 'which', 'with', 'would', 'your']);
  return new Set(title.toLocaleLowerCase().match(/[a-z0-9]{4,}/g)?.filter((token) => !stop.has(token)) ?? []);
}

function connectCorpus(records) {
  const prepared = records.map((record) => ({ record, tokens: titleTokens(record.title) }));
  for (const current of prepared) {
    const candidates = [];
    for (const other of prepared) {
      if (current.record.id === other.record.id) continue;
      const sharedThemes = current.record.themes.filter((theme) => other.record.themes.includes(theme));
      const sharedTitleTerms = [...current.tokens].filter((token) => other.tokens.has(token));
      if (sharedThemes.length === 0 && sharedTitleTerms.length === 0) continue;
      let score = sharedThemes.length * 8 + Math.min(6, sharedTitleTerms.length * 2);
      if (current.record.category === other.record.category) score += 4;
      if (current.record.provider !== other.record.provider) score += 2;
      if (current.record.indexable && other.record.indexable) score += 1;
      candidates.push({
        score,
        id: other.record.id,
        slug: other.record.slug,
        title: other.record.contentWarnings.length > 0 ? `Protected ${other.record.provider} conversation` : other.record.title,
        provider: other.record.provider,
        sharedThemes,
        reason: sharedThemes.length > 0
          ? `Shared ${sharedThemes.join(' + ')} lens${current.record.provider !== other.record.provider ? ' across providers' : ''}.`
          : `Shared title signal: ${sharedTitleTerms.slice(0, 3).join(', ')}.`,
      });
    }
    candidates.sort((left, right) => right.score - left.score || left.title.localeCompare(right.title));
    current.record.connections = candidates.slice(0, 6).map(({ score: _score, ...candidate }) => ({
      ...candidate,
      href: `/conversations/${candidate.slug}`,
    }));
  }
}

function sealRecord(record) {
  const projection = JSON.stringify(record);
  return { ...record, projectionSha256: sha256(projection), projectionBytes: Buffer.byteLength(projection, 'utf8') };
}

function summary(record) {
  const { messages: _messages, schema: _schema, publication: _publication, ...value } = record;
  const routes = { href: `/conversations/${record.slug}`, bodyHref: `/conversation-corpus/approved-records/${record.id}.json` };
  if (record.contentWarnings.length === 0) return { ...value, ...routes };
  return {
    ...value,
    title: `Protected ${record.provider} conversation`,
    excerpt: 'Open the content notice before loading this conversation.',
    humanSignal: '',
    aiSignal: '',
    distillation: {
      humanSignal: '',
      aiSignal: '',
      correctionSignal: '',
      questions: [],
      arc: record.distillation.arc,
      evidenceBoundary: 'This source-derived layer remains behind the same content notice as the full dialogue.',
    },
    lore: {
      ...record.lore,
      fragmentTitle: `Protected ${record.lore.artifact}`,
      invocation: '',
    },
    ...routes,
  };
}

function browseSummary(record) {
  const protectedRecord = record.contentWarnings.length > 0;
  return {
    id: record.id,
    slug: record.slug,
    title: protectedRecord ? `Protected ${record.provider} conversation` : record.title,
    provider: record.provider,
    createdAt: record.createdAt,
    category: record.category,
    themes: record.themes,
    contentWarningCount: record.contentWarnings.length,
    messageCount: record.messageCount,
    bodyState: record.bodyState,
    redactionCount: record.redactionCount,
    qualityScore: record.qualityScore,
    automatedFeatureCandidate: record.automatedFeatureCandidate,
    editorialReviewState: record.editorialReviewState,
    indexable: record.indexable,
    excerpt: protectedRecord ? 'Open the content notice before loading this conversation.' : record.excerpt,
    loreRealm: record.lore.realm,
    loreArtifact: record.lore.artifact,
    href: `/conversations/${record.slug}`,
  };
}

async function writeOrCheck(path, content, check) {
  if (check) {
    let current;
    try { current = await readFile(path, 'utf8'); } catch { current = undefined; }
    if (current !== content) throw new Error(`Generated conversation corpus is stale: ${path.slice(repositoryRoot.length + 1)}`);
    return;
  }
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, content, 'utf8');
}

function validatePublicValue(value, label) {
  const forbidden = [
    /[A-Za-z]:[\\/]Users[\\/]/i,
    /(?:\/Users\/|\/home\/|\/root\/|\/tmp\/)/i,
    /\b[A-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Z0-9](?:[A-Z0-9.-]*[A-Z0-9])?\b/i,
    /\b(?:ghp|gho|ghu|ghs|ghr|github_pat)_[A-Za-z0-9_]{20,}\b/,
    /\bsk-(?:live-)?[A-Za-z0-9_-]{20,}\b/,
    /-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----/i,
  ];
  const visit = (candidate, path = '$') => {
    if (typeof candidate === 'string') {
      for (const pattern of forbidden) {
        if (pattern.test(candidate)) throw new Error(`Residual private pattern in ${label} at ${path}: ${pattern}`);
      }
      return;
    }
    if (Array.isArray(candidate)) {
      candidate.forEach((item, index) => visit(item, `${path}[${index}]`));
      return;
    }
    if (candidate !== null && typeof candidate === 'object') {
      Object.entries(candidate).forEach(([key, item]) => visit(item, `${path}.${key}`));
    }
  };
  visit(value);
}

async function removeStaleRecords(expected, check) {
  let names = [];
  try { names = await readdir(approvedRecordRoot); } catch { return; }
  const stale = names.filter((name) => /^[a-f0-9]{20}\.json$/u.test(name) && !expected.has(name));
  if (stale.length === 0) return;
  if (check) throw new Error(`Generated conversation corpus contains ${stale.length} stale records`);
  const resolvedRoot = resolve(approvedRecordRoot);
  if (resolvedRoot !== resolve(repositoryRoot, 'public', 'conversation-corpus', 'approved-records')) throw new Error('Unsafe stale-record root');
  for (const name of stale) await rm(join(resolvedRoot, name), { force: true });
}

export async function buildCorpus({ chatgptDir, claudeJson, categories: categoryPath, check = false }) {
  const [categoryBytes, chatgptInfo, claudeInfo] = await Promise.all([
    readFile(categoryPath),
    stat(chatgptDir),
    stat(claudeJson),
  ]);
  if (!chatgptInfo.isDirectory() || !claudeInfo.isFile()) throw new Error('Conversation source type mismatch');
  const categories = categoryLookup(categoryBytes.toString('utf8'));
  const [chatgptResult, claudeResult] = await Promise.all([
    chatGptRecords(chatgptDir, categories),
    claudeRecords(claudeJson),
  ]);
  const chatgpt = chatgptResult.records;
  const claude = claudeResult.records;
  if (chatgpt.length !== EXPECTED_CHATGPT || claude.length !== EXPECTED_CLAUDE) {
    throw new Error(`Conversation denominator drift: ChatGPT=${chatgpt.length}, Claude=${claude.length}`);
  }
  const unsealedRecords = [...chatgpt, ...claude];
  if (unsealedRecords.length !== EXPECTED_UNIQUE || new Set(unsealedRecords.map((record) => record.id)).size !== EXPECTED_UNIQUE) {
    throw new Error('Conversation identity denominator drift');
  }
  const qualityAudit = annotateCorpusQuality(unsealedRecords);
  unsealedRecords.sort((left, right) => right.qualityScore - left.qualityScore || right.createdAt.localeCompare(left.createdAt) || left.slug.localeCompare(right.slug));
  connectCorpus(unsealedRecords);
  const records = unsealedRecords.map(sealRecord);
  for (const record of records) validatePublicValue(record, record.id);
  const approvedRecords = records.filter((record) => (
    record.editorialReviewState === 'approved'
    && record.publication?.state === 'owner-approved-public-projection'
    && record.rightsHoldCount === 0
    && record.privacyHoldCount === 0
  ));
  const summaries = approvedRecords.map(summary);
  const manifest = {
    schema: SCHEMA,
    generatedAt: GENERATED_AT,
    publicationState: 'aggregate-public-bodies-review-held',
    publicationAuthority: 'Public bodies require explicit per-record owner approval after privacy and rights review.',
    scope: 'Aggregate facts describe the complete local ChatGPT and Claude export index. Public records contain only explicitly approved projections; unreviewed source-derived titles, excerpts, signals, and bodies remain local.',
    boundaries: [
      'System and tool payloads are structural export data, not human-AI dialogue, and are counted but not published as turns.',
      'Hidden reasoning, thought summaries, tool-directed assistant messages, and Claude thinking/tool blocks are excluded from conversation bodies and counted in structuralExclusions.',
      'Private attachments are counted and replaced with placeholders; attachment bodies are not published.',
      'Automated redaction is a review aid, not proof of public safety or publication authority.',
      'Unreviewed records never enter the public index, browse projection, sitemap, API body store, or static assets.',
      'Source bodies remain attributed as historical reports. Lore, ranking, and classification add no factual authority.',
    ],
    selectionCriteria: {
      inclusion: 'complete local aggregate; public record admission requires explicit editorial, privacy, and rights approval',
      ranking: 'transparent multidimensional score: dialogue completeness + substance + thematic resonance + interpretability + correction depth - privacy risk',
      rankingUse: 'featured ordering only; per-record dimensions and reasons remain inspectable',
      indexability: 'approved records only; non-approved records are neither readable nor indexable',
      distillation: 'role-separated signal selection + detected questions/corrections + evidence boundary; never substitutes for the full dialogue',
      lore: 'original Apocky cosmology rendered as a fragmented navigation layer; it does not add factual authority or imitate source prose from another franchise',
      connections: 'shared themes + category + title signals + cross-provider bonus; each edge carries its reason',
    },
    counts: {
      uniqueConversations: records.length,
      chatgptConversations: chatgpt.length,
      claudeConversations: claude.length,
      anthropicDuplicateDelivery: 37,
      messages: records.reduce((total, record) => total + record.messageCount, 0),
      emptyConversationRecords: records.filter((record) => record.bodyState === 'absent-in-export').length,
      userMessages: records.reduce((total, record) => total + record.userMessageCount, 0),
      assistantMessages: records.reduce((total, record) => total + record.assistantMessageCount, 0),
      alternateBranchMessages: records.reduce((total, record) => total + record.alternateMessageCount, 0),
      redactions: records.reduce((total, record) => total + record.redactionCount, 0),
      automatedFeatureCandidates: records.filter((record) => record.automatedFeatureCandidate).length,
      editoriallyFeatureEligible: records.filter((record) => record.featureEligible).length,
      indexable: approvedRecords.filter((record) => record.indexable).length,
      publiclyApprovedConversations: approvedRecords.length,
      reviewHeldConversations: records.filter((record) => record.editorialReviewState === 'unreviewed').length,
      rejectedConversations: records.filter((record) => record.editorialReviewState === 'rejected').length,
      publishedMessages: approvedRecords.reduce((total, record) => total + record.messageCount, 0),
    },
    qualityAudit,
    structuralExclusions: {
      ChatGPT: chatgptResult.exclusions,
      Claude: claudeResult.exclusions,
    },
    categorySourceFingerprint: sha256(categoryBytes),
    records: summaries,
  };
  validatePublicValue(manifest, 'manifest');
  const browseManifest = {
    schema: 'apocky.public-conversation-corpus.browse.v1',
    generatedAt: GENERATED_AT,
    publicationState: manifest.publicationState,
    scope: manifest.scope,
    boundaries: manifest.boundaries,
    counts: manifest.counts,
    records: approvedRecords.map(browseSummary),
  };
  validatePublicValue(browseManifest, 'browse manifest');
  const expectedFiles = new Set();
  for (const record of approvedRecords) {
    const name = `${record.id}.json`;
    expectedFiles.add(name);
    await writeOrCheck(join(approvedRecordRoot, name), `${JSON.stringify(record)}\n`, check);
  }
  await removeStaleRecords(expectedFiles, check);
  await writeOrCheck(join(outputRoot, 'public-index.v1.json'), `${JSON.stringify(manifest)}\n`, check);
  await writeOrCheck(join(outputRoot, 'browse.v1.json'), `${JSON.stringify(browseManifest)}\n`, check);
  return manifest;
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = parseArgs(process.argv.slice(2));
  buildCorpus(args).then((manifest) => {
    console.log(`conversation corpus : ${args.check ? 'CURRENT' : 'WROTE'} · ${manifest.counts.uniqueConversations} conversations · ${manifest.counts.messages} messages · ${manifest.counts.redactions} redactions`);
  }).catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
