// § Actual chat components + actual CSS ; only boundary substitutions @ preview server
import { createRoot } from 'react-dom/client';
import { useState } from 'react';
import ApocryphaChat from '../../components/apocrypha/ApocryphaChat';
import type { ChatLane, LaneMessage } from '../../lib/apocrypha/chat-lanes';
import { GUEST_MESSAGE_MAX_BYTES, MEMBER_MESSAGE_MAX_BYTES } from '../../lib/apocrypha/chat-limits';

// A lane with OWNER capabilities and synthetic data, so the preview shows the whole signed-in
// surface — conversation list, settings, tool trace — without any production credential.
const previewMessages: LaneMessage[] = [
  { id: 'a:user', role: 'user', text: 'Help me turn a crowded day into one useful next step.', at: new Date('2026-09-06T17:00:00Z') },
  {
    id: 'a:apocrypha', role: 'apocrypha', at: new Date('2026-09-06T17:00:20Z'),
    text: 'Start with what matters today.\n\nName one thing you want to protect, choose an action small enough to finish, and leave room to change your mind.',
    tools: [
      { name: 'memory.recall', ok: true, elapsed_ms: 142 },
      { name: 'engine.generate', ok: true, elapsed_ms: 8410 },
      { name: 'ledger.write', ok: false, elapsed_ms: 12, error: 'write-back disabled by default' },
    ],
  },
];
// The fixture used to pin OWNER capabilities and return done:true on the first poll, which meant
// the one surface it could not show was the DEFAULT one: a guest, waiting. Three of the room's
// worst defects only exist on a slow guest job, so they were unreproducible in a browser and could
// only be argued about from source. Both axes are now query parameters.
//
// Capability rows are copied verbatim from lib/apocrypha/chat-lanes.ts (guest :133, member :198,
// owner :263) -- if those drift, this fixture is lying and the drift is the bug.
const CAPS = {
  guest: { conversations: false, newConversation: false, trace: false, cancel: false, durableHistory: false, byteLimit: GUEST_MESSAGE_MAX_BYTES },
  member: { conversations: true, newConversation: true, trace: false, cancel: false, durableHistory: true, byteLimit: MEMBER_MESSAGE_MAX_BYTES },
  owner: { conversations: true, newConversation: true, trace: true, cancel: true, durableHistory: true, byteLimit: null },
} as const;

const params = new URLSearchParams(location.search);
const capsKey = (['guest', 'member', 'owner'] as const).find((k) => k === params.get('caps')) ?? 'owner';
const jobKind = (['slow', 'fail-mid', 'fast'] as const).find((k) => k === params.get('job')) ?? 'fast';

// Carries a heading, a list and a fenced block, so "does the room render markdown" is answerable in
// the same scenario that answers "is a partial answer kept".
const STREAMED = [
  '# A useful next step',
  '',
  'Three things, in order:',
  '',
  '- name what you want to protect',
  '- choose an action small enough to finish',
  '- leave room to change your mind',
  '',
  'Here is the shape of it:',
  '',
  '```python',
  'def next_step(day):',
  '    protected = pick_one(day.commitments)',
  '    return smallest_finishable(protected)',
  '```',
  '',
  'The **point** is not the list. It is finishing one thing.',
].join(String.fromCharCode(10));

let tick = 0;

const previewLane: ChatLane = {
  id: capsKey,
  capabilities: CAPS[capsKey],
  async send() { tick = 0; return { jobId: 'preview-job', conversationId: 'f1000000-0000-4000-8000-000000000001' }; },
  async poll() {
    tick += 1;
    // Never settles. This is the state that disables the composer and, today, offers no way out.
    if (jobKind === 'slow') return { done: false, status: 'queued' as const, text: '' };
    if (jobKind === 'fail-mid') {
      // Grow for six ticks, then die holding text -- the case where the partial is discarded.
      const shown = STREAMED.slice(0, Math.floor((STREAMED.length / 6) * tick));
      if (tick < 6) return { done: false, status: 'running' as const, text: shown };
      return { done: true, status: 'failed' as const, text: STREAMED, failure: 'ENGINE_DROPPED' };
    }
    return { done: true, status: 'succeeded' as const, text: STREAMED };
  },
  async listConversations() {
    return [
      { id: 'f1000000-0000-4000-8000-000000000001', title: 'A crowded day', lastActiveIso: '2026-09-06T17:00:20Z', messageCount: 2 },
      { id: 'f1000000-0000-4000-8000-000000000002', title: 'Something much longer that has to ellipsize in the rail', lastActiveIso: '2026-09-05T11:02:00Z', messageCount: 8 },
    ];
  },
  async loadConversation() { return previewMessages; },
  async cancel() { /* nothing to cancel in a fixture */ },
};
import BrainExperience from '../../components/brain/BrainExperience';
// NOTE: WorkConsole cannot be mounted here. Importing it pulls a CommonJS dependency into the
// browser bundle and the whole fixture dies with "module is not defined" -- for EVERY mode, not
// just its own. Verify the Work console against a running instance instead.
import { FeedbackProvider } from '../../components/ui/Feedback';
import '../../styles/apocky-system.css';
import '../../styles/apocky-redesign.css';
import { mode, pendingFixture, prepareFixture } from './chat-room-boundaries';

function FixtureControls(): JSX.Element {
  const [inspection, setInspection] = useState('');
  async function inspect(): Promise<void> {
    const fixture = (window as Window & { chatRoomFixture?: { inspect: () => Promise<unknown> } }).chatRoomFixture;
    try { setInspection(fixture ? JSON.stringify(await fixture.inspect(), null, 2) : 'Fixture inspector is unavailable.'); }
    catch { setInspection('Fixture inspection failed. No service request was made.'); }
  }
  return <details style={{ position: 'fixed', bottom: 4, right: 4, zIndex: 1000, maxWidth: 'min(340px, calc(100vw - 8px))', background: '#101322', color: '#eef2ff', border: '1px solid #526070', borderRadius: 8, padding: '4px 8px', font: '12px/1.4 system-ui' }}>
      <summary style={{ minHeight: 24, cursor: 'pointer' }}>Local UI fixture · {mode} · {capsKey} · {jobKind}{pendingFixture ? ' · pending' : ''}</summary>
      <p>Synthetic data. Owner vault is mocked; account pending storage uses the real browser journal on this local origin. No production connection.</p>
      <nav aria-label="Fixture scenarios" style={{ display: 'flex', gap: 10, flexWrap: 'wrap' }}>
        <a href="/?mode=account&pending=0">Account</a><a href="/?mode=account&pending=1">Account pending</a><a href="/?mode=owner&pending=0">Owner</a><a href="/?mode=owner&pending=1">Owner pending</a>
        <a href="/?mode=account&caps=guest&job=slow">Guest · slow (stuck)</a>
        <a href="/?mode=account&caps=guest&job=fail-mid">Guest · fails mid-answer</a>
        <a href="/?mode=account&caps=owner&job=fail-mid">Owner · fails mid-answer</a>
        <a href="/?mode=account&caps=member&job=fast">Member · fast</a>
      </nav>
      <button type="button" style={{ minHeight: 44, marginTop: 12, padding: '8px 12px' }} onClick={() => { void inspect(); }}>Inspect UI test state</button>
      {inspection ? <pre role="status" aria-label="UI fixture state" style={{ maxHeight: '50dvh', overflow: 'auto', whiteSpace: 'pre-wrap', overflowWrap: 'anywhere', font: '12px/1.5 monospace' }}>{inspection}</pre> : null}
    </details>;
}

async function start(): Promise<void> {
  await prepareFixture();
  const element = document.getElementById('root'); if (!element) throw new Error('Fixture root missing.');
  createRoot(element).render(<FeedbackProvider>
    <FixtureControls />
    {mode === 'owner' ? <BrainExperience serverAccess="owner" /> : <ApocryphaChat lane={previewLane} signedIn />}
  </FeedbackProvider>);
}
void start().catch(error => { const element = document.getElementById('root'); if (element) element.textContent = 'Fixture failed: ' + String(error); });
