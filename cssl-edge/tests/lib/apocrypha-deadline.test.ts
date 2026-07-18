import { DeadlineExceededError, withDeadline } from '../../lib/apocrypha/deadline';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

async function main(): Promise<void> {
  const immediate = await withDeadline(Promise.resolve('ready'), 50);
  assert(immediate === 'ready', 'resolved operation changed value');

  let deadlineHookCalled = false;
  const started = Date.now();
  try {
    await Promise.race([
      withDeadline(new Promise<never>(() => undefined), 10, () => { deadlineHookCalled = true; }),
      new Promise<never>((_, reject) => setTimeout(() => reject(new Error('outer watchdog')), 100)),
    ]);
    throw new Error('never-resolving operation unexpectedly completed');
  } catch (error) {
    assert(error instanceof DeadlineExceededError, 'never-resolving operation did not report deadline');
  }
  assert(deadlineHookCalled, 'deadline hook was not called');
  assert(Date.now() - started < 100, 'deadline did not settle before outer watchdog');

  const expected = new Error('source failure');
  try {
    await withDeadline(Promise.reject(expected), 50);
    throw new Error('rejected operation unexpectedly completed');
  } catch (error) {
    assert(error === expected, 'source rejection was not preserved');
  }

  console.log('apocrypha-deadline.test : OK · resolve, hang, hook, and rejection cases passed');
}

void main();
