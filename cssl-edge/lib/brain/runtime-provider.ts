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
} from '../apocv4/runtime-proxy';

const OWNER_PRIVACY_PARTITION = 'owner:apocky';

export function ownerBrainRuntimeConfigured(): boolean {
  return process.env[APOCV4_BRAIN_RUNTIME_ENABLE_ENV] === '1';
}

function binding(userId: string) {
  return {
    sessionPrincipal: publicMemberPrincipalRef(userId),
    privacyPartition: OWNER_PRIVACY_PARTITION,
    credentialProfile: 'owner' as const,
  };
}

export async function probeOwnerBrainRuntime(traceparent?: string): Promise<RuntimeHealthProjection> {
  return fetchOwnerBrainRuntimeHealth(traceparent);
}

export async function sendOwnerBrainTurn(input: {
  readonly userId: string;
  readonly text: string;
  readonly sessionId: string;
  readonly requestId: string;
  readonly traceparent?: string;
}): Promise<RuntimeChatProjection> {
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
): Promise<OwnerBrainHistoryListProjection> {
  return listOwnerBrainRuntimeSessions({ ...binding(userId), limit: 24 }, traceparent);
}

export async function getOwnerBrainSession(
  userId: string,
  sessionId: string,
  traceparent?: string,
): Promise<OwnerBrainHistoryGetProjection> {
  return getOwnerBrainRuntimeSession({ ...binding(userId), sessionId }, traceparent);
}
