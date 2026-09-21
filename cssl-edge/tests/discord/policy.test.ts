/**
 * The consent gate, attacked.
 *
 * specs/discord/CONSENT_POLICY.md states its own falsifier: "If the bridge can be made to answer,
 * log, or remember a message that was neither a DM from the owner, nor a mention, nor a reply to
 * itself, nor in an opted-in channel -- this policy is not implemented, regardless of what the
 * code says it does. The test suite must demonstrate the discard by trying to get through it."
 *
 * So this file is mostly attempts to get through. A test that only confirms the happy path would
 * pass just as well against a bridge that answers everything, which is precisely the bridge this
 * policy exists to prevent.
 */
import {
  auditLine,
  decide,
  mayPersist,
  type InboundMessage,
  type PolicyState,
} from '@/scripts/apocrypha-discord/policy';

function assert(cond: boolean, message: string): asserts cond {
  if (!cond) throw new Error('assert failed: ' + message);
}

const OWNER = '11111111111111111';
const BOT = '22222222222222222';
const STRANGER = '33333333333333333';
const OPT_IN = '44444444444444444';
const RANDOM_CHANNEL = '55555555555555555';

function state(over: Partial<PolicyState> = {}): PolicyState {
  return {
    ownerId: OWNER,
    botId: BOT,
    optInChannels: new Set([OPT_IN]),
    halted: new Set<string>(),
    ...over,
  };
}

function msg(over: Partial<InboundMessage> = {}): InboundMessage {
  return {
    id: 'm1',
    channelId: RANDOM_CHANNEL,
    guildId: 'g1',
    authorId: STRANGER,
    authorIsBot: false,
    content: 'hello',
    mentions: [],
    referencedMessageId: null,
    referencedAuthorId: null,
    ...over,
  };
}

// ---------------------------------------------------------------------------------------------
// The attacks. Every one of these must be discarded.
// ---------------------------------------------------------------------------------------------

const ATTACKS: [string, InboundMessage][] = [
  ['an ordinary message in a channel it can see',
    msg()],
  ['a message that mentions somebody else',
    msg({ mentions: ['99999999999999999'] })],
  ['a message that merely CONTAINS the bot id as text',
    msg({ content: 'the bot is <@' + BOT + '> apparently', mentions: [] })],
  ['a reply to a third party, not to the bot',
    msg({ referencedMessageId: 'x', referencedAuthorId: STRANGER })],
  ['a DM from someone who is not the owner',
    msg({ guildId: null, authorId: STRANGER })],
  ['another bot talking, even when it mentions us',
    msg({ authorIsBot: true, mentions: [BOT] })],
  ['our own message echoed back, even mentioning us',
    msg({ authorId: BOT, mentions: [BOT] })],
  ['an empty mention with no text',
    msg({ authorId: OWNER, mentions: [BOT], content: '   ' })],
  ['a message in a channel that is NOT opted in, from the owner, unaddressed',
    msg({ authorId: OWNER })],
];

function testEveryUnaddressedMessageIsDiscarded(): void {
  for (const [what, m] of ATTACKS) {
    const d = decide(m, state());
    assert(d.act === 'ignore', 'must be ignored: ' + what + ' (got ' + d.act + '/' + d.reason + ')');
    assert(!mayPersist(d), 'must never be remembered: ' + what);
  }
}

// ---------------------------------------------------------------------------------------------
// The null. If the checks above passed because everything is ignored, these catch it.
// ---------------------------------------------------------------------------------------------

const ADMITTED: [string, InboundMessage, string][] = [
  ['a DM from the owner', msg({ guildId: null, authorId: OWNER }), 'owner_dm'],
  ['a mention in a guild', msg({ mentions: [BOT] }), 'mentioned'],
  ['a reply to one of our own messages', msg({ referencedAuthorId: BOT }), 'reply_to_self'],
  ['any message in an opted-in channel', msg({ channelId: OPT_IN }), 'opt_in_channel'],
  ['a stranger mentioning us in public', msg({ authorId: STRANGER, mentions: [BOT] }), 'mentioned'],
];

function testAddressedMessagesAreAdmitted(): void {
  for (const [what, m, reason] of ADMITTED) {
    const d = decide(m, state());
    assert(d.act === 'answer', 'must be answered: ' + what + ' (got ' + d.act + '/' + d.reason + ')');
    assert(d.reason === reason, what + ' should report reason ' + reason + ', got ' + d.reason);
  }
}

function testOnlyTheOwnerIsRemembered(): void {
  const strangerTurn = decide(msg({ authorId: STRANGER, mentions: [BOT] }), state());
  assert(strangerTurn.act === 'answer', 'a stranger who addresses us is answered');
  assert(!strangerTurn.owner, 'a stranger is not the owner');
  assert(!mayPersist(strangerTurn), 'a stranger exchange must NOT reach memory (R3)');

  const ownerTurn = decide(msg({ authorId: OWNER, mentions: [BOT] }), state());
  assert(ownerTurn.act === 'answer' && ownerTurn.owner, 'the owner is answered as owner');
  assert(mayPersist(ownerTurn), 'the owner exchange is the one that may be remembered');
}

// ---------------------------------------------------------------------------------------------
// Revocation
// ---------------------------------------------------------------------------------------------

function testStopHalts(): void {
  const d = decide(msg({ mentions: [BOT], content: 'stop' }), state());
  assert(d.act === 'halt', '"stop" addressed to us must halt, got ' + d.act);

  for (const phrase of ['Stop', 'STOP.', ' leave ', 'go away!']) {
    const each = decide(msg({ mentions: [BOT], content: phrase }), state());
    assert(each.act === 'halt', 'should halt on ' + JSON.stringify(phrase) + ', got ' + each.act);
  }
}

function testStopIsNotASubstringMatch(): void {
  // The failure this prevents: someone says "don't stop, keep going" and is silently muted,
  // with no way to discover why it went quiet.
  for (const phrase of ['do not stop', "don't stop explaining", 'stop by later', 'we should stop soon']) {
    const d = decide(msg({ mentions: [BOT], content: phrase }), state());
    assert(d.act === 'answer', JSON.stringify(phrase) + ' must NOT halt, got ' + d.act);
  }
}

function testStopAcrossTheRoomDoesNotHalt(): void {
  // "stop" said to another person, not to us. Honouring it would mean we were reading a
  // conversation we were never part of, and acting on it.
  const d = decide(msg({ content: 'stop', mentions: [] }), state());
  assert(d.act === 'ignore' && d.reason === 'not_addressed',
    'an unaddressed "stop" must be ignored, not treated as a command; got ' + d.act);
}

function testHaltedUserStaysHalted(): void {
  const halted = state({ halted: new Set([STRANGER]) });
  const d = decide(msg({ authorId: STRANGER, mentions: [BOT], content: 'hello again' }), halted);
  assert(d.act === 'ignore' && d.reason === 'halted', 'a halted person stays halted');
}

function testOnlyTheOwnerCanResume(): void {
  const halted = state({ halted: new Set([STRANGER, OWNER]) });

  const strangerTry = decide(msg({ authorId: STRANGER, mentions: [BOT], content: 'resume' }), halted);
  assert(strangerTry.act === 'ignore',
    'a halted stranger must not be able to un-halt themselves, got ' + strangerTry.act);

  const ownerTry = decide(msg({ authorId: OWNER, mentions: [BOT], content: 'resume' }), halted);
  assert(ownerTry.act === 'resume', 'the owner can lift their own halt, got ' + ownerTry.act);
}

// ---------------------------------------------------------------------------------------------
// R2: a discarded message leaves no trace
// ---------------------------------------------------------------------------------------------

function testAuditNeverCarriesContent(): void {
  const secret = 'SENSITIVE-PRIVATE-TEXT-9f3a';
  for (const [, m] of [...ATTACKS, ...ADMITTED.map((a) => [a[0], a[1]] as const)]) {
    const withSecret = { ...m, content: secret };
    const line = JSON.stringify(auditLine(withSecret, decide(withSecret, state())));
    assert(!line.includes(secret),
      'message text reached the audit log: ' + line);
    assert(!line.includes('content'), 'the audit line must not even have a content field');
  }
}

function testAuditCarriesEnoughToDebug(): void {
  // The null for the test above: if auditLine returned {} it would pass trivially.
  const m = msg({ mentions: [BOT] });
  const line = auditLine(m, decide(m, state()));
  for (const key of ['message', 'channel', 'guild', 'author', 'act', 'reason', 'owner']) {
    assert(key in line, 'the audit line must include ' + key);
  }
  assert(line.act === 'answer', 'the audit line records the decision');
}

// ---------------------------------------------------------------------------------------------
// Before READY, botId is empty. Nothing may be treated as a mention of an unknown id.
// ---------------------------------------------------------------------------------------------

function testNoBotIdMeansNoMentionMatch(): void {
  const early = state({ botId: '' });
  const d = decide(msg({ mentions: [''] }), early);
  assert(d.act === 'ignore', 'an empty botId must not match an empty mention entry');

  // The owner's DM still works before READY, because it does not depend on knowing our own id.
  const dm = decide(msg({ guildId: null, authorId: OWNER }), early);
  assert(dm.act === 'answer', 'an owner DM is addressed regardless of botId');
}

const TESTS: [string, () => void][] = [
  ['every unaddressed message is discarded (the falsifier)', testEveryUnaddressedMessageIsDiscarded],
  ['addressed messages ARE admitted, so the gate is not simply closed', testAddressedMessagesAreAdmitted],
  ['only the owner may be remembered', testOnlyTheOwnerIsRemembered],
  ['stop / leave / go away halt the speaker', testStopHalts],
  ['stop is a whole-message match, not a substring', testStopIsNotASubstringMatch],
  ['an unaddressed stop is not obeyed', testStopAcrossTheRoomDoesNotHalt],
  ['a halted person stays halted', testHaltedUserStaysHalted],
  ['only the owner can lift a halt', testOnlyTheOwnerCanResume],
  ['the audit log never carries message text', testAuditNeverCarriesContent],
  ['the audit log still carries enough to debug', testAuditCarriesEnoughToDebug],
  ['before READY, an empty botId matches nothing', testNoBotIdMeansNoMentionMatch],
];

let failed = 0;
for (const [name, fn] of TESTS) {
  try {
    fn();
    console.log('  ok   ' + name);
  } catch (err) {
    failed += 1;
    console.error('  FAIL ' + name + '\n       ' + (err as Error).message);
  }
}
console.log(failed === 0
  ? String(TESTS.length) + ' policy tests passed'
  : String(failed) + ' of ' + String(TESTS.length) + ' policy tests FAILED');
if (failed > 0) process.exitCode = 1;
