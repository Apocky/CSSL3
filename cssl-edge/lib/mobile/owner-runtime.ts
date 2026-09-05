import type { RequestUser } from '../admin-auth';
import { getAdminAllowlist } from '../admin-auth';
import { getOwnerBrainSession, listOwnerBrainSessions, ownerBrainRuntimeConfigured,
  probeOwnerBrainRuntime, sendOwnerBrainTurn } from '../brain/runtime-provider';
import { RuntimeProxyError } from '../apocv4/runtime-proxy';
import { AccountRuntimeError } from './account-runtime';

export function usesOwnerRuntime(user: RequestUser): boolean {
  return process.env.APOCV4_MOBILE_OWNER_BRIDGE === '1'
    && user.id === process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID
    && getAdminAllowlist().includes(user.email.toLowerCase());
}
export const ownerMobileRuntimeConfigured = ownerBrainRuntimeConfigured;

export async function callOwnerMobileRuntime(input: {
  user: RequestUser; surface: 'turn' | 'sessions' | 'status'; sessionId?: string;
  body?: Record<string, string>;
}): Promise<Record<string, unknown>> {
  if (!usesOwnerRuntime(input.user)) throw new AccountRuntimeError('ACCOUNT_RESPONSE_SCOPE_MISMATCH');
  try {
    if (input.surface === 'status') {
      await probeOwnerBrainRuntime(input.user.id);
      return { schema_version: 'apocky.mobile.status.v1', status: 'live' };
    }
    if (input.surface === 'turn') {
      const body = input.body!;
      const turn = await sendOwnerBrainTurn({ userId: input.user.id, text: body.text!, sessionId: body.session_id!, requestId: body.request_id! });
      return { schema_version: 'apocky.mobile.turn.v1', status: 'completed', text: turn.model_reported.text,
        session_id: body.session_id, request_id: body.request_id,
        model_id: turn.model_reported.model_id, response_digest: turn.model_reported.response_digest };
    }
    if (!input.sessionId) {
      const history = await listOwnerBrainSessions(input.user.id);
      return { schema_version: 'apocky.mobile.sessions.v1', status: 'live', sessions: history.sessions,
        count: history.count, discovery_scope: history.discovery_scope };
    }
    const history = await getOwnerBrainSession(input.user.id, input.sessionId);
    return { schema_version: 'apocky.mobile.session.v1', status: 'live', session: { ...history.session,
      schema_version: 'apocky.mobile.history-session.v1' } };
  } catch (error) {
    if (error instanceof AccountRuntimeError) throw error;
    if (error instanceof RuntimeProxyError) {
      if (error.publicStatus === 404) throw new AccountRuntimeError('ACCOUNT_SESSION_NOT_FOUND', 404);
      if (error.publicStatus === 504) throw new AccountRuntimeError('ACCOUNT_RESPONSE_TIMEOUT', 504);
    }
    throw new AccountRuntimeError('ACCOUNT_SERVICE_UNAVAILABLE');
  }
}
