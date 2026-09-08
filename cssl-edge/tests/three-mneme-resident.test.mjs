import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import {
  THREE_MNEME_CONTRACT,
  buildLease,
  buildScheduledTaskXml,
  validateLease,
  validateSealedManifest,
  validateSecretBundle,
} from '../scripts/apocrypha-memory-gateway/three-mneme-resident.mjs';

function sha(file) {
  return createHash('sha256').update(file).digest('hex').toUpperCase();
}

test('direct-Node resident preserves sealed scope, exact child pins, and lease schema', () => {
  const root = mkdtempSync(join(tmpdir(), 'three-mneme-resident-'));
  try {
    const entrypoint = join(root, 'broker', 'dist', 'index.js');
    const launcher = join(root, 'start-secure-local.ps1');
    const lock = join(root, 'package-lock.json');
    const manifestPath = join(root, 'task-manifest.v1.json');
    const nodePath = join(root, 'node.exe');
    for (const [file, content] of [
      [entrypoint, 'broker'], [launcher, 'legacy launcher'], [lock, 'lock'], [nodePath, 'node'],
    ]) {
      const parent = file.slice(0, file.lastIndexOf('\\'));
      if (parent) mkdirSync(parent, { recursive: true });
      writeFileSync(file, content);
    }
    const hashes = new Map([
      [entrypoint, sha('broker')], [launcher, sha('legacy launcher')], [lock, sha('lock')],
    ]);
    const manifest = {
      schema: 1,
      state_root: root,
      source_tree_sha256: 'A'.repeat(64),
      dist_tree_sha256: 'B'.repeat(64),
      package_lock_sha256: 'C'.repeat(64),
      rotation_scope: {
        agent_id: 'apocv4', subject: 'apocv4', allowed_projects: ['apocv4-owner'],
      },
      files: [
        { label: 'launcher.start', path: launcher, sha256: hashes.get(launcher) },
        { label: 'dist:dist/index.js', path: entrypoint, sha256: hashes.get(entrypoint) },
        { label: 'package-lock.json', path: lock, sha256: hashes.get(lock) },
      ],
    };
    validateSealedManifest(manifest, { stateRoot: root, hashFile: (file) => hashes.get(file) });
    assert.throws(
      () => validateSealedManifest({
        ...manifest,
        rotation_scope: { agent_id: 'apocv4', subject: 'apocv4', allowed_projects: ['other'] },
      }, { stateRoot: root, hashFile: (file) => hashes.get(file) }),
      { message: 'ROTATION_SCOPE_DRIFT' },
    );
    assert.throws(
      () => validateSealedManifest(manifest, { stateRoot: root, hashFile: () => 'D'.repeat(64) }),
      { message: 'PINNED_FILE_DRIFT' },
    );

    const lease = buildLease({
      manifest,
      manifestPath,
      entrypoint,
      pid: 42,
      processIdentity: { nodePath, processStartUtc: '2026-09-08T00:00:00.000Z' },
      instanceDigest: 'a'.repeat(64),
      nodePath,
    });
    assert.equal(lease.schema, 1);
    assert.equal(lease.pid, 42);
    assert.equal(lease.listener, '127.0.0.1');
    assert.equal(lease.port, 8787);
    assert.equal(lease.manifest_sha256, THREE_MNEME_CONTRACT.manifestSha256);
    validateLease(
      lease,
      { manifest, manifestPath, entrypoint },
      { address: '127.0.0.1', pid: 42 },
      (file) => hashes.get(file),
    );

    const task = buildScheduledTaskXml({
      nodePath: 'C:\\Program Files\\nodejs\\node.exe',
      scriptPath: 'C:\\Apocky\\three-mneme-resident.mjs',
      expectedScriptSha256: 'E'.repeat(64),
      sid: 'S-1-5-21-1-2-3-1001',
      workingDirectory: 'C:\\Apocky',
    });
    assert.match(task, /<Command>C:\\Program Files\\nodejs\\node\.exe<\/Command>/u);
    assert.match(task, /three-mneme-resident\.mjs&quot; supervise --self-sha256 E{64}/u);
    assert.match(task, /<WorkingDirectory>C:\\Apocky<\/WorkingDirectory>/u);
    assert.match(task, /<ExecutionTimeLimit>PT0S<\/ExecutionTimeLimit>/u);
    assert.match(task, /<RestartOnFailure><Interval>PT1M<\/Interval><Count>999<\/Count><\/RestartOnFailure>/u);
    assert.doesNotMatch(task, /powershell|pwsh|cmd\.exe/iu);

    const bundle = validateSecretBundle({
      version: 2,
      canonical: { mode: 'supabase-rpc', supabase_url: 'https://example.supabase.co', service_role_key: 's'.repeat(32) },
      auth: { jwt_secret: 'j'.repeat(32), owner_capability: 'o'.repeat(32), jwt_ttl_seconds: 900 },
    });
    assert.equal(bundle.auth.jwt_ttl_seconds, 900);
    assert.throws(() => validateSecretBundle({
      ...bundle,
      auth: { ...bundle.auth, owner_capability: bundle.auth.jwt_secret },
    }), { message: 'BROKER_BUNDLE_CREDENTIAL_COLLISION' });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
