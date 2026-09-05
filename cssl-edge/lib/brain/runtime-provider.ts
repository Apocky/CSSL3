import {
  APOCV4_BRAIN_RUNTIME_ENABLE_ENV,
  fetchOwnerBrainRuntimeHealth,
  getOwnerBrainRuntimeSession,
  listOwnerBrainRuntimeSessions,
  publicMemberPrincipalRef,
  submitOwnerBrainRuntimeChat,
  type OwnerBrainHistoryGetProjection,
  type OwnerBrainHistoryListProjection,
  type RuntimeChatProjection,
  type RuntimeHealthProjection,
  RuntimeProxyError,
} from '../apocv4/runtime-proxy';

const OWNER_PRIVACY_PARTITION = 'owner:apocky';

function requireOwnerSubject(userId: string): void {
  if (process.env.APOCV4_RUNTIME_TRANSPORT === 'outbound-bridge'
    && userId !== process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID) {
    throw new RuntimeProxyError('runtime_principal_binding_invalid', 403);
  }
}

export function ownerBrainRuntimeConfigured(): boolean {
  return process.env[APOCV4_BRAIN_RUNTIME_ENABLE_ENV] === '1';
}

function binding(userId: string) {
  requireOwnerSubject(userId);
  return {
    sessionPrincipal: publicMemberPrincipalRef(userId),
    privacyPartition: OWNER_PRIVACY_PARTITION,
    credentialProfile: 'owner' as const,
  };
}

export async function probeOwnerBrainRuntime(userId: string, traceparent?: string): Promise<RuntimeHealthProjection> {
  requireOwnerSubject(userId);
  return fetchOwnerBrainRuntimeHealth(traceparent);
}

export async function sendOwnerBrainTurn(input: {
  readonly userId: string;
  readonly text: string;
  readonly sessionId: string;
  readonly requestId: string;
  readonly traceparent?: string;
}): Promise<RuntimeChatProjection> {
  requireOwnerSubject(input.userId);
  const principal = publicMemberPrincipalRef(input.userId);
  return submitOwnerBrainRuntimeChat({
    message: input.text,
    conversationId: input.sessionId,
    requestId: input.requestId,
    sessionPrincipal: principal,
    privacyPartition: OWNER_PRIVACY_PARTITION,
    credentialProfile: 'owner',
  }, input.traceparent);
}

export async function listOwnerBrainSessions(
  userId: string,
  traceparent?: string,
  page: { readonly cursor?: string | null; readonly limit?: number } = {},
): Promise<OwnerBrainHistoryListProjection> {
  return listOwnerBrainRuntimeSessions({ ...binding(userId), limit: page.limit ?? 24, cursor: page.cursor ?? null }, traceparent);
}

export async function getOwnerBrainSession(
  userId: string,
  sessionId: string,
  traceparent?: string,
): Promise<OwnerBrainHistoryGetProjection> {
  return getOwnerBrainRuntimeSession({ ...binding(userId), sessionId }, traceparent);
}
