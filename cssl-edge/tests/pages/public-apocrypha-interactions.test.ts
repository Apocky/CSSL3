import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const component = readFileSync(
  resolve(process.cwd(), 'components/apocrypha/PublicChat.tsx'),
  'utf8',
);
const style = readFileSync(
  resolve(process.cwd(), 'styles/PublicApocrypha.module.css'),
  'utf8',
);

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

for (const token of [
  'className={styles.composerShell}',
  'className={styles.lensButton}',
  "surface?.kind === 'intent'",
  "surface?.kind === 'conversation'",
  "surface?.kind === 'message'",
  "surface?.kind === 'scope'",
]) {
  assert(component.includes(token), `missing contextual interaction seam: ${token}`);
}

assert(component.includes('onContextMenu={(event)'), 'blank-canvas and message right-click entry points are missing');
assert(component.includes("event.shiftKey && event.key === 'F10'"), 'message context actions lack a keyboard equivalent');
assert(component.includes("event.key.toLowerCase() === 'k'"), 'intent lens shortcut is missing');
assert(component.includes('aria-pressed={candidate.id === mode}'), 'active intent is not exposed accessibly');
assert(component.includes('navigateMessageMenu'), 'message menu lacks arrow-key navigation');
assert(component.includes("surface.kind === 'scope'"), 'effect scope is not a distinct interaction layer');
assert(component.includes("querySelector<HTMLElement>('textarea')"), 'effect scope does not receive focus when opened');
assert(component.includes('aria-invalid={duplicateCodePath || codePaths.length > 32}'), 'effect scope does not expose invalid path state');
assert(component.includes("conversation_history === 'durable_principal_bound'"), 'durable runtime sessions are rejected by the client envelope');
assert(component.includes("'/api/apocrypha/sessions'"), 'durable session discovery is not wired into the conversation');
assert(component.includes('fetchSessionSnapshot'), 'durable thread restoration is missing');
assert(component.includes('session_id: dispatchConversationId'), 'new turns do not use the fenced durable session contract');
assert(component.includes('ACTIVE_SESSION_KEY'), 'the active durable thread cannot survive a page reload');
assert(component.includes('styles.recentThreads'), 'recent threads are not grouped inside contextual conversation actions');
assert(component.includes("window.localStorage.removeItem(ACTIVE_SESSION_KEY)"), 'signed-out state does not clear the local thread pointer');
assert(component.includes('loadedStoredSessionRef.current'), 'an active worldline outside the recent-session window cannot be recovered directly');
assert(component.includes('hidden history will not be reused'), 'recovery failure can silently resume unseen runtime history');
assert(component.includes('generation === sessionGenerationRef.current'), 'stale session responses are not fenced from current auth state');
assert(component.includes('DURABLE_UUID_PATTERN'), 'principal-scoped UUIDv5 turn identities cannot survive restoration');
assert(component.includes('record.turn_states'), 'pending and failed turns are flattened into completed-looking history');
assert(component.includes('record.code_requests'), 'governed code requests are not restored into the conversation');
assert(component.includes('surface_truncation'), 'per-surface history loss is not visible to the conversation');
assert(component.includes("method: 'DELETE'"), 'durable conversation tombstoning is not reachable from the conversation');
assert(component.includes('audit-ledger rows remain'), 'archive UI misrepresents tombstoning as physical history deletion');
assert(component.includes("aria-live={historyHydrating ? 'off' : 'polite'}"), 'history hydration can flood the live-region announcement');
assert(component.includes('aria-haspopup="dialog"'), 'dialog interaction surfaces are exposed with menu semantics');
assert(component.includes('Approach constellation'), 'interaction faculties are expressed as a contextual constellation');
assert(component.includes('Effect airlock'), 'governed effects do not expose an intuitive authority boundary');
assert(component.includes('Worldlines'), 'durable conversations are not expressed as persistent contextual worlds');
assert(component.includes('Orbiting work'), 'background work is not expressed inside its worldline');
assert(component.includes('Made here'), 'conversation artifacts are not grouped with their worldline');
assert(component.includes('Continue from here'), 'message context does not offer a dimensional continuation gesture');
assert(component.includes("dispatch: 'Refract'"), 'mode dispatch remains visually generic rather than behaviorally expressive');
assert(component.includes('is a prompt frame:'), 'creative approaches overclaim distinct faculty routing');
assert(component.includes('codeApprovalBinding('), 'one-run approval is not bound to an exact request representation');
assert(component.includes('auth_generation: authGeneration'), 'one-run approval is not bound to the auth subject generation');
assert(component.includes('objective: objective.trim()'), 'one-run approval is not bound to the trimmed objective');
assert(component.includes('allowed_paths: [...allowedPaths]'), 'one-run approval is not bound to canonical allowed paths');
const approvalConsumption = component.indexOf('if (runCodeEffect) setCodeApproval(null);');
const effectDispatch = component.indexOf("authFetch('/api/admin/apocv4/code'");
assert(approvalConsumption >= 0 && approvalConsumption < effectDispatch, 'one-run approval is not consumed before effect dispatch');
assert(component.includes('isCurrentOperation(dispatchGeneration, dispatchConversationId)'), 'late async writes are not fenced to their auth worldline generation');
assert(component.includes('abortActiveOperations()'), 'in-flight operations are not aborted during worldline rebind');
assert(component.includes('reconciledCodeRequest'), 'uncertain effects are not reconciled against their original durable request');
assert(component.includes('no second effect was sent'), 'uncertain effects can imply an automatic reminted retry');
assert(component.includes('currentSessionRecorded'), 'directly restored worldlines cannot expose lifecycle actions');
assert(component.includes('refreshCurrentSnapshot'), 'opening conversation actions does not refresh the current world model');
assert(component.includes('SNAPSHOT_POLL_LIMIT'), 'active background work lacks a bounded settlement poll');
assert(component.includes('settledEffectCount: rollbackSessionDigest ? 2 : 1'), 'live compensation undercounts code effect plus rollback events');
assert(component.includes('clampAboveSurfaceAnchor('), 'Forge airlock placement is not clamped to the viewport');
assert(!component.includes('styles.toolDock'), 'prompt modes remain permanently docked in the composer');
assert(!component.includes('styles.truthRail'), 'privacy and response facts still occupy a permanent rail');

for (const token of [
  '.floatingSurface',
  '.intentPalette',
  '.messageContextMenu',
  '.recentThreads',
  '.threadChoice',
  '.worldObjects',
  '.turnStateReceipt',
  '.scopeSheet',
  '@media (pointer: coarse)',
  'max-height: calc(100dvh - 24px)',
  'height: calc(100dvh - 64px)',
]) {
  assert(style.includes(token), `missing responsive interaction styling: ${token}`);
}

console.log('public-apocrypha-interactions.test : OK · contextual, dimensional, keyboard and mobile interaction contracts passed');
