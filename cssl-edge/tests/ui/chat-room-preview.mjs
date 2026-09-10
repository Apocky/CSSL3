// § Chat-room preview · actual UI+CSS ⊗ synthetic boundaries · localhost-only
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { readFile, realpath } from 'node:fs/promises';
import { build } from 'esbuild';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const fixture = path.join(root, 'tests/ui/chat-room-fixture.tsx');
const boundary = path.join(root, 'tests/ui/chat-room-boundaries.ts');
const host = '127.0.0.1';
const port = 3197;
const publicRoot = await realpath(path.join(root, 'public'));
const stubs = new Map([
  ['next/link', 'export { PreviewLink as default }'],
  ['next/router', 'export { usePreviewRouter as useRouter }'],
  ['@/components/hub/SiteSession', 'export { usePreviewSession as useSiteSession }'],
  ['@/lib/browser-auth', 'export { previewAuthFetch as authFetch }'],
  ['@/lib/auth', 'export { getPreviewAuthClient as getAuthClient }'],
  ['@/lib/brain/mini-brain', 'export { openPreviewMiniBrain as openMiniBrain, registerPreviewOfflineShell as registerMiniBrainOfflineShell }'],
]);
const plugin = {
  name: 'chat-preview-boundaries',
  setup(builder) {
    builder.onResolve({ filter: /^\// }, args => args.kind === 'url-token' ? { path: args.path, external: true } : undefined);
    builder.onResolve({ filter: /.*/ }, args => stubs.has(args.path) ? { path: args.path, namespace: 'chat-preview' } : undefined);
    builder.onLoad({ filter: /.*/, namespace: 'chat-preview' }, args => ({ contents: stubs.get(args.path) + ' from ' + JSON.stringify(boundary) + ';', loader: 'ts', resolveDir: root }));
  },
};
const result = await build({
  absWorkingDir: root, entryPoints: [fixture], outdir: path.join(root, '.preview-memory'), entryNames: 'chat-room',
  bundle: true, write: false, platform: 'browser', format: 'esm', target: 'es2022', jsx: 'automatic',
  sourcemap: 'inline', define: { 'process.env.NODE_ENV': '"development"' },
  loader: { '.woff': 'file', '.woff2': 'file', '.ttf': 'file', '.eot': 'file', '.svg': 'file' }, plugins: [plugin], logLevel: 'warning',
});
const files = new Map(result.outputFiles.map(file => ['/' + path.basename(file.path), file.contents]));
const html = '<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover"><title>Apocrypha chat — local UI fixture</title><link rel="stylesheet" href="/chat-room.css"><div id="root"></div><script type="module" src="/chat-room.js"></script></html>';
const mime = { '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.png': 'image/png', '.webp': 'image/webp', '.ico': 'image/x-icon', '.woff': 'font/woff', '.woff2': 'font/woff2', '.ttf': 'font/ttf', '.eot': 'application/vnd.ms-fontobject' };
const server = http.createServer(async (request, response) => {
  response.setHeader('Cache-Control', 'no-store');
  response.setHeader('X-Content-Type-Options', 'nosniff');
  response.setHeader('Content-Security-Policy', "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'; object-src 'none'; frame-ancestors 'none'");
  if (request.method !== 'GET') { response.writeHead(405).end(); return; }
  try {
    const pathname = decodeURIComponent(new URL(request.url || '/', 'http://' + host + ':' + port).pathname);
    if (pathname === '/' || pathname === '/apocrypha' || pathname === '/brain') { response.setHeader('Content-Type', 'text/html; charset=utf-8'); response.end(html); return; }
    const bundled = files.get(pathname);
    if (bundled) { response.setHeader('Content-Type', mime[path.extname(pathname)] || 'application/octet-stream'); response.end(bundled); return; }
    const ext = path.extname(pathname);
    if (['.svg', '.png', '.webp', '.ico', '.woff', '.woff2', '.ttf'].includes(ext)) {
      const target = await realpath(path.join(publicRoot, '.' + pathname));
      if (!target.startsWith(publicRoot + path.sep)) { response.writeHead(403).end(); return; }
      response.setHeader('Content-Type', mime[ext]); response.end(await readFile(target)); return;
    }
    response.writeHead(404).end('Fixture route only. No live service connection.');
  } catch { response.writeHead(404).end('Fixture asset not found.'); }
});
server.on('error', error => { console.error('Preview did not start:', error.code || error.message); process.exitCode = 1; });
server.listen(port, host, () => console.log('CHAT_ROOM_PREVIEW http://' + host + ':' + port + '/?mode=account&pending=0 — actual UI, synthetic boundaries; no production connection.'));
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => server.close());
