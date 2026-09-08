import { drawOracle, type OracleSignal } from '../oracle';
import { analyzeSpell, createSigil, DEFAULT_SPELL_LIMITS, HALOIC_VOCAB, type VocabularyEntry } from '../spellcraft';
import { selectedIntent } from '../../components/spellcraft/intent-presets';

export type ChatCreationTool = 'sigil' | 'reflection';
export const CHAT_TOOL_TEXT_LIMIT = 4_096;
export const CHAT_TOOL_MEANING_LIMIT = 12;

export interface ChatToolResult {
  readonly kind: ChatCreationTool;
  readonly title: string;
  readonly source: string;
  readonly meaning: string;
  readonly prompt: string;
  readonly message: string;
  readonly svg?: string;
  readonly variant?: number;
}

export type ChatToolOutcome = { readonly ok: true; readonly result: ChatToolResult }
  | { readonly ok: false; readonly error: string };

export function createChatToolResult(source: string, kind: ChatCreationTool, variant = 0): ChatToolOutcome {
  if (!source.trim() || source.length > DEFAULT_SPELL_LIMITS.maxInputChars) {
    return { ok: false, error: 'Choose a focus or enter up to 512 symbolic characters.' };
  }
  if (kind !== 'sigil' && kind !== 'reflection') {
    return { ok: false, error: 'Choose a sigil or reflection.' };
  }
  if (!Number.isInteger(variant) || variant < 0 || variant > 255) {
    return { ok: false, error: 'Choose a shape from 1 through 256.' };
  }
  try {
    const analysis = analyzeSpell(source);
    if (analysis.status !== 'valid') {
      return { ok: false, error: 'Those symbolic words could not be read. Try a focus above or choose another combination.' };
    }
    const preset = selectedIntent(analysis.input);
    const title = preset?.label ?? 'Your intention';
    const meaning = analysis.interpretation.text.charAt(0).toUpperCase() + analysis.interpretation.text.slice(1);
    const prompt = preset?.prompt ?? 'What is one small way to put this intention into practice?';
    const message = [kind === 'sigil' ? 'My sigil: ' + title : 'My reflection: ' + title,
      meaning, prompt, 'Symbolic words: ' + analysis.input,
      ...(kind === 'sigil' ? ['Shape: ' + (variant + 1) + '. The image was created in this browser; this message contains its meaning.'] : []),
    ].join('\n\n');
    if (message.length > CHAT_TOOL_TEXT_LIMIT) {
      return { ok: false, error: 'This result is too long to add to a message. Try a shorter intention.' };
    }
    const art = kind === 'sigil' ? createSigil(analysis, { variant }) : undefined;
    return { ok: true, result: { kind, title, source: analysis.input, meaning, prompt, message,
      ...(art ? { svg: art.svg, variant: art.variant } : {}),
    } };
  } catch {
    return { ok: false, error: 'This creation could not be made. Choose a focus and try again.' };
  }
}

export function findSymbolicMeanings(query: string): readonly VocabularyEntry[] {
  const term = query.trim().toLocaleLowerCase('en-US').slice(0, 80);
  if (!term) return HALOIC_VOCAB.filter(entry => entry.namespace === 'root').slice(0, CHAT_TOOL_MEANING_LIMIT);
  return HALOIC_VOCAB.filter(entry => [entry.lexeme, entry.meaning, entry.namespace, ...entry.tags]
    .some(value => value.toLocaleLowerCase('en-US').includes(term)))
    .sort((left, right) => Number(right.lexeme === term) - Number(left.lexeme === term))
    .slice(0, CHAT_TOOL_MEANING_LIMIT);
}

export function meaningForMessage(entry: VocabularyEntry): string {
  return entry.lexeme + ' — ' + entry.meaning + '\nFrom Apocky’s symbolic vocabulary.';
}

export interface ChatOracleResult {
  readonly kind: 'oracle';
  readonly question: string;
  readonly signal: OracleSignal;
  readonly counterweight: string;
  readonly nextQuestion: string;
  readonly message: string;
}

export type ChatOracleOutcome = { readonly ok: true; readonly result: ChatOracleResult }
  | { readonly ok: false; readonly error: string };

export function createChatOracleResult(question: string, seed: string): ChatOracleOutcome {
  try {
    const reading = drawOracle(question, seed);
    const message = ['My oracle reflection: ' + reading.question,
      'Chance-drawn signal: ' + reading.signal.toUpperCase() + '. A reflection prompt, not a prediction.',
      reading.counterweight, reading.nextQuestion,
    ].join('\n\n');
    if (message.length > CHAT_TOOL_TEXT_LIMIT) return { ok: false, error: 'Try a shorter question.' };
    return { ok: true, result: { kind: 'oracle', question: reading.question, signal: reading.signal,
      counterweight: reading.counterweight, nextQuestion: reading.nextQuestion, message,
    } };
  } catch (caught) {
    const code = caught instanceof Error ? caught.message : '';
    return { ok: false, error: code === 'APX-ORACLE-HIGH-STAKES-BLOCKED'
      ? 'This oracle does not answer medical, legal, financial, safety, self-harm, surveillance, coercion, or directed-harm decisions. Use qualified help and real evidence.'
      : code === 'APX-ORACLE-QUESTION-REQUIRED' ? 'Write one question first.'
        : code === 'APX-ORACLE-QUESTION-TOO-LONG' ? 'Keep the question under 280 characters.'
          : 'This reading could not be made. Try again in a current browser.' };
  }
}
