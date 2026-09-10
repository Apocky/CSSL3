import type { NextApiRequest, NextApiResponse } from 'next';
import type { SupabaseClient } from '@supabase/supabase-js';

import { hasSameOrigin } from '@/lib/auth-session';
import {
  deriveMnemeOwnerPublicKey,
  mnemeOwnerPublicKeyMatches,
  MnemeBootstrapError,
} from '@/lib/brain/mneme-bootstrap';
import {
  requireBrainOwner,
  respondBrainOwnerFailure,
  setBrainPrivateHeaders,
} from '@/lib/brain/owner';
import { deriveMemberProfileId } from '@/lib/mneme/member-profile';
import { ensureProfile, getMnemeClient, getProfile } from '@/lib/mneme/store';
import type { Profile } from '@/lib/mneme/types';
import { envelope } from '@/lib/response';

const CONFIRMATION = 'CREATE_OWNER_PRIVATE_MNEME_PROFILE';

interface BootstrapDependencies {
  readonly getClient: () => SupabaseClient | null;
  readonly getProfile: (client: SupabaseClient, profileId: string) => Promise<Profile | null>;
  readonly ensureProfile: (client: SupabaseClient, profileId: string, publicKey: Uint8Array) => Promise<Profile>;
}

const defaultDependencies: BootstrapDependencies = {
  getClient: getMnemeClient,
  getProfile,
  ensureProfile,
};

function exactConfirmation(value: unknown): value is { confirmation: typeof CONFIRMATION } {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const row = value as Record<string, unknown>;
  return Object.keys(row).length === 1 && row.confirmation === CONFIRMATION;
}

function contentType(req: NextApiRequest): string | null {
  return (Array.isArray(req.headers['content-type']) ? req.headers['content-type'][0] : req.headers['content-type'])
    ?.split(';', 1)[0]
    ?.trim()
    .toLowerCase() ?? null;
}

export function createBrainMnemeBootstrapHandler(dependencies: BootstrapDependencies = defaultDependencies) {
  return async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
    setBrainPrivateHeaders(res);
    res.setHeader('Allow', 'POST');
    if (req.method !== 'POST') {
      res.status(405).json({ error: 'Method not allowed', code: 'BRAIN_METHOD_NOT_ALLOWED', ...envelope() });
      return;
    }
    if (!hasSameOrigin(req)) {
      res.status(403).json({ error: 'Same-origin request required', code: 'BRAIN_ORIGIN_DENIED', ...envelope() });
      return;
    }
    if (contentType(req) !== 'application/json') {
      res.status(415).json({ error: 'Content-Type must be application/json', code: 'BRAIN_CONTENT_TYPE_REQUIRED', ...envelope() });
      return;
    }
    const owner = await requireBrainOwner(req);
    if (!owner.ok) {
      respondBrainOwnerFailure(res, owner);
      return;
    }
    if (!exactConfirmation(req.body)) {
      res.status(400).json({
        error: 'Explicit owner confirmation is required. No memory profile was created.',
        code: 'BRAIN_MNEME_CONFIRMATION_REQUIRED',
        ...envelope(),
      });
      return;
    }
    const client = dependencies.getClient();
    if (!client) {
      res.status(503).json({
        error: 'Private Mneme storage is not connected. No profile was created.',
        code: 'BRAIN_MNEME_STORAGE_UNAVAILABLE',
        ...envelope(),
      });
      return;
    }

    try {
      const profileId = deriveMemberProfileId(owner.user.id);
      const binding = deriveMnemeOwnerPublicKey({
        userId: owner.user.id,
        configuredPublicKeyHex: process.env.MNEME_SOVEREIGN_PUBKEY_HEX,
        bindingSecret: process.env.MNEME_PROFILE_BINDING_SECRET ?? process.env.APOCV4_SESSION_BINDING_SECRET,
      });
      const existing = await dependencies.getProfile(client, profileId);
      if (existing) {
        if (!mnemeOwnerPublicKeyMatches(existing.sovereign_pk, binding.publicKey)) {
          throw new MnemeBootstrapError('BRAIN_MNEME_PROFILE_BINDING_MISMATCH', 409);
        }
        res.status(200).json({
          schema_version: 'apocky.owner-brain.mneme-bootstrap.v1',
          status: 'already_provisioned',
          key_source: binding.source,
          controls: { owner_session: 'verified', profile_namespace: 'server_derived', confirmation: 'explicit' },
          ...envelope(),
        });
        return;
      }

      let created: Profile;
      let status: 'created' | 'already_provisioned' = 'created';
      try {
        created = await dependencies.ensureProfile(client, profileId, binding.publicKey);
      } catch {
        const raced = await dependencies.getProfile(client, profileId);
        if (!raced) throw new Error('BRAIN_MNEME_PROFILE_CREATE_FAILED');
        created = raced;
        status = 'already_provisioned';
      }
      if (!mnemeOwnerPublicKeyMatches(created.sovereign_pk, binding.publicKey)) {
        throw new MnemeBootstrapError('BRAIN_MNEME_PROFILE_BINDING_MISMATCH', 409);
      }
      res.status(status === 'created' ? 201 : 200).json({
        schema_version: 'apocky.owner-brain.mneme-bootstrap.v1',
        status,
        key_source: binding.source,
        controls: { owner_session: 'verified', profile_namespace: 'server_derived', confirmation: 'explicit' },
        ...envelope(),
      });
    } catch (error) {
      const code = error instanceof MnemeBootstrapError ? error.code : 'BRAIN_MNEME_PROFILE_CREATE_FAILED';
      const status = error instanceof MnemeBootstrapError ? error.publicStatus : 503;
      res.status(status).json({
        error: status === 409
          ? 'The existing private profile does not match this owner binding. Nothing was changed.'
          : 'The private profile could not be created and no success was recorded.',
        code,
        ...envelope(),
      });
    }
  };
}

export default createBrainMnemeBootstrapHandler();
