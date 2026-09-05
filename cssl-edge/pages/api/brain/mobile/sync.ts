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
  ownerBrainRuntimeRequestId,
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
  admitVerifiedMiniBrainRequest,
  getMiniBrainRelayStateStore,
  type MiniBrainRelayStateStore,
} from '@/lib/brain/mobile-relay-state';
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
  readonly relayState: () => MiniBrainRelayStateStore;
}

export const config = { maxDuration: 180 };

const defaultDependencies: SyncDependencies = {
  configured: ownerBrainRuntimeConfigured,
  getSession: getOwnerBrainSession,
  sendTurn: sendOwnerBrainTurn,
  relayState: getMiniBrainRelayStateStore,
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

interface TerminalTurnFailure {
  readonly errorClass: string;
  readonly errorDigest: string;
  readonly reissueSafe: boolean;
}

function terminalTurnFailure(
  session: RuntimeSessionGetProjection | null,
  requestId: string,
): TerminalTurnFailure | null {
  if (!session || hasRequest(session, requestId)) return null;
  const state = session.session.turn_states.find(candidate => (
    candidate.request_id === requestId && candidate.state === 'FAILED'
  ));
  if (
    !state
    || typeof state.error_class !== 'string'
    || typeof state.error_digest !== 'string'
    || !/^[0-9a-f]{64}$/u.test(state.error_digest)
  ) return null;
  return {
    errorClass: state.error_class,
    errorDigest: state.error_digest,
    reissueSafe: state.error_class === 'InterruptedChatAttempt',
  };
}

function turnStateProjectionTruncated(session: RuntimeSessionGetProjection | null): boolean {
  const surface = session?.session.surface_truncation.turn_states;
  return Boolean(
    surface
    && typeof surface === 'object'
    && !Array.isArray(surface)
    && (surface as Record<string, unknown>).truncated === true,
  );
}

function respondTerminalTurnFailure(
  res: NextApiResponse,
  input: {
    readonly requestId: string;
    readonly sessionId: string;
    readonly cursor: string;
    readonly failure: TerminalTurnFailure;
  },
): void {
  res.status(409).json({
    error: input.failure.reissueSafe
      ? 'The prior desktop attempt was interrupted before any answer was retained. The encrypted turn can be reissued under a fresh identity.'
      : 'The prior desktop attempt reached a terminal failure. Review the preserved turn before issuing a new request.',
    code: 'BRAIN_SYNC_TERMINAL_FAILED',
    request_id: input.requestId,
    session_id: input.sessionId,
    current_cursor: input.cursor,
    error_class: input.failure.errorClass,
    error_digest: input.failure.errorDigest,
    reissue_safe: input.failure.reissueSafe,
    retryable: false,
    ...envelope(),
  });
}

function respondUnresolvedTurnOutcome(
  res: NextApiResponse,
  input: { readonly requestId: string; readonly sessionId: string; readonly cursor: string },
): void {
  res.status(409).json({
    error: 'The desktop request failed, but its exact terminal state has aged outside the bounded projection. The encrypted turn remains intact for explicit review; it will not be retried or duplicated automatically.',
    code: 'BRAIN_SYNC_OUTCOME_UNRESOLVED',
    request_id: input.requestId,
    session_id: input.sessionId,
    current_cursor: input.cursor,
    error_class: 'RuntimeOutcomeOutsideProjection',
    reissue_safe: false,
    retryable: false,
    ...envelope(),
  });
}

function syncResponse(input: {
  readonly status: MiniBrainSyncResponse['status'];
  readonly sessionId: string;
  readonly requestId: string;
  readonly runtimeRequestId?: string;
  readonly projection: RuntimeSessionGetProjection | null;
  readonly baseCursor: string | null;
  readonly acknowledgedRequestIds?: readonly string[];
  readonly tombstones?: readonly MiniBrainTombstone[];
  readonly fallbackMessages?: readonly Record<string, unknown>[];
}): MiniBrainSyncResponse {
  const receipt = input.projection?.observed.receipt;
  const cursor = input.projection?.session.tip_digest ?? null;
  const unchanged = cursor !== null && cursor === input.baseCursor;
  const env = envelope();
  const projectedMessages = [
    ...(input.projection?.session.messages ?? []),
    ...(input.fallbackMessages ?? []),
  ];
  const messages = unchanged ? [] : projectedMessages.map(message => (
    input.runtimeRequestId && message.request_id === input.runtimeRequestId
      ? { ...message, request_id: input.requestId }
      : message
  )) ?? [];
  return {
    schema_version: MINI_BRAIN_SYNC_RESPONSE_SCHEMA,
    status: input.status,
    session_id: input.sessionId,
    request_id: input.requestId,
    acknowledged_request_ids: input.acknowledgedRequestIds ?? [],
    cursor,
    messages,
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
      rate_limit: 'owner_durable_window',
      partition: 'server_derived_owner',
    },
    served_by: env.served_by,
    ts: env.ts,
  };
}

function durableCommittedMessages(input: {
  readonly committed: RuntimeChatProjection;
  readonly runtimeRequestId: string;
  readonly text: string;
}): readonly Record<string, unknown>[] | null {
  const { committed } = input;
  if (
    committed.observed.runtime['schema_version'] !== 'apocv4.chat-response.v2'
    || committed.authority.conversation_history !== 'durable_principal_bound'
    || committed.identity === null
    || committed.context === null
  ) return null;
  const recordedAt = committed.observed.receipt.observed_at;
  const receipt = {
    context: {
      frame_digest: committed.context.frame_digest,
      provenance_spine_digest: committed.context.provenance_spine_digest,
    },
  };
  return [{
    role: 'user',
    content: input.text,
    request_id: input.runtimeRequestId,
    recorded_at: recordedAt,
    event_digest: null,
    receipt,
  }, {
    role: 'assistant',
    content: committed.model_reported.text,
    request_id: input.runtimeRequestId,
    recorded_at: recordedAt,
    event_digest: null,
    receipt,
  }];
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
      const signed = await verifyMiniBrainSyncRequest({ body: req.body, userId: owner.user.id });
      const verified = await admitVerifiedMiniBrainRequest(dependencies.relayState(), signed);
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
      const runtimeRequestId = request.operation === 'append'
        ? ownerBrainRuntimeRequestId(owner.user.id, request.request_id)
        : null;
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

      if (before && runtimeRequestId && hasRequest(before, runtimeRequestId)) {
        res.status(200).json(syncResponse({
          status: 'idempotent_replay',
          sessionId: request.session_id,
          requestId: request.request_id,
          runtimeRequestId,
          projection: before,
          baseCursor: null,
          acknowledgedRequestIds: [request.request_id],
        }));
        return;
      }
      const priorFailure = runtimeRequestId ? terminalTurnFailure(before, runtimeRequestId) : null;
      if (before && priorFailure) {
        respondTerminalTurnFailure(res, {
          requestId: request.request_id,
          sessionId: request.session_id,
          cursor: before.session.tip_digest,
          failure: priorFailure,
        });
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

      if (!runtimeRequestId) throw new RuntimeProxyError('runtime_request_identity_missing', 502);

      let committed: RuntimeChatProjection;
      try {
        committed = await dependencies.sendTurn({
          userId: owner.user.id,
          text: request.payload!.text,
          sessionId: request.session_id,
          requestId: request.request_id,
        }) as RuntimeChatProjection;
      } catch (sendError) {
        try {
          const failedSession = await observedSession(dependencies, owner.user.id, request.session_id);
          const failure = terminalTurnFailure(failedSession, runtimeRequestId);
          if (failedSession && failure) {
            respondTerminalTurnFailure(res, {
              requestId: request.request_id,
              sessionId: request.session_id,
              cursor: failedSession.session.tip_digest,
              failure,
            });
            return;
          }
          if (failedSession && turnStateProjectionTruncated(failedSession)) {
            respondUnresolvedTurnOutcome(res, {
              requestId: request.request_id,
              sessionId: request.session_id,
              cursor: failedSession.session.tip_digest,
            });
            return;
          }
        } catch {
          // Preserve the original runtime failure when its receipt cannot be read back.
        }
        throw sendError;
      }
      if (committed.observed.runtime['request_id'] !== runtimeRequestId) {
        throw new RuntimeProxyError('runtime_chat_receipt_mismatch', 502);
      }
      const after = await observedSession(dependencies, owner.user.id, request.session_id);
      if (!after) throw new RuntimeProxyError('runtime_chat_session_readback_missing', 502);
      const observedInSession = hasRequest(after, runtimeRequestId);
      const fallbackMessages = observedInSession ? [] : durableCommittedMessages({
        committed,
        runtimeRequestId,
        text: request.payload!.text,
      });
      if (!observedInSession && fallbackMessages === null) {
        throw new RuntimeProxyError('runtime_chat_session_receipt_missing', 502);
      }
      res.status(200).json(syncResponse({
        status: 'appended',
        sessionId: request.session_id,
        requestId: request.request_id,
        runtimeRequestId,
        projection: after,
        baseCursor: null,
        acknowledgedRequestIds: [request.request_id],
        fallbackMessages: fallbackMessages ?? [],
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
