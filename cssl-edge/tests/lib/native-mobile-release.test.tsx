import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { parseNativeMobileRelease, PREPARING_MOBILE_RELEASE, type NativeMobileRelease } from '@/lib/mobile/release';
import { loadNativeMobileRelease } from '@/lib/mobile/release-server';
import ApocryphaDownload from '@/pages/download/apocrypha';

const payload = Buffer.from('APK_TEST_FIXTURE_NOT_AN_INSTALLABLE_PACKAGE');
const digest = createHash('sha256').update(payload).digest('hex');
const filename = 'Apocrypha-1.0.0-preview.apk';
const candidate = JSON.parse(readFileSync(resolve('public/releases/apocrypha-mobile/manifest.json'), 'utf8')) as unknown;
assert(parseNativeMobileRelease(candidate), 'the checked-in release must conform to the distribution contract');
const ready: NativeMobileRelease = {
  ...PREPARING_MOBILE_RELEASE,
  android: {
    state: 'ready',
    artifact: { href: `/downloads/${filename}`, sha256: digest, bytes: payload.length, signing_certificate_sha256: 'b'.repeat(64) },
    verification: { ...PREPARING_MOBILE_RELEASE.android.verification, package_signature: 'verified' },
  },
};
assert(parseNativeMobileRelease(ready), 'signed preview may retain explicit pending phone/account checks');
assert.equal(parseNativeMobileRelease({ ...ready, access: 'public' }), null);
assert.equal(parseNativeMobileRelease({ ...ready, secret: 'unexpected field' }), null);
for (const name of ['package_signature', 'emulator_launch', 'account_sign_in_and_chat', 'physical_device'] as const) {
  assert.equal(parseNativeMobileRelease({ ...ready, android: { ...ready.android, verification: { ...ready.android.verification, [name]: [ready.android.verification[name]] } } }), null, 'release verification enums must not coerce arrays');
}
assert.equal(parseNativeMobileRelease({ ...ready, android: { ...ready.android, verification: { ...ready.android.verification, package_signature: 'pending' } } }), null);
for (const href of ['https://evil.test/app.apk', '//evil.test/app.apk', '/downloads/../secret.apk', '/downloads/app.apk?token=secret', '/downloads/app.apk#hash', '/downloads/%2e%2e.apk', '/downloads/app.exe', '/downloads/sub/app.apk']) {
  assert.equal(parseNativeMobileRelease({ ...ready, android: { ...ready.android, artifact: { ...ready.android.artifact, href } } }), null);
}
for (const invalid of [{ bytes: 0 }, { bytes: -1 }, { bytes: 1.5 }, { bytes: 600 * 1024 * 1024 }, { sha256: 'bad' }, { signing_certificate_sha256: '' }]) {
  assert.equal(parseNativeMobileRelease({ ...ready, android: { ...ready.android, artifact: { ...ready.android.artifact, ...invalid } } }), null);
}
for (const [channel, url] of [
  ['testflight', 'https://testflight.apple.com/join/AbC123xy'],
  ['app-store', 'https://apps.apple.com/us/app/apocrypha/id123456789'],
] as const) {
  const value = { ...ready, ios: { state: 'ready', distribution: { channel, url } } };
  assert(parseNativeMobileRelease(value));
  assert(renderToStaticMarkup(<ApocryphaDownload release={value as NativeMobileRelease} />).includes(url));
}
for (const url of ['http://testflight.apple.com/join/AbC123xy', 'https://testflight.apple.com.evil.test/join/AbC123xy', 'https://testflight.apple.com@evil.test/join/AbC123xy', 'https://testflight.apple.com/join/AbC123xy?redirect=evil', 'https://testflight.apple.com:444/join/AbC123xy', 'https://apps.apple.com/us/app/apocrypha/id123456789', 'https://testflight.apple.com/join/short']) {
  assert.equal(parseNativeMobileRelease({ ...ready, ios: { state: 'ready', distribution: { channel: 'testflight', url } } }), null);
}
const candidateHtml = renderToStaticMarkup(<ApocryphaDownload release={PREPARING_MOBILE_RELEASE} />);
assert(!candidateHtml.includes('href="/downloads/'));
assert(!candidateHtml.includes('href="https://apps.apple.com'));
assert(!candidateHtml.includes('href="https://testflight.apple.com'));
assert(candidateHtml.includes('Sign in with your Apocky account.'));
assert(candidateHtml.includes('id="main-content"'));
const readyHtml = renderToStaticMarkup(<ApocryphaDownload release={ready} />);
assert(readyHtml.includes(`href="/downloads/${filename}"`));
assert(readyHtml.includes(`href="/downloads/${filename}.sha256"`));
assert(readyHtml.includes('Download Android preview'));
assert(readyHtml.includes('Account sign-in and chat'));
assert(readyHtml.includes('Pending'));
assert(!readyHtml.includes('production-ready'));

const root = mkdtempSync(join(tmpdir(), 'apocky-native-release-test-'));
try {
  const publicRoot = join(root, 'public');
  mkdirSync(join(publicRoot, 'releases', 'apocrypha-mobile'), { recursive: true });
  mkdirSync(join(publicRoot, 'downloads'), { recursive: true });
  const manifest = join(publicRoot, 'releases', 'apocrypha-mobile', 'manifest.json');
  const apk = join(publicRoot, 'downloads', filename);
  writeFileSync(manifest, JSON.stringify(ready));
  assert.equal(loadNativeMobileRelease(publicRoot).android.artifact, null, 'metadata cannot advertise a missing APK');
  writeFileSync(apk, payload);
  assert.equal(loadNativeMobileRelease(publicRoot).android.artifact, null, 'missing checksum sidecar withholds download');
  writeFileSync(`${apk}.sha256`, `${digest}  ${filename}\n`);
  assert.deepEqual(loadNativeMobileRelease(publicRoot), ready, 'matching actual bytes + sidecar release the preview link');
  writeFileSync(apk, Buffer.from('X'.repeat(payload.length)));
  assert.equal(loadNativeMobileRelease(publicRoot).android.artifact, null, 'same-size tampering fails digest gate');
  writeFileSync(apk, payload);
  writeFileSync(`${apk}.sha256`, `${digest}  wrong.apk\n`);
  assert.equal(loadNativeMobileRelease(publicRoot).android.artifact, null, 'wrong sidecar binding withholds download');
  writeFileSync(manifest, '{invalid');
  assert.deepEqual(loadNativeMobileRelease(publicRoot), PREPARING_MOBILE_RELEASE, 'corrupt manifest degrades without broken links');
} finally {
  assert.equal(dirname(resolve(root)), resolve(tmpdir()));
  assert(basename(root).startsWith('apocky-native-release-test-'));
  rmSync(root, { recursive: true, force: true });
}
console.log('native mobile release: strict links, preview claims, artifact bytes/checksum gates, candidate rendering passed');
