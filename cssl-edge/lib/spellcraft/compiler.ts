import { canonicalizeProgram } from './parser';
import type {
  AllowedSymbolicOperation,
  CompiledNode,
  CompiledSpell,
  SpellExpression,
  SpellProgram,
  SpellTerm,
} from './types';
import { validateSpellProgram } from './validator';

export const ALLOWED_SYMBOLIC_OPERATIONS: readonly AllowedSymbolicOperation[] = Object.freeze([
  'symbolic.reference',
  'symbolic.modify',
  'symbolic.compose',
  'symbolic.call',
  'symbolic.branch',
]);

function sortedUnique(values: readonly string[]): string[] {
  return [...new Set(values)].sort();
}

/** Compile to a bounded, declarative graph. The result is explicitly non-executable. */
export function compileSpell(program: SpellProgram): CompiledSpell {
  const validation = validateSpellProgram(program);
  if (!validation.valid) throw new TypeError('Cannot compile a spell program that failed validation.');
  const operations: CompiledNode[] = [];

  const addNode = (
    operation: AllowedSymbolicOperation,
    inputs: readonly CompiledNode['id'][],
    vocabularyRefs: readonly CompiledNode['vocabularyRefs'][number][],
    tags: readonly string[],
    label: string,
  ): CompiledNode['id'] => {
    if (!ALLOWED_SYMBOLIC_OPERATIONS.includes(operation)) throw new Error('Compiler attempted a non-allowlisted operation.');
    const id = `node-${operations.length + 1}` as const;
    operations.push(Object.freeze({
      id,
      operation,
      inputs: Object.freeze([...inputs]),
      vocabularyRefs: Object.freeze([...vocabularyRefs]),
      tags: Object.freeze(sortedUnique(tags)),
      label,
    }));
    return id;
  };

  const compileTerm = (term: SpellTerm): CompiledNode['id'] => {
    const vocabularyRefs = term.morphemes.map((morpheme) => morpheme.id);
    const tags = term.morphemes.flatMap((morpheme) => morpheme.tags);
    const heads = term.morphemes.filter((morpheme) => ['root', 'verb', 'power'].includes(morpheme.namespace));
    const modifiers = term.morphemes.filter((morpheme) => ['prefix', 'suffix'].includes(morpheme.namespace));
    const actionHeads = heads.filter((head) => head.operation?.startsWith('action.'));
    let output = addNode(
      'symbolic.reference',
      [],
      vocabularyRefs,
      tags,
      heads.map((head) => head.meaning).join(' + '),
    );
    if (modifiers.length > 0) {
      output = addNode(
        'symbolic.modify',
        [output],
        modifiers.map((modifier) => modifier.id),
        modifiers.flatMap((modifier) => modifier.tags),
        modifiers.map((modifier) => modifier.meaning).join(' + '),
      );
    }
    if (actionHeads.length > 0) {
      output = addNode(
        'symbolic.call',
        [output],
        actionHeads.map((head) => head.id),
        actionHeads.flatMap((head) => head.tags),
        actionHeads.map((head) => head.meaning).join(' + '),
      );
    }
    return output;
  };

  const compileExpression = (expression: SpellExpression): CompiledNode['id'] => {
    const termOutputs = expression.terms.map(compileTerm);
    if (termOutputs.length === 1) return termOutputs[0] as CompiledNode['id'];
    return addNode(
      'symbolic.compose',
      termOutputs,
      expression.terms.flatMap((term) => term.morphemes.map((morpheme) => morpheme.id)),
      expression.terms.flatMap((term) => term.morphemes.flatMap((morpheme) => morpheme.tags)),
      'compose symbolic terms',
    );
  };

  let output: CompiledNode['id'];
  if (program.form === 'conditional') {
    const condition = compileExpression(program.condition);
    const thenOutput = compileExpression(program.then);
    const inputs: CompiledNode['id'][] = [condition, thenOutput];
    if (program.else) inputs.push(compileExpression(program.else));
    output = addNode('symbolic.branch', inputs, [], ['CONDITIONAL'], 'reflective if / then / else branch');
  } else {
    output = compileExpression(program.body);
  }

  return Object.freeze({
    kind: 'apocky.symbolic-plan/v1',
    executable: false,
    operations: Object.freeze(operations),
    output,
    canonicalProgram: canonicalizeProgram(program),
  });
}
