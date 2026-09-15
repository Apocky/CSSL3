// readingForPrompt: what survives when the reading does not fit.
//
// The contract the prompt itself states is "Every card supplied must appear in the reading by
// name: each spread position, each clarifier, and the shadow card." So the renderer has two tiers
// and they are not interchangeable: the IDENTITY line (position, card, reversal) is the contract;
// position descriptions, keywords, meanings and blurbs are enrichment.
//
// This pins the order things are given up in. It exists because the alternative shipped: on
// 2026-09-12 a reading ignored three clarifiers and renamed the shadow card. A renderer that
// truncates with .slice() cuts wherever it lands -- mid-meaning, mid-name, mid-card -- and a
// half-written card name is exactly how a card gets "renamed".

import { readingForPrompt } from '../scripts/apocrypha-worker/prompt';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

const CARDS = [
  { name: 'The Tower', position: { name: 'Pressure' } },
  { name: 'The Star', is_reversed: true, position: { name: 'Response' } },
  { name: 'The Moon', position: { name: 'Clarifier for Pressure' } },
  { name: 'Ten of Swords', position: { name: 'Shadow' } },
];

function reading(cards = CARDS): unknown {
  return {
    system: { name: 'Chaos Tarot' },
    spread: { name: 'Three Signals' },
    items: cards.map((card) => ({
      ...card,
      position: { ...card.position, description: 'what presses on the querent right now' },
      meanings: {
        upright: 'X'.repeat(400),
        reversed: 'Y'.repeat(400),
        keywords: ['upheaval', 'revelation', 'collapse'],
        keywords_reversed: ['delay', 'withheld hope'],
        description: 'Z'.repeat(300),
      },
    })),
  };
}

function identitiesPresent(text: string, cards = CARDS): void {
  for (const card of cards) {
    assert(text.includes(card.name), `card "${card.name}" was dropped`);
    assert(text.includes(card.position.name), `position "${card.position.name}" was dropped`);
    const line = text.split('\n').find((row) => row.includes(card.name));
    assert(line !== undefined && line.includes(card.position.name),
      `card "${card.name}" was separated from its position`);
    if ('is_reversed' in card && card.is_reversed) {
      assert(/revers/i.test(line ?? ''), `reversal of "${card.name}" was lost`);
    }
  }
}

async function main(): Promise<void> {
  // 1 — room for everything: enrichment is present.
  const full = readingForPrompt(reading(), 100_000);
  identitiesPresent(full);
  assert(full.includes('Keywords:') && full.includes('Meaning:') && full.includes('Position means:'),
    'detail was shed when there was room for it');
  assert(full.includes('Chaos Tarot') && full.includes('Three Signals'), 'header was lost');
  assert(full.includes('Clarifier for Pressure'), 'the clarifier relationship was flattened away');
  assert(full.includes('Shadow'), 'the shadow card lost its position');

  // 2 — squeezed: detail goes, every card stays.
  const squeezed = readingForPrompt(reading(), 600);
  assert(squeezed.length <= 600, `squeezed render overran its budget at ${squeezed.length} chars`);
  identitiesPresent(squeezed);
  assert(!squeezed.includes('Meaning:'), 'enrichment survived while the budget was tight');

  // 3 — the ordering itself: detail must be given up BEFORE any card is.
  //    Sweep the budget downward and assert no card disappears while any detail remains.
  for (let budget = 2_000; budget >= 400; budget -= 100) {
    const text = readingForPrompt(reading(), budget);
    const anyDetail = /Meaning:|Keywords:|Position means:|About:/.test(text);
    const allCards = CARDS.every((card) => text.includes(card.name));
    if (anyDetail) assert(allCards, `a card was dropped at budget ${budget} while detail was still present`);
    assert(text.length <= budget, `render overran its budget at ${budget}`);
  }

  // 3b — room between the tiers must actually be USED for enrichment.
  //    Without this, a renderer that skips straight from "everything" to "bare identities" passes
  //    every other check here while quietly throwing away detail it had room for. Measured against
  //    the identity-only size rather than a guessed constant, so it survives fixture changes.
  const identityOnly = readingForPrompt(reading(), 600).length;
  const roomy = readingForPrompt(reading(), Math.round(identityOnly * 3));
  identitiesPresent(roomy);
  assert(/Meaning:|Keywords:|Position means:|About:/.test(roomy),
    'the renderer left enrichment out while it still had room for it');
  assert(!/omitted for length/.test(roomy), 'a complete reading declared itself incomplete');

  // 4 — genuinely cannot fit: cards are dropped from the END and the loss is STATED.
  const tiny = readingForPrompt(reading(), 120);
  assert(tiny.length <= 120, `tiny render overran its budget at ${tiny.length} chars`);
  assert(/omitted for length/.test(tiny), 'cards were dropped without saying so');
  assert(/this reading is incomplete/.test(tiny), 'a truncated reading did not declare itself incomplete');

  // 5 — a card name is never cut in half. Every card mentioned is mentioned WHOLE.
  for (const budget of [120, 200, 300, 450, 600, 900]) {
    const text = readingForPrompt(reading(), budget);
    for (const card of CARDS) {
      const partial = card.name.slice(0, Math.max(4, card.name.length - 2));
      if (text.includes(partial)) {
        assert(text.includes(card.name), `"${card.name}" appears truncated at budget ${budget} -- that is how a card gets renamed`);
      }
    }
  }

  // 6 — an empty or absent reading renders nothing rather than an empty scaffold.
  assert(readingForPrompt(undefined) === '', 'absent reading produced text');
  assert(readingForPrompt({}) === '', 'empty reading produced text');

  console.log('apocrypha-reading-render.test: tiering, budget sweep 2000->400, omission declared, no half-named cards');
}

main().then(() => console.log('apocrypha-reading-render OK')).catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
