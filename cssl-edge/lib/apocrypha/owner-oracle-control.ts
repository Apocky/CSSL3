import type { PostgrestError } from '@supabase/supabase-js';

import {
  getApocryphaServiceClient,
  type JobIdentity,
} from './job-control';

const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

export interface OwnerOracleRun {
  run_id: string;
  conversation_id: string;
  job_id: string;
  prompt: string;
}

export interface OwnerOracleCleanup {
  run_id: string;
  cleaned: true;
}

function uuid(value: unknown, code: string): string {
  if (typeof value !== 'string' || !UUID_V4.test(value)) throw new Error(code);
  return value.toLowerCase();
}

function row(value: unknown): Record<string, unknown> | null {
  const candidate = Array.isArray(value) ? value[0] : value;
  return candidate && typeof candidate === 'object' && !Array.isArray(candidate)
    ? candidate as Record<string, unknown>
    : null;
}

function rpcFailure(operation: 'SEED' | 'CLEANUP' | 'REGISTER', error: PostgrestError): never {
  const classification = `${error.code ?? ''}:${error.message ?? ''}`;
  if (classification.includes('ORACLE_RUN_NOT_FOUND')) throw new Error('OWNER_ORACLE_RUN_NOT_FOUND');
  if (classification.includes('ORACLE_CLEANUP_CONFLICT')) throw new Error('OWNER_ORACLE_CLEANUP_CONFLICT');
  throw new Error(`OWNER_ORACLE_${operation}_FAILED:${error.code ?? 'unknown'}`);
}

export function ownerOracleUuid(value: unknown, field: 'nonce' | 'run_id'): string {
  return uuid(value, `OWNER_ORACLE_${field.toUpperCase()}_INVALID`);
}

export async function seedOwnerChatOracle(
  identity: JobIdentity,
  nonce: string,
): Promise<OwnerOracleRun> {
  const client = getApocryphaServiceClient();
  const normalizedNonce = ownerOracleUuid(nonce, 'nonce');
  const { data, error } = await client.rpc('apocrypha_seed_owner_chat_oracle', {
    p_tenant_id: identity.tenantId,
    p_owner_principal_id: identity.principalId,
    p_nonce: normalizedNonce,
  });
  if (error) rpcFailure('SEED', error);
  const result = row(data);
  if (!result || typeof result.prompt !== 'string' || !result.prompt || result.prompt.length > 32_000) {
    throw new Error('OWNER_ORACLE_SEED_EMPTY');
  }
  return {
    run_id: uuid(result.run_id, 'OWNER_ORACLE_SEED_INVALID'),
    conversation_id: uuid(result.conversation_id, 'OWNER_ORACLE_SEED_INVALID'),
    job_id: uuid(result.job_id, 'OWNER_ORACLE_SEED_INVALID'),
    prompt: result.prompt,
  };
}

export async function cleanupOwnerChatOracle(
  identity: JobIdentity,
  runId: string,
): Promise<OwnerOracleCleanup> {
  const client = getApocryphaServiceClient();
  const normalizedRunId = ownerOracleUuid(runId, 'run_id');
  const { data, error } = await client.rpc('apocrypha_cleanup_owner_chat_oracle', {
    p_tenant_id: identity.tenantId,
    p_owner_principal_id: identity.principalId,
    p_run_id: normalizedRunId,
  });
  if (error) rpcFailure('CLEANUP', error);
  const result = row(data);
  if (!result || uuid(result.run_id, 'OWNER_ORACLE_CLEANUP_INVALID') !== normalizedRunId || result.cleaned !== true) {
    throw new Error('OWNER_ORACLE_CLEANUP_INVALID');
  }
  return { run_id: normalizedRunId, cleaned: true };
}

export async function registerOwnerChatOracleJob(
  identity: JobIdentity,
  runId: string,
  jobId: string,
): Promise<void> {
  const normalizedRunId = ownerOracleUuid(runId, 'run_id');
  const normalizedJobId = uuid(jobId, 'OWNER_ORACLE_JOB_ID_INVALID');
  const client = getApocryphaServiceClient();
  const { data, error } = await client.rpc('apocrypha_register_owner_chat_oracle_job', {
    p_tenant_id: identity.tenantId,
    p_owner_principal_id: identity.principalId,
    p_run_id: normalizedRunId,
    p_job_id: normalizedJobId,
  });
  if (error) rpcFailure('REGISTER', error);
  const result = row(data);
  if (!result
    || uuid(result.run_id, 'OWNER_ORACLE_REGISTER_INVALID') !== normalizedRunId
    || uuid(result.job_id, 'OWNER_ORACLE_REGISTER_INVALID') !== normalizedJobId
    || result.registered !== true) {
    throw new Error('OWNER_ORACLE_REGISTER_INVALID');
  }
}

export async function ownerChatConversationVisible(
  identity: JobIdentity,
  conversationId: string,
): Promise<boolean> {
  const normalizedConversationId = uuid(conversationId, 'OWNER_ORACLE_CONVERSATION_ID_INVALID');
  const client = getApocryphaServiceClient();
  const { data, error } = await client.rpc('apocrypha_owner_chat_conversation_visible', {
    p_tenant_id: identity.tenantId,
    p_owner_principal_id: identity.principalId,
    p_conversation_id: normalizedConversationId,
  });
  if (error) throw new Error(`OWNER_ORACLE_VISIBILITY_FAILED:${error.code ?? 'unknown'}`);
  if (typeof data !== 'boolean') throw new Error('OWNER_ORACLE_VISIBILITY_INVALID');
  return data;
}
