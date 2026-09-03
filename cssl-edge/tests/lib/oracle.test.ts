import assert from 'node:assert/strict';

import { drawOracle, isHighStakesOracleQuestion, normalizeOracleQuestion, ORACLE_MAX_QUESTION_LENGTH } from '../../lib/oracle';

function main(): void {
  assert.equal(normalizeOracleQuestion('  Should   I begin?  '), 'Should I begin?');

  const first = drawOracle('Should I begin?', 'fixed-seed');
  const repeat = drawOracle(' Should  I begin? ', 'fixed-seed');
  assert.deepEqual(first, repeat, 'the same normalized question and seed must be reproducible');
  assert.ok(first.signal === 'yes' || first.signal === 'no');
  assert.ok(first.clarity >= 55 && first.clarity <= 95);
  assert.match(first.receipt, /^apocky-oracle\/1\.0\.0:[0-9a-f]{16}$/);

  assert.throws(() => drawOracle('', 'seed'), /APX-ORACLE-QUESTION-REQUIRED/);
  assert.throws(() => drawOracle('x'.repeat(ORACLE_MAX_QUESTION_LENGTH + 1), 'seed'), /APX-ORACLE-QUESTION-TOO-LONG/);
  assert.throws(() => drawOracle('Question?', ''), /APX-ORACLE-SEED-REQUIRED/);
  assert.equal(isHighStakesOracleQuestion('Should I change my medication dose?'), true);
  for (const adversarialQuestion of [
    'Should I stop taking insulin?',
    'Should I plead guilty?',
    'Should I wire my life savings?',
    'Should I drive after drinking?',
    'Should I threaten my neighbor?',
  ]) {
    assert.equal(isHighStakesOracleQuestion(adversarialQuestion), true, adversarialQuestion);
    assert.throws(() => drawOracle(adversarialQuestion, 'seed'), /APX-ORACLE-HIGH-STAKES-BLOCKED/);
  }
  assert.throws(() => drawOracle('Should I buy this stock?', 'seed'), /APX-ORACLE-HIGH-STAKES-BLOCKED/);

  // eslint-disable-next-line no-console
  console.log('oracle.test : OK');
}

main();
