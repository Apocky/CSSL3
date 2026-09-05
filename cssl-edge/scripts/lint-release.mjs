import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const require = createRequire(import.meta.url);
const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const tsc = require.resolve('typescript/bin/tsc');
const typecheck = spawnSync(process.execPath, [tsc, '--noEmit', '--pretty', 'false'], {
  cwd: root,
  stdio: 'inherit',
});
if (typecheck.status !== 0) process.exit(typecheck.status ?? 1);

const releaseFiles = [
  'components/apocrypha/ChatThread.tsx',
  'components/AdminLayout.tsx',
  'lib/apocrypha/proxy.ts',
  'lib/apocrypha/retired-route.ts',
  'pages/api/admin/check.ts',
  'pages/api/admin/apocrypha/chat.ts',
  'pages/api/admin/apocrypha/telemetry.ts',
  'lib/apocrypha/vision.ts',
  'pages/api/admin/apocrypha/vision/session.ts',
  'pages/api/admin/apocrypha/vision/session/[session_ref].ts',
  'pages/api/admin/apocrypha/vision/session/[session_ref]/frame.ts',
  'pages/api/admin/apocrypha/vision/session/[session_ref]/control.ts',
  'components/apocrypha/VisionPanel.tsx',
];
for (const relative of releaseFiles) {
  const content = readFileSync(resolve(root, relative), 'utf8');
  if (content.includes('/api/v1') || content.includes('text/event-stream')) {
    console.error(`release lint failed: predecessor or synthetic stream token in ${relative}`);
    process.exit(1);
  }
}
console.log('release lint: typecheck and V2 boundary checks passed');
