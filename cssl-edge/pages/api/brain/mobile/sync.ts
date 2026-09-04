import type { NextApiRequest, NextApiResponse } from 'next';

import { hasSameOrigin } from '@/lib/auth-session';
import type {
  RuntimeChatProjection,
  RuntimeSessionGetProjection,
} from '@/lib/apocv4/runtime-proxy';
import { RuntimeProxyError } from '@/lib/apocv4/runtime-proxy';
import {
  getOwnerBrainSession,
  ownerBrainRuntimeConfigured,
  sendOwnerBrainTurn,
} from '@/lib/brain/runtime-provider';
import {
  MINI_BRAIN_SYNC_RESPONSE_SCHEMA,
  type MiniBrainSyncResponse,
  type MiniBrainTombstone,
} from '@/lib/brain/mobile-contracts';
import {
  MiniBrainRelayError,
  verifyMiniBrainSyncRequest,
} from '@/lib/brain/mobile-relay';
import {
  requireBrainOwner,
  respondBrainOwnerFailure,
  setBrainPrivateHeaders,
} from '@/lib/brain/owner';
import { envelope } from '@/lib/response';

interface SyncDependencies {
  readonly configured: () => boolean;
  readonly getSession: typeof getOwnerBrainSession;
  readonly sendTurn: typeof sendOwnerBrainTurn;
}

const defaultDependencies: SyncDependencies = {
  configured: ownerBrainRuntimeConfigured,
  getSession: getOwnerBrainSession,
  sendTurn: sendOwnerBrainTurn,
};

function contentType(req: NextApiRequest): string | null {
  return (Array.isArray(req.headers['content-type']) ? req.headers['content-type'][0] : req.headers['content-type'])
    ?.split(';', 1)[0]
    ?.trim()
    .toLowerCase() ?? null;
}

function hasRequest(session: RuntimeSessionGetProjection, requestId: string): boolean {
  return session.session.messages.some(message => message.request_id === requestId);
}

function syncResponse(input: {
  readonly status: MiniBrainSyncResponse['status'];
  readonly sessionId: string;
  readonly requestId: string;
  readonly projection: RuntimeSessionGetProjection | null;
  readonly baseCursor: string | null;
  readonly tombstones?: readonly MiniBrainTombstone[];
}): MiniBrainSyncResponse {
  const receipt = input.projection?.observed.receipt;
  const cursor = input.projection?.session.tip_digest ?? null;
  const unchanged = cursor !== null && cursor === input.baseCursor;
  const env = envelope();
  return {
    schema_version: MINI_BRAIN_SYNC_RESPONSE_SCHEMA,
    status: input.status,
    session_id: input.sessionId,
    request_id: input.requestId,
    cursor,
    messages: unchanged ? [] : input.projection?.session.messages ?? [],
    tombstones: input.tombstones ?? [],
    events_truncated: input.projection?.session.events_truncated ?? false,
    provenance: {
      transport: 'owner_bound_apocv4_runtime',
      privacy_partition_ref: receipt?.privacy_partition_ref ?? null,
      principal_ref: receipt?.principal_ref ?? null,
      binding_ref: receipt?.binding_ref ?? null,
    },
    controls: {
      owner_session: 'verified',
      device_signature: 'verified',
      replay: 'bounded_sequence_and_idempotent_request',
      rate_limit: 'relay_instance_burst',
      partition: 'server_derived_owner',
    },
    served_by: env.served_by,
    ts: env.ts,
  };
}

async function observedSession(
  dependencies: SyncDependencies,
  userId: string,
  sessionId: string,
): Promise<RuntimeSessionGetProjection | null> {
  try {
    return await dependencies.getSession(userId, sessionId);
  } catch (error) {
    if (error instanceof RuntimeProxyError && error.code === 'session_not_found') return null;
    throw error;
  }
}

export function createMiniBrainSyncHandler(dependencies: SyncDependencies = defaultDependencies) {
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

    try {
      const verified = await verifyMiniBrainSyncRequest({ body: req.body, userId: owner.user.id });
      if (!dependencies.configured()) {
        res.status(503).json({
          error: 'Desktop Apocrypha is not connected. This signed turn remains safe in the encrypted device queue.',
          code: 'BRAIN_LOCAL_PROVIDER_DISABLED',
          retryable: true,
          ...envelope(),
        });
        return;
      }

      const { request } = verified;
      const before = await observedSession(dependencies, owner.user.id, request.session_id);
      if (request.operation === 'pull') {
        if (!before) {
          const tombstones: MiniBrainTombstone[] = request.base_cursor ? [{
            session_id: request.session_id,
            observed_at: new Date().toISOString(),
            reason: 'REMOTE_SESSION_ABSENT',
          }] : [];
          res.status(200).json(syncResponse({
            status: request.base_cursor ? 'tombstoned' : 'empty',
            sessionId: request.session_id,
            requestId: request.request_id,
            projection: null,
            baseCursor: request.base_cursor,
            tombstones,
          }));
          return;
        }
        res.status(200).json(syncResponse({
          status: before.session.tip_digest === request.base_cursor ? 'current' : 'advanced',
          sessionId: request.session_id,
          requestId: request.request_id,
          projection: before,
          baseCursor: request.base_cursor,
        }));
        return;
      }

      if (before && hasRequest(before, request.request_id)) {
        res.status(200).json(syncResponse({
          status: 'idempotent_replay',
          sessionId: request.session_id,
          requestId: request.request_id,
          projection: before,
          baseCursor: null,
        }));
        return;
      }
      if (
        (before && before.session.tip_digest !== request.base_cursor)
        || (!before && request.base_cursor !== null)
      ) {
        res.status(409).json({
          error: before
            ? 'Desktop history advanced before this queued turn. Pull and review the merged history before retrying.'
            : 'The cached desktop history is no longer present. Review the tombstone before retrying.',
          code: before ? 'BRAIN_SYNC_CONFLICT' : 'BRAIN_SYNC_REMOTE_ABSENT',
          current_cursor: before?.session.tip_digest ?? null,
          tombstones: before ? [] : [{
            session_id: request.session_id,
            observed_at: new Date().toISOString(),
            reason: 'REMOTE_SESSION_ABSENT',
          }],
          retryable: true,
          ...envelope(),
        });
        return;
      }

      await dependencies.sendTurn({
        userId: owner.user.id,
        text: request.payload!.text,
        sessionId: request.session_id,
        requestId: request.request_id,
      }) as RuntimeChatProjection;
      const after = await observedSession(dependencies, owner.user.id, request.session_id);
      if (!after || !hasRequest(after, request.request_id)) {
        throw new RuntimeProxyError('runtime_session_receipt_missing', 502);
      }
      res.status(200).json(syncResponse({
        status: 'appended',
        sessionId: request.session_id,
        requestId: request.request_id,
        projection: after,
        baseCursor: null,
      }));
    } catch (error) {
      if (error instanceof MiniBrainRelayError) {
        if (error.publicStatus === 429) res.setHeader('Retry-After', '60');
        res.status(error.publicStatus).json({
          error: 'The signed mobile sync request did not pass its security boundary.',
          code: error.code,
          ...envelope(),
        });
        return;
      }
      const code = error instanceof RuntimeProxyError ? error.code : 'runtime_unreachable';
      const status = error instanceof RuntimeProxyError && error.publicStatus === 504 ? 504 : 502;
      res.status(status).json({
        error: 'Desktop Apocrypha did not return a verified sync receipt. The queued turn was not represented as committed.',
        code: `BRAIN_${code.toUpperCase()}`,
        retryable: true,
        ...envelope(),
      });
    }
  };
}

export default createMiniBrainSyncHandler();
