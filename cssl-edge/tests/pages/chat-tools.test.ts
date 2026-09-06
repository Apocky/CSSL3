// § ChatTools behavior · actual engines + bounded React-event harness ; N! browser-acceptance claim
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import ts from 'typescript';
import * as helpers from '../../lib/apocrypha/chat-tools';
import * as presets from '../../components/spellcraft/intent-presets';
import * as oracle from '../../lib/oracle';
import { analyzeSpell, createSigil, HALOIC_VOCAB } from '../../lib/spellcraft';

function engines(): void {
  for (const preset of presets.INTENT_PRESETS) {
    const analysis = analyzeSpell(preset.source);
    if (analysis.status !== 'valid') throw new Error('Preset analysis rejected.');
    const sigil = helpers.createChatToolResult(preset.source, 'sigil', 0);
    const reflection = helpers.createChatToolResult(preset.source, 'reflection');
    assert.equal(sigil.ok, true, preset.label); assert.equal(reflection.ok, true, preset.label);
    if (!sigil.ok || !reflection.ok) throw new Error('Preset creation rejected.');
    assert.equal(sigil.result.svg, createSigil(analysis, { variant: 0 }).svg, 'actual sigil engine defines image');
    assert.equal(sigil.result.meaning, reflection.result.meaning, 'one intention retains meaning across tools');
    assert.equal(sigil.result.source, analysis.input); assert.equal(sigil.result.title, preset.label);
    assert.equal(reflection.result.svg, undefined);
    assert.ok(sigil.result.message.includes('this message contains its meaning'));
    for (const result of [sigil.result, reflection.result]) {
      assert.ok(result.message.length <= helpers.CHAT_TOOL_TEXT_LIMIT);
      assert.ok(result.message.includes(result.meaning)); assert.ok(result.message.includes(result.prompt));
      assert.ok(!result.message.includes('<svg'), 'draft contains meaning; image is explicitly separate');
    }
    const next = helpers.createChatToolResult(preset.source, 'sigil', 1);
    assert.equal(next.ok, true);
    if (next.ok) { assert.notEqual(next.result.svg, sigil.result.svg); assert.equal(next.result.meaning, sigil.result.meaning); }
  }
  for (const source of ['', '   ', 'x'.repeat(513), 'not-a-listed-symbolic-lexeme', '<script>alert(1)</script>']) {
    assert.equal(helpers.createChatToolResult(source, 'sigil').ok, false, source.slice(0, 30));
  }
  for (const variant of [-1, 256, 0.5, NaN, Infinity]) assert.equal(helpers.createChatToolResult('ka-ken-el', 'sigil', variant).ok, false);
  assert.equal(helpers.createChatToolResult('ka-ken-el', 'sigil', 255).ok, true);
  assert.equal(helpers.createChatToolResult('ka-ken-el', 'unsupported' as helpers.ChatCreationTool).ok, false);

  for (const query of ['', '  ', 'ken', 'KNOWLEDGE', 'root', 'creation', 'x'.repeat(90)]) {
    const results = helpers.findSymbolicMeanings(query);
    assert.ok(results.length <= helpers.CHAT_TOOL_MEANING_LIMIT);
    for (const entry of results) assert.ok(HALOIC_VOCAB.includes(entry), 'lookup returns actual vocabulary records');
  }
  assert.equal(helpers.findSymbolicMeanings('  KEN  ')[0]?.lexeme, 'ken', 'exact lexeme sorts first');
  assert.deepEqual(helpers.findSymbolicMeanings('impossible-unlisted-word'), []);
  const found = helpers.findSymbolicMeanings('ken')[0]; assert.ok(found);
  assert.ok(helpers.meaningForMessage(found).includes(found.meaning));
  assert.ok(helpers.meaningForMessage(found).includes('Apocky’s symbolic vocabulary'));

  const question = 'Should I try a small creative experiment today?'; const seed = 'fixed-preview-test-seed';
  const expected = oracle.drawOracle(question, seed); const outcome = helpers.createChatOracleResult(question, seed);
  assert.equal(outcome.ok, true);
  if (outcome.ok) {
    assert.equal(outcome.result.question, expected.question); assert.equal(outcome.result.signal, expected.signal);
    assert.equal(outcome.result.counterweight, expected.counterweight); assert.equal(outcome.result.nextQuestion, expected.nextQuestion);
    assert.ok(outcome.result.message.includes('not a prediction')); assert.ok(outcome.result.message.length <= helpers.CHAT_TOOL_TEXT_LIMIT);
    assert.deepEqual(helpers.createChatOracleResult('  ' + question + '  ', seed), outcome, 'normalization retains reproducibility');
    assert.ok(!outcome.result.message.includes(expected.receipt), 'internal receipt is not presented as user confidence');
  }
  for (const invalid of ['', 'x'.repeat(oracle.ORACLE_MAX_QUESTION_LENGTH + 1), 'Should I change my medication dose?', 'Should I wire my life savings?', 'Should I threaten my neighbor?']) {
    assert.equal(helpers.createChatOracleResult(invalid, seed).ok, false, invalid.slice(0, 40));
  }
  assert.equal(helpers.createChatOracleResult(question, '').ok, false);
}

type Node = { type: unknown; props: Record<string, any> };
function nodes(value: unknown): Node[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Node; return [node, ...nodes(node.props.children)];
}
function text(value: unknown): string {
  if (Array.isArray(value)) return value.map(text).join('');
  if (value && typeof value === 'object' && 'props' in value) return text((value as Node).props.children);
  return typeof value === 'string' || typeof value === 'number' ? String(value) : '';
}
function events(): void {
  const filename = path.join(process.cwd(), 'components/apocrypha/ChatTools.tsx');
  const source = fs.readFileSync(filename, 'utf8');
  const compiled = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, esModuleInterop: true } }).outputText;
  const state: unknown[] = []; let cursor = 0; let seedCalls = 0;
  const effectDeps: Array<readonly unknown[]> = []; let effectCursor = 0; let effects: Array<() => void> = [];
  let focusCount = 0; let scrollCount = 0; let hiddenResult = false; let missingResult = false;
  const jsx = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const module = { exports: {} as { default: (props: { onInsert: (text: string) => void | boolean; disabled?: boolean }) => Node } };
  vm.runInNewContext(compiled, { module, exports: module.exports, require: (name: string) => {
    if (name === 'react') return {
      useRef: (initial: unknown) => { const index = cursor++; if (!(index in state)) state[index] = { current: initial }; return state[index]; },
      useEffect: (effect: () => void, dependencies: readonly unknown[]) => { const index = effectCursor++; const previous = effectDeps[index]; if (!previous || dependencies.some((value, slot) => !Object.is(value, previous[slot]))) effects.push(effect); effectDeps[index] = dependencies; },
      useId: () => 'fixture-tools', useMemo: (calculate: () => unknown) => calculate(), useState: (initial: unknown) => { const index = cursor++; if (!(index in state)) state[index] = initial; return [state[index], (next: unknown) => { state[index] = typeof next === 'function' ? next(state[index]) : next; }]; } };
    if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx, Fragment: 'fragment' };
    if (name.endsWith('intent-presets')) return presets;
    if (name.endsWith('/chat-tools')) return helpers;
    if (name.endsWith('/oracle')) return { ...oracle, createOracleSeed: () => { seedCalls += 1; return 'event-fixture-seed'; } };
    if (name.endsWith('.module.css')) return new Proxy({}, { get: (_target, key) => key });
    throw new Error('Unexpected ChatTools dependency: ' + name);
  } }, { filename });
  const insertions: string[] = []; let accept = true; let disabled = false;
  const render = () => {
    cursor = 0; effectCursor = 0; effects = [];
    const tree = module.exports.default({ disabled, onInsert: value => { if (!accept) return false; insertions.push(value); return true; } });
    for (const node of nodes(tree)) if (node.props.ref && node.type === 'article') node.props.ref.current = missingResult ? null : { closest: () => hiddenResult ? {} : null, focus: () => { focusCount += 1; }, scrollIntoView: () => { scrollCount += 1; } };
    for (const effect of effects) effect();
    return tree;
  };
  const find = (tree: Node, predicate: (node: Node) => boolean) => { const found = nodes(tree).find(predicate); assert.ok(found, 'Expected rendered control'); return found; };
  const button = (tree: Node, label: string) => find(tree, node => node.type === 'button' && text(node).replace(/[✧✎☾]/g, '').trim() === label);
  const submit = (tree: Node) => find(tree, node => node.type === 'form').props.onSubmit({ preventDefault() {} });
  let tree = render(); assert.equal(insertions.length, 0); assert.equal(seedCalls, 0, 'render does not draw an oracle');
  submit(tree); tree = render();
  assert.ok(nodes(tree).some(node => node.props['aria-label'] === 'Your sigil'));
  assert.equal(insertions.length, 0, 'create never inserts or sends automatically');
  assert.equal(focusCount, 1, 'new visible result receives focus'); assert.equal(scrollCount, 1);
  hiddenResult = true; button(tree, 'Try another shape').props.onClick(); tree = render();
  assert.equal(focusCount, 1, 'hidden result does not steal focus'); assert.equal(insertions.length, 0);
  hiddenResult = false; missingResult = true; button(tree, 'Try another shape').props.onClick(); tree = render();
  assert.equal(focusCount, 1, 'absent result DOM is harmless'); assert.equal(insertions.length, 0);
  missingResult = false;
  const firstImage = find(tree, node => node.type === 'img').props.src;
  button(tree, 'Try another shape').props.onClick(); tree = render();
  assert.notEqual(find(tree, node => node.type === 'img').props.src, firstImage); assert.equal(insertions.length, 0);
  button(tree, 'Add to message').props.onClick(); tree = render(); assert.equal(insertions.length, 1); assert.match(text(tree), /Review it before sending/);
  accept = false; button(tree, 'Add to message').props.onClick(); tree = render(); assert.equal(insertions.length, 1); assert.match(text(tree), /will not fit/); assert.doesNotMatch(text(tree), /Added to your message/);
  accept = true; disabled = true; tree = render();
  assert.equal(button(tree, 'Add to message').props.disabled, true);
  button(tree, 'Add to message').props.onClick(); assert.equal(insertions.length, 1, 'disabled handler also refuses insertion');
  disabled = false; tree = render(); button(tree, 'Growth').props.onClick(); tree = render();
  assert.ok(!nodes(tree).some(node => node.props['aria-label'] === 'Your sigil'), 'changed intention removes stale result');
  button(tree, 'Reflection').props.onClick(); tree = render(); submit(tree); tree = render();
  assert.ok(nodes(tree).some(node => node.props['aria-label'] === 'Your reflection')); assert.equal(insertions.length, 1);
  button(tree, 'Add to message').props.onClick(); assert.equal(insertions.length, 2); assert.match(insertions[1]!, /^My reflection: Growth/);
  button(render(), 'Oracle').props.onClick(); tree = render(); assert.equal(seedCalls, 0, 'switching tool does not draw');
  find(tree, node => node.type === 'textarea').props.onChange({ target: { value: 'Should I begin a small drawing today?' } });
  tree = render(); submit(tree); tree = render(); assert.equal(seedCalls, 1); assert.equal(insertions.length, 2);
  assert.ok(nodes(tree).some(node => node.props['aria-label'] === 'Your oracle reflection'));
  button(tree, 'Add to message').props.onClick(); assert.equal(insertions.length, 3); assert.match(insertions[2]!, /^My oracle reflection:/);
  find(render(), node => node.type === 'textarea').props.onChange({ target: { value: 'Should I change my medication dose?' } });
  tree = render(); submit(tree); tree = render(); assert.ok(nodes(tree).some(node => node.props.role === 'alert'));
  assert.ok(!nodes(tree).some(node => node.props['aria-label'] === 'Your oracle reflection')); assert.equal(insertions.length, 3);
}
engines(); events();
console.log('chat-tools.test : OK — engine equivalence, boundaries, and explicit insertion events; browser acceptance separate');
