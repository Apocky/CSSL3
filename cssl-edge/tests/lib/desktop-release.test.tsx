import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import {
  parseDesktopRelease,
  pendingChecks,
  PREPARING_DESKTOP_RELEASE,
  type DesktopRelease,
} from '@/lib/desktop/release';
import { loadDesktopRelease } from '@/lib/desktop/release-server';
import { PREPARING_MOBILE_RELEASE } from '@/lib/mobile/release';
import ApocryphaDownload from '@/pages/download/apocrypha';

const payload = Buffer.from('NSIS_TEST_FIXTURE_NOT_AN_INSTALLABLE_PACKAGE');
const digest = createHash('sha256').update(payload).digest('hex');
const filename = 'Apocrypha-Desktop-0.1.0-windows-x64.exe';

const candidate = JSON.parse(
  readFileSync(resolve('public/releases/apocrypha-desktop/manifest.json'), 'utf8'),
) as unknown;
assert(parseDesktopRelease(candidate), 'the checked-in desktop release must conform to the distribution contract');

const ready: DesktopRelease = {
  ...PREPARING_DESKTOP_RELEASE,
  windows: {
    state: 'ready',
    artifact: { href: `/downloads/${filename}`, sha256: digest, bytes: payload.length, format: 'nsis-installer' },
    signing: 'unsigned',
    verification: {
      launch: 'passed',
      service_configuration: 'passed',
      account_sign_in_and_chat: 'pending',
      installer_install_and_uninstall: 'pending',
    },
  },
};
assert(parseDesktopRelease(ready), 'an unsigned preview may retain explicitly pending checks');

// ── envelope ──────────────────────────────────────────────────────────────
assert.equal(parseDesktopRelease({ ...ready, schema_version: 'apocky.desktop-release.v2' }), null);
assert.equal(parseDesktopRelease({ ...ready, access: 'public' }), null);
assert.equal(parseDesktopRelease({ ...ready, channel: 'stable' }), null, 'this contract carries preview builds only');
assert.equal(parseDesktopRelease({ ...ready, secret: 'unexpected field' }), null);
assert.equal(parseDesktopRelease(null), null);
assert.equal(parseDesktopRelease([ready]), null);
for (const version of ['', '1', '1.0', 'v1.0.0', '1.0.0-alpha', '00000.0.0']) {
  assert.equal(parseDesktopRelease({ ...ready, version }), null, `${version} must be refused`);
}

// ── the artifact reference may not escape /downloads ──────────────────────
for (const href of [
  'https://evil.test/setup.exe',
  '//evil.test/setup.exe',
  '/downloads/../secret.exe',
  '/downloads/sub/setup.exe',
  '/downloads/setup.exe?token=secret',
  '/downloads/setup.exe#hash',
  '/downloads/%2e%2e.exe',
  '/downloads/setup.apk',
  '/downloads/.hidden.exe',
  '/releases/setup.exe',
]) {
  const broken = { ...ready, windows: { ...ready.windows, artifact: { ...ready.windows.artifact, href } } };
  assert.equal(parseDesktopRelease(broken), null, `${href} must be refused`);
}
for (const invalid of [
  { bytes: 0 },
  { bytes: -1 },
  { bytes: 1.5 },
  { bytes: 400 * 1024 * 1024 },
  { sha256: 'bad' },
  { sha256: digest.toUpperCase() },
  { format: 'zip' },
]) {
  const broken = { ...ready, windows: { ...ready.windows, artifact: { ...ready.windows.artifact, ...invalid } } };
  assert.equal(parseDesktopRelease(broken), null, `${JSON.stringify(invalid)} must be refused`);
}

// ── state and verification honesty ────────────────────────────────────────
assert.equal(
  parseDesktopRelease({ ...ready, windows: { ...ready.windows, state: 'preparing' } }),
  null,
  'a preparing state may not carry an artifact',
);
assert.equal(
  parseDesktopRelease({ ...ready, windows: { ...ready.windows, state: 'ready', artifact: null } }),
  null,
  'a ready state must name an artifact',
);
for (const gate of ['launch', 'service_configuration'] as const) {
  const broken = {
    ...ready,
    windows: { ...ready.windows, verification: { ...ready.windows.verification, [gate]: 'pending' } },
  };
  assert.equal(parseDesktopRelease(broken), null, `${gate} must pass before a build is downloadable`);
}
for (const name of ['launch', 'service_configuration', 'account_sign_in_and_chat', 'installer_install_and_uninstall'] as const) {
  const broken = {
    ...ready,
    windows: { ...ready.windows, verification: { ...ready.windows.verification, [name]: [ready.windows.verification[name]] } },
  };
  assert.equal(parseDesktopRelease(broken), null, 'verification enums must not coerce arrays');
}
assert.equal(parseDesktopRelease({ ...ready, windows: { ...ready.windows, signing: 'self-signed' } }), null);
assert(parseDesktopRelease({ ...ready, windows: { ...ready.windows, signing: 'authenticode' } }));

assert.deepEqual(pendingChecks(ready), ['account sign-in and chat', 'install and uninstall on a clean computer']);
assert.equal(pendingChecks(PREPARING_DESKTOP_RELEASE).length, 4);

// ── the page ──────────────────────────────────────────────────────────────
const preparingHtml = renderToStaticMarkup(
  <ApocryphaDownload release={PREPARING_MOBILE_RELEASE} desktop={PREPARING_DESKTOP_RELEASE} />,
);
assert(!preparingHtml.includes('href="/downloads/'), 'no download may be offered while every platform is preparing');
assert(preparingHtml.includes('No Windows download is available yet.'));
assert(preparingHtml.includes('On your computer'));

const readyHtml = renderToStaticMarkup(<ApocryphaDownload release={PREPARING_MOBILE_RELEASE} desktop={ready} />);
assert(readyHtml.includes(`href="/downloads/${filename}"`));
assert(readyHtml.includes(`href="/downloads/${filename}.sha256"`));
assert(readyHtml.includes('Download for Windows'));
assert(readyHtml.includes(digest), 'the checksum must be readable on the page itself');
assert(readyHtml.includes('not signed yet'), 'an unsigned installer must say so before a person meets the warning');
assert(readyHtml.includes('Windows protected'), 'the page must name the screen Windows will actually show');
assert(readyHtml.includes('account sign-in and chat'), 'pending checks stay visible on a preview');
assert(!readyHtml.includes('production-ready'));

const signedHtml = renderToStaticMarkup(
  <ApocryphaDownload
    release={PREPARING_MOBILE_RELEASE}
    desktop={{ ...ready, windows: { ...ready.windows, signing: 'authenticode' } }}
  />,
);
assert(!signedHtml.includes('not signed yet'), 'the warning must disappear once the installer is signed');

// ── the loader re-checks what the manifest claims ─────────────────────────
const root = mkdtempSync(join(tmpdir(), 'apocky-desktop-release-test-'));
try {
  const publicRoot = join(root, 'public');
  mkdirSync(join(publicRoot, 'releases', 'apocrypha-desktop'), { recursive: true });
  mkdirSync(join(publicRoot, 'downloads'), { recursive: true });
  const manifest = join(publicRoot, 'releases', 'apocrypha-desktop', 'manifest.json');
  const installer = join(publicRoot, 'downloads', filename);
  const sidecar = `${installer}.sha256`;

  writeFileSync(manifest, JSON.stringify(ready));
  writeFileSync(installer, payload);
  writeFileSync(sidecar, `${digest}  ${filename}\n`);
  assert.equal(loadDesktopRelease(publicRoot).windows.state, 'ready');
  assert.equal(loadDesktopRelease(publicRoot).windows.artifact?.sha256, digest);

  writeFileSync(installer, Buffer.from('DIFFERENT_BYTES_ENTIRELY_XXXXXXXXXXXXXXXXX'));
  assert.equal(loadDesktopRelease(publicRoot).windows.state, 'preparing', 'a changed file must not keep its old hash');

  writeFileSync(installer, payload);
  writeFileSync(sidecar, `${digest}  wrong-name.exe\n`);
  assert.equal(loadDesktopRelease(publicRoot).windows.state, 'preparing', 'the sidecar must name the staged file');

  writeFileSync(sidecar, `${digest}  ${filename}\n`);
  writeFileSync(manifest, JSON.stringify({ ...ready, windows: { ...ready.windows, artifact: { ...ready.windows.artifact, bytes: payload.length + 1 } } }));
  assert.equal(loadDesktopRelease(publicRoot).windows.state, 'preparing', 'a byte-count mismatch must not publish');

  writeFileSync(manifest, '{ not json');
  assert.equal(loadDesktopRelease(publicRoot).windows.state, 'preparing');

  rmSync(manifest);
  assert.deepEqual(loadDesktopRelease(publicRoot), PREPARING_DESKTOP_RELEASE, 'a missing manifest offers nothing');
} finally {
  rmSync(root, { recursive: true, force: true });
}

console.log('desktop-release: ok');
