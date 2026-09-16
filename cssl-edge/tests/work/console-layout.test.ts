// The Work console on a phone.
//
// This gate exists because of a photograph. The console is owner-gated and full-screen, so it
// cannot be rendered in the browser harness (importing it pulls CommonJS into the bundle and kills
// the whole fixture) and cannot be reached without an owner session on a real device. It therefore
// shipped for months drawing underneath the phone status bar -- the title colliding with the clock,
// the engine pill sitting on the wifi and battery icons -- with nobody able to see it but the owner.
//
// These are SOURCE assertions, which are weaker than a measurement. They are here so the specific
// regressions that were observed cannot come back silently; they are not a substitute for looking.

import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

const read = (rel: string) => fs.readFileSync(path.join(process.cwd(), rel), 'utf8');
const css = read('components/work/WorkConsole.module.css');
const room = read('styles/ApocryphaChat.module.css');

// -- it draws inside the screen, not under the hardware ------------------------------------------
// The room had five uses of this; the console had ZERO.
const insets = (css.match(/env\(safe-area-inset-/g) ?? []).length;
assert.ok(insets >= 3, `the console must respect safe areas on all edges, found ${insets} uses`);
assert.ok(/\.bar\s*\{[^}]*safe-area-inset-top/s.test(css), 'the top bar must clear the status bar');
assert.ok(/\.composer\s*\{[^}]*safe-area-inset-bottom/s.test(css), 'the composer must clear the home indicator');
assert.ok((room.match(/env\(safe-area-inset-/g) ?? []).length >= 3, 'the room keeps its own safe-area handling');

// -- a phone is not a narrow desktop -------------------------------------------------------------
// One breakpoint at 860px only turned the rail sideways. At 375px that spent the top of the window
// on two column headings, one of which read "—".
assert.ok(css.includes('@media (max-width: 560px)'), 'the console needs a phone layout, not only a tablet one');

// -- the hint line ran off the right edge, under a floating pill ---------------------------------
// components/AkashicConsent.tsx is position:fixed at bottom:0.75rem right:0.75rem, app-wide, and
// landed on top of the composer hint. The hint must wrap AND keep a lane clear of it.
assert.ok(/\.hint\s*\{[^}]*overflow-wrap:\s*anywhere/s.test(css), 'the hint must wrap rather than clip');
assert.ok(/\.hint\s*\{[^}]*padding-right/s.test(css), 'the hint must keep clear of the fixed consent pill');

// -- iOS must not zoom the console when the field is focused -------------------------------------
assert.ok(/font-size:\s*16px/.test(css), 'the composer field must be at least 16px or iOS zooms the page');

console.log('work/console-layout.test : OK - safe areas, phone breakpoint, wrapping hint, no iOS zoom');
