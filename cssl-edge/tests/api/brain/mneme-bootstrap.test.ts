import assert from 'node:assert/strict';
import { createPublicKey } from 'node:crypto';
import type { NextApiRequest, NextApiResponse } from 'next';
import type { SupabaseClient } from '@supabase/supabase-js';

import {
  deriveMnemeOwnerPublicKey,
  mnemeOwnerPublicKeyMatches,
} from '@/lib/brain/mneme-bootstrap';
import type { Profile } from '@/lib/mneme/types';
import { createBrainMnemeBootstrapHandler } from '@/pages/api/brain/mneme/bootstrap';

interface Output {
  statusCode: number;
  body: Record<string, unknown>;
  headers: Record<string, string>;
}

function request(body: unknown, origin = 'http://localhost:3000'): { req: NextApiRequest; res: NextApiResponse; out: Output } {
  const out: Output = { statusCode: 200, body: {}, headers: {} };
  const req = {
    method: 'POST',
    headers: {
      host: 'localhost:3000',
      origin,
      'content-type': 'application/json',
      'x-apocky-test-admin-email': 'owner@example.com',
    },
    query: {},
    body,
  } as unknown as NextApiRequest;
  const res = {
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
    status(statusCode: number) { out.statusCode = statusCode; return this; },
    json(value: Record<string, unknown>) { out.body = value; return this; },
    end() { return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function profile(publicKey: Uint8Array): Profile {
  return {
    profile_id: 'member-test',
    sovereign_pk: publicKey,
    sigma_mask: new Uint8Array(19),
    created_at: '2026-09-03T00:00:00.000Z',
    memory_count: 0,
    message_count: 0,
    meta: {},
  };
}

const mutableEnv = process.env as Record<string, string | undefined>;
const previous = {
  bypass: process.env.LAZARUS_TEST_AUTH_BYPASS,
  admins: process.env.APOCKY_ADMIN_EMAILS,
  configuredKey: process.env.MNEME_SOVEREIGN_PUBKEY_HEX,
  profileBinding: process.env.MNEME_PROFILE_BINDING_SECRET,
  sessionBinding: process.env.APOCV4_SESSION_BINDING_SECRET,
};

async function main(): Promise<void> {
  try {
    mutableEnv.LAZARUS_TEST_AUTH_BYPASS = '1';
    mutableEnv.APOCKY_ADMIN_EMAILS = 'owner@example.com';
    delete mutableEnv.MNEME_SOVEREIGN_PUBKEY_HEX;
    mutableEnv.MNEME_PROFILE_BINDING_SECRET = 'mneme-owner-binding-test-secret-'.repeat(2);
    delete mutableEnv.APOCV4_SESSION_BINDING_SECRET;

    const first = deriveMnemeOwnerPublicKey({
      userId: 'verified-owner-id',
      bindingSecret: mutableEnv.MNEME_PROFILE_BINDING_SECRET,
    });
    const again = deriveMnemeOwnerPublicKey({
      userId: 'verified-owner-id',
      bindingSecret: mutableEnv.MNEME_PROFILE_BINDING_SECRET,
    });
    const other = deriveMnemeOwnerPublicKey({
      userId: 'different-owner-id',
      bindingSecret: mutableEnv.MNEME_PROFILE_BINDING_SECRET,
    });
    assert.equal(first.source, 'server_derived_owner_binding');
    assert.equal(first.publicKey.length, 32);
    assert(mnemeOwnerPublicKeyMatches(first.publicKey, again.publicKey), 'same verified identity is stable');
    assert(!mnemeOwnerPublicKeyMatches(first.publicKey, other.publicKey), 'different verified identities remain isolated');
    const spki = Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), Buffer.from(first.publicKey)]);
    assert.equal(createPublicKey({ key: spki, format: 'der', type: 'spki' }).asymmetricKeyType, 'ed25519');

    const configured = deriveMnemeOwnerPublicKey({
      userId: 'verified-owner-id',
      configuredPublicKeyHex: 'ab'.repeat(32),
      bindingSecret: 'ignored-because-an-explicit-key-is-authoritative',
    });
    assert.equal(configured.source, 'configured_owner_key');
    assert.equal(Buffer.from(configured.publicKey).toString('hex'), 'ab'.repeat(32));

    const wrongOrigin = request({ confirmation: 'CREATE_OWNER_PRIVATE_MNEME_PROFILE' }, 'https://attacker.invalid');
    await createBrainMnemeBootstrapHandler({
      getClient: () => { throw new Error('must not touch storage'); },
      getProfile: async () => { throw new Error('must not read'); },
      ensureProfile: async () => { throw new Error('must not write'); },
    })(wrongOrigin.req, wrongOrigin.res);
    assert.equal(wrongOrigin.out.statusCode, 403);
    assert.equal(wrongOrigin.out.body.code, 'BRAIN_ORIGIN_DENIED');

    const missingConfirmation = request({ confirmation: 'yes' });
    await createBrainMnemeBootstrapHandler({
      getClient: () => { throw new Error('must not touch storage'); },
      getProfile: async () => { throw new Error('must not read'); },
      ensureProfile: async () => { throw new Error('must not write'); },
    })(missingConfirmation.req, missingConfirmation.res);
    assert.equal(missingConfirmation.out.statusCode, 400);
    assert.equal(missingConfirmation.out.body.code, 'BRAIN_MNEME_CONFIRMATION_REQUIRED');

    const disconnected = request({ confirmation: 'CREATE_OWNER_PRIVATE_MNEME_PROFILE' });
    await createBrainMnemeBootstrapHandler({
      getClient: () => null,
      getProfile: async () => { throw new Error('must not read'); },
      ensureProfile: async () => { throw new Error('must not write'); },
    })(disconnected.req, disconnected.res);
    assert.equal(disconnected.out.statusCode, 503);
    assert.equal(disconnected.out.body.code, 'BRAIN_MNEME_STORAGE_UNAVAILABLE');

    const client = {} as SupabaseClient;
    let capturedProfileId = '';
    let capturedKey = new Uint8Array();
    const created = request({ confirmation: 'CREATE_OWNER_PRIVATE_MNEME_PROFILE' });
    await createBrainMnemeBootstrapHandler({
      getClient: () => client,
      getProfile: async () => null,
      ensureProfile: async (_client, profileId, publicKey) => {
        capturedProfileId = profileId;
        capturedKey = publicKey;
        return profile(publicKey);
      },
    })(created.req, created.res);
    assert.equal(created.out.statusCode, 201);
    assert.equal(created.out.body.status, 'created');
    assert.equal(created.out.body.key_source, 'server_derived_owner_binding');
    assert.match(capturedProfileId, /^member-[0-9a-f]{40}$/u);
    assert.equal(capturedKey.length, 32);
    assert(!Object.hasOwn(created.out.body, 'profile_id'), 'opaque profile id never crosses the browser boundary');
    assert.match(created.out.headers['cache-control'] ?? '', /private.*no-store/);

    const existing = request({ confirmation: 'CREATE_OWNER_PRIVATE_MNEME_PROFILE' });
    let writes = 0;
    await createBrainMnemeBootstrapHandler({
      getClient: () => client,
      getProfile: async () => profile(capturedKey),
      ensureProfile: async () => { writes += 1; return profile(capturedKey); },
    })(existing.req, existing.res);
    assert.equal(existing.out.statusCode, 200);
    assert.equal(existing.out.body.status, 'already_provisioned');
    assert.equal(writes, 0, 'idempotent retry never rewrites an existing profile');

    const mismatch = request({ confirmation: 'CREATE_OWNER_PRIVATE_MNEME_PROFILE' });
    await createBrainMnemeBootstrapHandler({
      getClient: () => client,
      getProfile: async () => profile(new Uint8Array(32).fill(7)),
      ensureProfile: async () => { throw new Error('must not rewrite mismatched profile'); },
    })(mismatch.req, mismatch.res);
    assert.equal(mismatch.out.statusCode, 409);
    assert.equal(mismatch.out.body.code, 'BRAIN_MNEME_PROFILE_BINDING_MISMATCH');

    console.log('mneme-bootstrap.test : OK · explicit owner confirmation + real Ed25519 binding + idempotent profile creation');
  } finally {
    for (const [key, value] of Object.entries({
      LAZARUS_TEST_AUTH_BYPASS: previous.bypass,
      APOCKY_ADMIN_EMAILS: previous.admins,
      MNEME_SOVEREIGN_PUBKEY_HEX: previous.configuredKey,
      MNEME_PROFILE_BINDING_SECRET: previous.profileBinding,
      APOCV4_SESSION_BINDING_SECRET: previous.sessionBinding,
    })) {
      if (value === undefined) delete mutableEnv[key];
      else mutableEnv[key] = value;
    }
  }
}

void main().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
