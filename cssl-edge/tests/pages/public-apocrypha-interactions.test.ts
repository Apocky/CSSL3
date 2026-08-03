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
assert(!component.includes('styles.toolDock'), 'prompt modes remain permanently docked in the composer');
assert(!component.includes('styles.truthRail'), 'privacy and response facts still occupy a permanent rail');

for (const token of [
  '.floatingSurface',
  '.intentPalette',
  '.messageContextMenu',
  '.scopeSheet',
  '@media (pointer: coarse)',
  'max-height: calc(100dvh - 24px)',
  'height: calc(100dvh - 64px)',
]) {
  assert(style.includes(token), `missing responsive interaction styling: ${token}`);
}

console.log('public-apocrypha-interactions.test : OK · contextual, dimensional, keyboard and mobile interaction contracts passed');
