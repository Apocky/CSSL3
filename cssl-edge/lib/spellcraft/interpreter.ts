import type {
  ResolvedMorpheme,
  SpellExpression,
  SpellInterpretation,
  SpellProgram,
  SpellTerm,
} from './types';
import { validateSpellProgram } from './validator';

const OPERATION_PHRASES: Readonly<Record<string, string>> = Object.freeze({
  'intent.cause': 'bring forth',
  'intent.negate': 'release the inverse of',
  'intent.aspire': 'aspire toward',
  'intent.amplify': 'intensify',
  'intent.origin': 'return to the origin of',
  'modifier.transform': 'as transformation',
  'modifier.state': 'as a state',
  'modifier.agent': 'as a bearer',
  'modifier.collective': 'as a collective',
  'modifier.abstract': 'as an abstraction',
  'modifier.place': 'as a realm',
  'modifier.continuous': 'in continuing motion',
  'action.invoke': 'invite',
  'action.release': 'release',
  'action.move': 'move with',
  'action.protect': 'protect',
  'action.harmonize': 'harmonize',
  'action.shield': 'hold a boundary around',
  'action.open': 'open or reveal',
  'action.banish': 'remove from reflective focus',
});

function phraseFor(morpheme: ResolvedMorpheme): string {
  return morpheme.operation ? (OPERATION_PHRASES[morpheme.operation] ?? morpheme.meaning) : morpheme.meaning;
}

function interpretTerm(term: SpellTerm): string {
  const prefixes = term.morphemes.filter((morpheme) => morpheme.namespace === 'prefix');
  const heads = term.morphemes.filter((morpheme) => ['root', 'verb', 'power'].includes(morpheme.namespace));
  const suffixes = term.morphemes.filter((morpheme) => morpheme.namespace === 'suffix');
  const action = heads.find((head) => head.operation?.startsWith('action.'));
  const concepts = heads.filter((head) => head !== action).map((head) => head.meaning);
  const pieces = [
    ...prefixes.map(phraseFor),
    ...(action ? [phraseFor(action)] : []),
    ...(concepts.length > 0 ? [concepts.join(' with ')] : action ? [] : heads.map((head) => head.meaning)),
    ...suffixes.map(phraseFor),
  ];
  return pieces.join(' ').trim();
}

function interpretExpression(expression: SpellExpression): string {
  const terms = expression.terms.map(interpretTerm);
  const finalTerm = expression.terms.at(-1);
  const finalHasAction = finalTerm?.morphemes.some((morpheme) => morpheme.operation?.startsWith('action.')) ?? false;
  if (finalHasAction && terms.length > 1) {
    const predicate = terms.at(-1) ?? '';
    return `${predicate} ${terms.slice(0, -1).join(' and ')}`.trim();
  }
  return terms.join(' · ');
}

function traceTerms(program: SpellProgram): readonly SpellTerm[] {
  if (program.form === 'conditional') {
    return [
      ...program.condition.terms,
      ...program.then.terms,
      ...(program.else ? program.else.terms : []),
    ];
  }
  return program.body.terms;
}

export function interpretSpell(program: SpellProgram): SpellInterpretation {
  const validation = validateSpellProgram(program);
  if (!validation.valid) throw new TypeError('Cannot interpret a spell program that failed validation.');
  let text: string;
  if (program.form === 'conditional') {
    const elseText = program.else ? `; otherwise, ${interpretExpression(program.else)}` : '';
    text = `If ${interpretExpression(program.condition)}, reflect on ${interpretExpression(program.then)}${elseText}.`;
  } else {
    text = `${interpretExpression(program.body)}.`;
  }

  const trace = traceTerms(program).map((term) => Object.freeze({
    term: term.source,
    gloss: interpretTerm(term),
    vocabularyRefs: Object.freeze(term.morphemes.map((morpheme) => morpheme.id)),
    confidence: Number((term.morphemes.reduce((sum, morpheme) => sum + morpheme.confidence, 0) / term.morphemes.length).toFixed(3)),
  }));

  return Object.freeze({
    kind: 'reflective-symbolic-interpretation',
    text,
    disclaimer: 'A creative reflection generated from declared symbols. It does not execute effects or establish metaphysical, medical, legal, or factual truth.',
    trace: Object.freeze(trace),
  });
}
