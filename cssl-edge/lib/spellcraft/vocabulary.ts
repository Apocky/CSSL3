import { sha256Hex, stableStringify } from './hash';
import {
  VOCABULARY_ID,
  VOCAB_VERSION,
  type VocabularyEntry,
  type VocabularyNamespace,
  type VocabularyOperation,
} from './types';

type EntryDefinition = Readonly<{
  lexeme: string;
  namespace: VocabularyNamespace;
  meaning: string;
  category: string;
  tags: readonly string[];
  operation?: VocabularyOperation;
}>;

function entry(definition: EntryDefinition): VocabularyEntry {
  const tags = Object.freeze([...definition.tags].sort());
  return Object.freeze({
    ...definition,
    tags,
    id: `${VOCABULARY_ID}:${definition.namespace}:${definition.lexeme}`,
  });
}

/**
 * A compact, namespaced vocabulary derived from owner-authorized Haloic language materials.
 * Colliding surface forms intentionally remain separate; callers must qualify them.
 */
export const HALOIC_VOCAB: readonly VocabularyEntry[] = Object.freeze([
  entry({ lexeme: 'ka', namespace: 'prefix', meaning: 'bring forth or cause', category: 'intent', tags: ['CAUSE'], operation: 'intent.cause' }),
  entry({ lexeme: 'ul', namespace: 'prefix', meaning: 'invert or negate', category: 'intent', tags: ['NEGATE'], operation: 'intent.negate' }),
  entry({ lexeme: 'na', namespace: 'prefix', meaning: 'hope or aspire toward', category: 'intent', tags: ['ASPIRE'], operation: 'intent.aspire' }),
  entry({ lexeme: 'arch', namespace: 'prefix', meaning: 'intensify', category: 'intent', tags: ['AMPLIFY'], operation: 'intent.amplify' }),
  entry({ lexeme: 'ur', namespace: 'prefix', meaning: 'return to origin', category: 'intent', tags: ['ORIGIN'], operation: 'intent.origin' }),

  entry({ lexeme: 'el', namespace: 'suffix', meaning: 'becoming or transformation', category: 'modifier', tags: ['TRANSFORM'], operation: 'modifier.transform' }),
  entry({ lexeme: 'eth', namespace: 'suffix', meaning: 'quality or state', category: 'modifier', tags: ['STATE'], operation: 'modifier.state' }),
  entry({ lexeme: 'i', namespace: 'suffix', meaning: 'agent or bearer', category: 'modifier', tags: ['AGENT'], operation: 'modifier.agent' }),
  entry({ lexeme: 'im', namespace: 'suffix', meaning: 'collective or plurality', category: 'modifier', tags: ['COLLECTIVE'], operation: 'modifier.collective' }),
  entry({ lexeme: 'a', namespace: 'suffix', meaning: 'abstract form', category: 'modifier', tags: ['ABSTRACT'], operation: 'modifier.abstract' }),
  entry({ lexeme: 'on', namespace: 'suffix', meaning: 'place or realm', category: 'modifier', tags: ['PLACE'], operation: 'modifier.place' }),
  entry({ lexeme: 'ei', namespace: 'suffix', meaning: 'ongoing motion', category: 'modifier', tags: ['CONTINUOUS'], operation: 'modifier.continuous' }),
  entry({ lexeme: 'ai', namespace: 'suffix', meaning: 'ongoing motion', category: 'modifier', tags: ['CONTINUOUS'], operation: 'modifier.continuous' }),

  entry({ lexeme: 'ki', namespace: 'particle', meaning: 'if', category: 'logic', tags: ['IF'], operation: 'flow.if' }),
  entry({ lexeme: 'ya', namespace: 'particle', meaning: 'then', category: 'logic', tags: ['THEN'], operation: 'flow.then' }),
  entry({ lexeme: 'al', namespace: 'particle', meaning: 'else', category: 'logic', tags: ['ELSE'], operation: 'flow.else' }),

  entry({ lexeme: 'an', namespace: 'root', meaning: 'air, wind, or breath', category: 'element', tags: ['AIR', 'INSPIRATION'] }),
  entry({ lexeme: 'er', namespace: 'root', meaning: 'earth, ground, or foundation', category: 'element', tags: ['EARTH', 'FOUNDATION'] }),
  entry({ lexeme: 'ig', namespace: 'root', meaning: 'fire, passion, or will', category: 'element', tags: ['FIRE', 'WILL'] }),
  entry({ lexeme: 'on', namespace: 'root', meaning: 'water, flow, or feeling', category: 'element', tags: ['FLOW', 'WATER'] }),
  entry({ lexeme: 'um', namespace: 'root', meaning: 'spirit or essence', category: 'element', tags: ['AETHER', 'SPIRIT'] }),
  entry({ lexeme: 'man', namespace: 'root', meaning: 'self or consciousness', category: 'concept', tags: ['CONSCIOUSNESS', 'SELF'] }),
  entry({ lexeme: 'sha', namespace: 'root', meaning: 'shadow or the hidden', category: 'concept', tags: ['HIDDEN', 'SHADOW'] }),
  entry({ lexeme: 'shan', namespace: 'root', meaning: 'darkness or deep shadow', category: 'concept', tags: ['DARKNESS', 'SHADOW'] }),
  entry({ lexeme: 'tir', namespace: 'root', meaning: 'order, honor, or law', category: 'concept', tags: ['HONOR', 'ORDER'] }),
  entry({ lexeme: 'thu', namespace: 'root', meaning: 'chaos or wild possibility', category: 'concept', tags: ['CHAOS', 'POSSIBILITY'] }),
  entry({ lexeme: 'ken', namespace: 'root', meaning: 'knowledge or illumination', category: 'concept', tags: ['KNOWLEDGE', 'LIGHT'] }),
  entry({ lexeme: 'sol', namespace: 'root', meaning: 'light, sun, or soul', category: 'concept', tags: ['LIGHT', 'SOUL', 'SUN'] }),
  entry({ lexeme: 'lif', namespace: 'root', meaning: 'life or vitality', category: 'concept', tags: ['LIFE', 'VITALITY'] }),
  entry({ lexeme: 'dag', namespace: 'root', meaning: 'clarity or day', category: 'concept', tags: ['CLARITY', 'LIGHT'] }),
  entry({ lexeme: 'nau', namespace: 'root', meaning: 'need, boundary, or limitation', category: 'concept', tags: ['BOUNDARY', 'LIMIT'] }),
  entry({ lexeme: 'rad', namespace: 'root', meaning: 'journey, change, or motion', category: 'concept', tags: ['CHANGE', 'JOURNEY'] }),
  entry({ lexeme: 'ber', namespace: 'root', meaning: 'birth, creation, or growth', category: 'concept', tags: ['CREATION', 'GROWTH'] }),
  entry({ lexeme: 'alg', namespace: 'root', meaning: 'shelter or protection', category: 'concept', tags: ['PROTECT', 'SHELTER'] }),
  entry({ lexeme: 'ing', namespace: 'root', meaning: 'seed or potential', category: 'concept', tags: ['POTENTIAL', 'SEED'] }),
  entry({ lexeme: 'wun', namespace: 'root', meaning: 'joy or unity', category: 'concept', tags: ['JOY', 'UNITY'] }),
  entry({ lexeme: 'yar', namespace: 'root', meaning: 'cycle or time', category: 'concept', tags: ['CYCLE', 'TIME'] }),

  entry({ lexeme: 'kal', namespace: 'verb', meaning: 'invite or invoke symbolically', category: 'action', tags: ['INVITE'], operation: 'action.invoke' }),
  entry({ lexeme: 'lag', namespace: 'verb', meaning: 'release or depart', category: 'action', tags: ['RELEASE'], operation: 'action.release' }),
  entry({ lexeme: 'rad', namespace: 'verb', meaning: 'move or travel', category: 'action', tags: ['MOVE'], operation: 'action.move' }),
  entry({ lexeme: 'alg', namespace: 'verb', meaning: 'protect or shelter', category: 'action', tags: ['PROTECT'], operation: 'action.protect' }),

  entry({ lexeme: 'om', namespace: 'power', meaning: 'balance or harmonize', category: 'power-word', tags: ['BALANCE', 'SPIRIT'], operation: 'action.harmonize' }),
  entry({ lexeme: 'zur', namespace: 'power', meaning: 'shield or hold a boundary', category: 'power-word', tags: ['BOUNDARY', 'SHIELD'], operation: 'action.shield' }),
  entry({ lexeme: 'kai', namespace: 'power', meaning: 'open or reveal', category: 'power-word', tags: ['OPEN', 'REVEAL'], operation: 'action.open' }),
  entry({ lexeme: 'ban', namespace: 'power', meaning: 'banish from symbolic focus', category: 'power-word', tags: ['BANISH', 'RELEASE'], operation: 'action.banish' }),
]);

const canonicalVocabulary = {
  vocabularyId: VOCABULARY_ID,
  version: VOCAB_VERSION,
  entries: HALOIC_VOCAB,
};

export const VOCABULARY_HASH = sha256Hex(stableStringify(canonicalVocabulary));

const BY_LEXEME = new Map<string, readonly VocabularyEntry[]>();
for (const vocabularyEntry of HALOIC_VOCAB) {
  const existing = BY_LEXEME.get(vocabularyEntry.lexeme) ?? [];
  BY_LEXEME.set(vocabularyEntry.lexeme, Object.freeze([...existing, vocabularyEntry]));
}

export function findVocabularyEntries(lexeme: string): readonly VocabularyEntry[] {
  return BY_LEXEME.get(lexeme.toLowerCase()) ?? [];
}

export function findQualifiedVocabularyEntry(namespace: string, lexeme: string): VocabularyEntry | undefined {
  return HALOIC_VOCAB.find((candidate) => candidate.namespace === namespace && candidate.lexeme === lexeme.toLowerCase());
}

export function isVocabularyNamespace(value: string): value is VocabularyNamespace {
  return ['prefix', 'root', 'suffix', 'verb', 'particle', 'power'].includes(value);
}
