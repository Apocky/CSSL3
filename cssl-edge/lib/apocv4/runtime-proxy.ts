// Server-only transport for the authenticated Apocv4 runtime.

import { createHmac } from 'node:crypto';
import { request as httpsRequest } from 'node:https';
import { isIP } from 'node:net';

import {
  isRuntimeSessionPrincipal,
  publicMemberPrincipalRef,
  type RuntimeSessionPrincipal,
} from './session-principal';

export { publicMemberPrincipalRef } from './session-principal';
export type { RuntimeSessionPrincipal } from './session-principal';

export const APOCV4_PROXY_SCHEMA = 'apocky.apocv4-runtime-proxy.v1';

const RUNTIME_SCHEMA = 'apocv4.runtime-service.v1';
const TOKEN_RE = /^[A-Za-z0-9._~+/-]{1,8192}$/;
const SHA256_RE = /^[0-9a-f]{64}$/;
const TRACEPARENT_RE = /^00-([0-9a-f]{32})-([0-9a-f]{16})-01$/;
const PORT_RE = /^(?:[1-9][0-9]{0,3}|[1-5][0-9]{4}|6[0-4][0-9]{3}|65[0-4][0-9]{2}|655[0-2][0-9]|6553[0-5])$/;
const BASE64_RE = /^[A-Za-z0-9+/]+={0,2}$/;
const HEALTH_RESPONSE_LIMIT = 256 * 1024;
const CHAT_RESPONSE_LIMIT = 512 * 1024;
const SESSION_RESPONSE_LIMIT = 2 * 1024 * 1024;
const SESSION_PROJECTION_LIMIT = 512 * 1024;
const SESSION_MESSAGE_LIMIT = 64;
const SESSION_MESSAGE_BYTES = 256 * 1024;
const SESSION_TURN_STATE_LIMIT = 64;
const SESSION_TURN_STATE_BYTES = 32 * 1024;
const SESSION_JOB_LIMIT = 64;
const SESSION_JOB_BYTES = 32 * 1024;
const SESSION_ARTIFACT_LIMIT = 32;
const SESSION_ARTIFACT_BYTES = 96 * 1024;
const SESSION_PROPOSAL_LIMIT = 64;
const SESSION_PROPOSAL_BYTES = 48 * 1024;
const SESSION_CODE_REQUEST_LIMIT = 64;
const SESSION_CODE_REQUEST_BYTES = 96 * 1024;
const SESSION_EFFECT_LIMIT = 32;
const SESSION_EFFECT_BYTES = 48 * 1024;
const SESSION_UPSTREAM_ARRAY_LIMIT = 4096;
const OBJECTIVE_RESPONSE_LIMIT = 2 * 1024 * 1024;
const CODE_RESPONSE_LIMIT = 3 * 1024 * 1024;
const ROLLBACK_RESPONSE_LIMIT = 512 * 1024;
const HEALTH_DEADLINE_MS = 12_000;
const CHAT_DEADLINE_MS = 80_000;
const SESSION_DEADLINE_MS = 30_000;
const JOB_DEADLINE_MS = 30_000;
const JOB_RESPONSE_LIMIT = 512 * 1024;
const VISION_RESPONSE_LIMIT = 512 * 1024;
export const RUNPOD_SYNC_DEADLINE_MS = 95_000;
export const RUNPOD_CODE_DEADLINE_MS = 240_000;
const ROLLBACK_DEADLINE_MS = 45_000;
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[45][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const CLIENT_SESSION_UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const JOB_ID_RE = /^job:[0-9a-f]{64}$/;
const SESSION_BINDING_SCHEMA = 'apocv4.session-binding.v1';
const CODE_PATH_RE = /^[A-Za-z0-9_.@+ -]+(?:\/[A-Za-z0-9_.@+ -]+)*$/;
const WINDOWS_RESERVED_PATH_STEMS = new Set([
  'CON', 'PRN', 'AUX', 'NUL',
  'COM1', 'COM2', 'COM3', 'COM4', 'COM5', 'COM6', 'COM7', 'COM8', 'COM9',
  'LPT1', 'LPT2', 'LPT3', 'LPT4', 'LPT5', 'LPT6', 'LPT7', 'LPT8', 'LPT9',
]);
const PROTECTED_CODE_PATHS = new Set([
  'AGENTS.md',
  'PRIME_DIRECTIVE.md',
  'env.txt',
  'src/apocv4/admission.py',
  'src/apocv4/authority_policy.py',
  'src/apocv4/effect_gateway.py',
  'src/apocv4/patch_effect_executor.py',
  'src/apocv4/patch_runtime.py',
  'src/apocv4/runtime_auth.py',
  'src/apocv4/runtime_service.py',
]);
const PROTECTED_CODE_PREFIXES = [
  '.staging/',
  'config/',
  'deploy/',
  'specs/wayfinder/',
];

type JsonObject = Record<string, unknown>;
type RuntimePath = '/health' | '/v1/chat' | '/v1/code' | '/v1/code/rollback' | '/v1/objectives'
  | '/v1/sessions/list' | '/v1/sessions/get' | '/v1/sessions/delete'
  | '/v1/jobs/submit' | '/v1/jobs/list' | '/v1/jobs/status' | '/v1/jobs/cancel'
  | '/v1/vision';
export type RuntimeCredentialProfile = 'owner' | 'public';

export interface RuntimeChatAuthority {
  effect_authority: 'NONE';
  tool_authority: 'NONE' | 'READ_ONLY_CONTEXT';
  memory_scope: 'ephemeral' | 'owner_partitioned_retrieval' | 'public_safe_retrieval';
  conversation_history: 'not_retained' | 'session_bounded' | 'durable_principal_bound';
  training_consent: false;
}

export interface RuntimeChatIdentity {
  schema_version: 'apocv4.identity.v1';
  system_id: 'apocrypha';
  architecture: 'governed_hybrid_digital_intelligence';
  compiler_version: string;
  identity_digest: string;
  learned_model_role: 'replaceable_faculty_not_system_identity';
  lineage: string;
}

export interface RuntimeChatCapability {
  id: string;
  status: string;
  authority: string;
  evidence: string;
}

export interface RuntimeChatContext extends JsonObject {
  frame_id: string;
  frame_digest: string;
  provenance_spine_digest: string;
  retrieval: JsonObject & { status: string; count: number; refs: unknown[] };
  memory: JsonObject & {
    provider: string;
    status: string;
    records_used: number;
    receipt_digest: string | null;
    refs: unknown[];
  };
  capabilities: RuntimeChatCapability[];
}

export interface RuntimeReceipt {
  observed_at: string;
  latency_ms: number;
  upstream_status: number;
  auth_mode: string | null;
  auth_registry_ref: string | null;
  binding_ref: string | null;
  principal_ref: string | null;
  privacy_partition_ref: string | null;
  effect_scope_ref: string | null;
}

export interface RuntimeHealthProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'health';
  observed: {
    evidence_lane: 'observed_runtime_http';
    receipt: RuntimeReceipt;
    runtime: JsonObject;
  };
  model_reported: {
    evidence_lane: 'model_reported';
    present: false;
    note: string;
  };
}

export interface RuntimeObjectiveProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'objective';
  observed: {
    evidence_lane: 'observed_runtime_http_and_test_receipts';
    receipt: RuntimeReceipt;
    runtime: JsonObject;
    attempts: JsonObject[];
  };
  model_reported: {
    evidence_lane: 'model_reported_not_observed_fact';
    note: string;
    attempts: JsonObject[];
  };
}

export interface RuntimeChatProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'chat';
  observed: {
    evidence_lane: 'observed_runtime_http_and_transport';
    receipt: RuntimeReceipt;
    runtime: JsonObject;
  };
  model_reported: JsonObject & {
    evidence_lane: 'model_reported_not_observed_fact';
    text: string;
  };
  authority: RuntimeChatAuthority;
  identity: RuntimeChatIdentity | null;
  context: RuntimeChatContext | null;
}

export interface RuntimeChatInput {
  message: string;
  conversationId: string;
  requestId: string;
  sessionId?: string;
  sessionPrincipal?: RuntimeSessionPrincipal;
  privacyPartition: string;
  credentialProfile?: RuntimeCredentialProfile;
}

export interface RuntimeSessionBindingInput {
  sessionPrincipal: RuntimeSessionPrincipal;
  privacyPartition: string;
  credentialProfile?: RuntimeCredentialProfile;
}

export interface RuntimeSessionListInput extends RuntimeSessionBindingInput {
  limit?: number;
}

export interface RuntimeSessionGetInput extends RuntimeSessionBindingInput {
  sessionId: string;
}

export interface RuntimeSessionDeleteInput extends RuntimeSessionGetInput {
  requestId: string;
}

export interface RuntimeJobSubmitInput extends RuntimeSessionGetInput {
  requestId: string;
  objective: string;
  maxIterations?: number;
}

export interface RuntimeJobStatusInput extends RuntimeSessionGetInput {
  jobId: string;
}

export interface RuntimeJobCancelInput extends RuntimeJobStatusInput {
  requestId: string;
}

export interface RuntimeJobProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'job';
  observed: RuntimeSessionObservation;
  job: JsonObject;
}

export interface RuntimeJobListProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'job_list';
  observed: RuntimeSessionObservation;
  jobs: JsonObject[];
  count: number;
}

export interface RuntimeVisionInput {
  imageB64: string;
  mimeType: 'image/jpeg' | 'image/png' | 'image/webp';
  observedAt: string;
  perceptId: string;
  provenanceRef: string;
  question: string;
  privacyPartition: string;
  credentialProfile?: RuntimeCredentialProfile;
}

export interface RuntimeVisionProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'vision';
  observed: {
    evidence_lane: 'model_reported_visual_observation_over_observed_runtime_transport';
    receipt: RuntimeReceipt;
    perception_digest: string;
    observation: JsonObject;
    runtime_state: JsonObject;
  };
}

export interface RuntimeSessionSummary extends JsonObject {
  session_id: string;
  title: string;
  created_at: string;
  updated_at: string;
  message_count: number;
  active_job_count: number;
  artifact_count: number;
  tip_digest: string;
}

export interface RuntimeSessionSnapshot extends JsonObject {
  schema_version: 'apocv4.workspace-session-snapshot.v1';
  session_id: string;
  title: string;
  created_at: string;
  updated_at: string;
  event_count: number;
  events_truncated: boolean;
  tip_digest: string;
  messages: JsonObject[];
  turn_states: JsonObject[];
  jobs: JsonObject[];
  artifacts: JsonObject[];
  code_requests: JsonObject[];
  proposals: JsonObject[];
  effects: JsonObject[];
  surface_truncation: JsonObject;
  world: JsonObject;
  workspace: JsonObject;
}

interface RuntimeSessionObservation {
  evidence_lane: 'observed_runtime_http_and_principal_bound_session';
  receipt: RuntimeReceipt;
  request_contract: 'not_applicable' | 'session_id';
}

export interface RuntimeSessionListProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'session_list';
  observed: RuntimeSessionObservation;
  sessions: RuntimeSessionSummary[];
  count: number;
}

export interface RuntimeSessionGetProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'session_get';
  observed: RuntimeSessionObservation;
  session: RuntimeSessionSnapshot;
}

export interface RuntimeSessionDeleteProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'session_delete';
  observed: RuntimeSessionObservation;
  session_id: string;
  deleted: true;
  event_digest: string;
}

export interface RuntimeCodeInput {
  objective: string;
  allowedPaths: string[];
  privacyPartition: string;
  sessionId?: string;
  sessionPrincipal?: RuntimeSessionPrincipal;
  requestId?: string;
}

export interface RuntimeRollbackInput {
  promotionEventDigest: string;
  sessionId?: string;
  sessionPrincipal?: RuntimeSessionPrincipal;
  requestId?: string;
  privacyPartition?: string;
}

export interface RuntimeCodeProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'code';
  observed: {
    evidence_lane: 'observed_runtime_http_effect_and_test_receipts';
    receipt: RuntimeReceipt;
    runtime: JsonObject;
    test: JsonObject | null;
  };
  generated: {
    evidence_lane: 'model_generated_artifact_identity_not_source_diff';
    proposal_digest: string;
    requested_allowed_paths: string[];
    faculty_attempts: JsonObject[];
  };
}

export interface RuntimeRollbackProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'rollback';
  observed: {
    evidence_lane: 'observed_runtime_http_rollback_receipt';
    receipt: RuntimeReceipt;
    runtime: JsonObject;
  };
  model_reported: {
    evidence_lane: 'model_reported';
    present: false;
    note: string;
  };
}

export class RuntimeProxyError extends Error {
  readonly code: string;
  readonly publicStatus: number;
  readonly upstreamStatus: number | null;
  readonly observedAt: string;
  readonly deadlineMs: number | null;

  constructor(
    code: string,
    publicStatus: number,
    upstreamStatus: number | null = null,
    deadlineMs: number | null = null,
  ) {
    super(code);
    this.name = 'RuntimeProxyError';
    this.code = code;
    this.publicStatus = publicStatus;
    this.upstreamStatus = upstreamStatus;
    this.observedAt = new Date().toISOString();
    this.deadlineMs = deadlineMs;
  }
}

interface RuntimeCall {
  data: JsonObject;
  rollback_lease_ref: string | null;
  receipt: RuntimeReceipt;
}

function isObject(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function exactKeys(value: JsonObject, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return actual.length === wanted.length && actual.every((key, index) => key === wanted[index]);
}

function boundedCanonicalString(value: unknown, maximumLength = 512): value is string {
  return typeof value === 'string'
    && value === value.trim()
    && value.length > 0
    && value.length <= maximumLength;
}

function boundedCanonicalUtf8(value: unknown, maximumBytes: number): value is string {
  return typeof value === 'string'
    && value === value.trim()
    && value.length > 0
    && Buffer.byteLength(value, 'utf8') <= maximumBytes;
}

function boundedJsonValue(value: unknown, maximumBytes: number): boolean {
  try {
    const encoded = JSON.stringify(value);
    return typeof encoded === 'string' && Buffer.byteLength(encoded, 'utf8') <= maximumBytes;
  } catch {
    return false;
  }
}

function canonicalJson(value: unknown): string {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') {
    return JSON.stringify(value);
  }
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new RuntimeProxyError('session_binding_invalid', 500);
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  if (isObject(value)) {
    return `{${Object.keys(value).sort().map((key) => (
      `${JSON.stringify(key)}:${canonicalJson(value[key])}`
    )).join(',')}}`;
  }
  throw new RuntimeProxyError('session_binding_invalid', 500);
}

function sessionBindingSecret(): Buffer {
  const raw = process.env.APOCV4_SESSION_BINDING_SECRET;
  if (!raw || raw !== raw.trim()) {
    throw new RuntimeProxyError('runtime_session_binding_unavailable', 503);
  }
  const encoded = Buffer.from(raw, 'utf8');
  if (
    encoded.length < 32
    || encoded.length > 8_192
    || [...encoded].some((byte) => byte < 0x21 || byte > 0x7e)
  ) {
    throw new RuntimeProxyError('runtime_session_binding_unavailable', 503);
  }
  return encoded;
}

function withSessionBinding(path: RuntimePath, body: JsonObject): JsonObject {
  if (!Object.hasOwn(body, 'session_principal')) return body;
  if (
    Object.hasOwn(body, 'session_binding_mac')
    || typeof body.session_principal !== 'string'
    || !isRuntimeSessionPrincipal(body.session_principal)
  ) {
    throw new RuntimeProxyError('session_binding_invalid', 500);
  }
  const mac = createHmac('sha256', sessionBindingSecret())
    .update(canonicalJson({ schema_version: SESSION_BINDING_SCHEMA, path, body }), 'utf8')
    .digest('hex');
  return { ...body, session_binding_mac: mac };
}

function canonicalTimestamp(value: unknown): value is string {
  return boundedCanonicalString(value, 64)
    && /(?:Z|[+-][0-9]{2}:[0-9]{2})$/.test(value)
    && Number.isFinite(Date.parse(value));
}

function boundedNonnegativeInteger(value: unknown, maximum = 1_000_000): value is number {
  return Number.isInteger(value) && Number(value) >= 0 && Number(value) <= maximum;
}

function validateSessionBinding(input: RuntimeSessionBindingInput): void {
  const { sessionPrincipal, privacyPartition, credentialProfile = 'owner' } = input;
  if (
    !isRuntimeSessionPrincipal(sessionPrincipal)
    || !boundedCanonicalString(privacyPartition, 256)
    || (credentialProfile !== 'owner' && credentialProfile !== 'public')
    || (credentialProfile === 'owner' && privacyPartition !== 'owner:apocky')
    || (credentialProfile === 'public' && privacyPartition !== 'public:apocrypha')
  ) {
    throw new RuntimeProxyError('session_request_invalid', 400);
  }
}

function normalizeSessionIdentity(
  value: JsonObject,
  otherKeys: readonly string[],
  expectedSessionId?: string,
): { sessionId: string; legacy: boolean } | null {
  const canonical = exactKeys(value, ['session_id', ...otherKeys]);
  const legacy = exactKeys(value, ['conversation_id', ...otherKeys]);
  if (canonical === legacy) return null;
  const candidate = canonical ? value.session_id : value.conversation_id;
  if (
    typeof candidate !== 'string'
    || !CLIENT_SESSION_UUID_RE.test(candidate)
    || (expectedSessionId !== undefined && candidate !== expectedSessionId)
  ) return null;
  return { sessionId: candidate, legacy };
}

function normalizeSessionSummary(value: unknown): RuntimeSessionSummary | null {
  if (!isObject(value)) return null;
  const otherKeys = [
    'title', 'created_at', 'updated_at', 'message_count',
    'active_job_count', 'artifact_count', 'tip_digest',
  ];
  const identity = normalizeSessionIdentity(value, otherKeys);
  if (
    !identity
    || !boundedCanonicalString(value.title, 256)
    || !canonicalTimestamp(value.created_at)
    || !canonicalTimestamp(value.updated_at)
    || !boundedNonnegativeInteger(value.message_count)
    || !boundedNonnegativeInteger(value.active_job_count)
    || !boundedNonnegativeInteger(value.artifact_count)
    || typeof value.tip_digest !== 'string'
    || !SHA256_RE.test(value.tip_digest)
  ) return null;
  return {
    session_id: identity.sessionId,
    title: value.title,
    created_at: value.created_at,
    updated_at: value.updated_at,
    message_count: value.message_count,
    active_job_count: value.active_job_count,
    artifact_count: value.artifact_count,
    tip_digest: value.tip_digest,
  };
}

function projectSessionTurnReceipt(
  result: JsonObject,
  credentialProfile: RuntimeCredentialProfile,
): JsonObject | null {
  const model = result.model_reported;
  const authority = result.authority;
  const expectedMemoryScope = credentialProfile === 'owner'
    ? 'owner_partitioned_retrieval'
    : 'public_safe_retrieval';
  if (
    result.schema_version !== 'apocv4.chat-response.v2'
    || !isObject(model)
    || !isObject(authority)
    || !boundedCanonicalString(model.model_id, 256)
    || !boundedCanonicalString(model.response_id, 256)
    || typeof model.response_digest !== 'string'
    || !SHA256_RE.test(model.response_digest)
    || typeof model.serving_profile_digest !== 'string'
    || !SHA256_RE.test(model.serving_profile_digest)
    || authority.memory_scope !== expectedMemoryScope
    || authority.conversation_history !== 'durable_principal_bound'
    || !validateV2ChatIdentity(result.identity)
    || !validateV2ChatContext(result.context)
  ) return null;
  return {
    model_id: model.model_id,
    response_id: model.response_id,
    response_digest: model.response_digest,
    serving_profile_digest: model.serving_profile_digest,
    memory_scope: authority.memory_scope,
    conversation_history: authority.conversation_history,
    identity: result.identity,
    context: result.context,
  };
}

function normalizeSessionMessage(
  value: unknown,
  credentialProfile: RuntimeCredentialProfile,
): JsonObject | null {
  if (!isObject(value) || (value.role !== 'user' && value.role !== 'assistant')) return null;
  const baseKeys = ['role', 'content', 'request_id', 'recorded_at', 'event_digest'];
  const exactBase = exactKeys(value, baseKeys);
  const exactLegacyAssistant = value.role === 'assistant' && exactKeys(value, [...baseKeys, 'result']);
  if (
    (!exactBase && !exactLegacyAssistant)
    || !boundedCanonicalUtf8(value.content, 128 * 1024)
    || typeof value.request_id !== 'string'
    || !UUID_RE.test(value.request_id)
    || !canonicalTimestamp(value.recorded_at)
    || typeof value.event_digest !== 'string'
    || !SHA256_RE.test(value.event_digest)
    || (exactLegacyAssistant && (!isObject(value.result) || !boundedJsonValue(value.result, CHAT_RESPONSE_LIMIT)))
  ) return null;
  const receipt = exactLegacyAssistant
    ? projectSessionTurnReceipt(value.result as JsonObject, credentialProfile)
    : null;
  return {
    role: value.role,
    content: value.content,
    request_id: value.request_id,
    recorded_at: value.recorded_at,
    event_digest: value.event_digest,
    ...(receipt ? { receipt } : {}),
  };
}

function onlyKnownKeys(
  value: JsonObject,
  required: readonly string[],
  optional: readonly string[] = [],
): boolean {
  const allowed = new Set([...required, ...optional]);
  return required.every((key) => Object.hasOwn(value, key))
    && Object.keys(value).every((key) => allowed.has(key));
}

function nullableDigest(value: unknown): value is string | null {
  return value === null || (typeof value === 'string' && SHA256_RE.test(value));
}

function canonicalStringList(
  value: unknown,
  maximumItems: number,
  maximumItemLength: number,
  pattern?: RegExp,
): value is string[] {
  return Array.isArray(value)
    && value.length <= maximumItems
    && value.every((entry) => boundedCanonicalString(entry, maximumItemLength)
      && (pattern === undefined || pattern.test(entry)));
}

function canonicalCodePathList(value: unknown, allowEmpty = false): value is string[] {
  if (!Array.isArray(value) || (!allowEmpty && value.length === 0)) return false;
  try {
    canonicalCodePaths(value);
    return true;
  } catch {
    return allowEmpty && value.length === 0;
  }
}

function normalizeSessionTurnState(value: unknown): JsonObject | null {
  if (!isObject(value)) return null;
  const base = [
    'request_id', 'state', 'recorded_at', 'user_event_digest', 'terminal_event_digest',
  ];
  if (
    typeof value.request_id !== 'string'
    || !UUID_RE.test(value.request_id)
    || !canonicalTimestamp(value.recorded_at)
    || typeof value.user_event_digest !== 'string'
    || !SHA256_RE.test(value.user_event_digest)
  ) return null;
  if (value.state === 'PENDING') {
    if (!exactKeys(value, base) || value.terminal_event_digest !== null) return null;
    return {
      request_id: value.request_id,
      state: 'PENDING',
      recorded_at: value.recorded_at,
      user_event_digest: value.user_event_digest,
      terminal_event_digest: null,
    };
  }
  if (
    value.state !== 'FAILED'
    || !onlyKnownKeys(
      value,
      [...base, 'error_class', 'error_digest'],
      ['failure_code', 'rejected_result_digest'],
    )
    || typeof value.terminal_event_digest !== 'string'
    || !SHA256_RE.test(value.terminal_event_digest)
    || !boundedCanonicalString(value.error_class, 128)
    || typeof value.error_digest !== 'string'
    || !SHA256_RE.test(value.error_digest)
    || (Object.hasOwn(value, 'failure_code') && !boundedCanonicalString(value.failure_code, 128))
    || (Object.hasOwn(value, 'rejected_result_digest')
      && (typeof value.rejected_result_digest !== 'string'
        || !SHA256_RE.test(value.rejected_result_digest)))
  ) return null;
  return selected(value, [
    ...base, 'error_class', 'error_digest', 'failure_code', 'rejected_result_digest',
  ]);
}

function normalizeSessionJob(value: unknown): JsonObject | null {
  if (!isObject(value) || !onlyKnownKeys(value, ['job_id', 'state'], [
    'request_id', 'request_digest', 'action_id', 'arguments', 'attempt', 'output',
    'output_digest', 'artifact_ids', 'error_class', 'error_digest', 'reason_code',
    'cancel_request_id', 'recovered_from',
  ])) return null;
  const state = value.state;
  if (
    typeof value.job_id !== 'string'
    || !/^job:[0-9a-f]{64}$/.test(value.job_id)
    || !['QUEUED', 'RUNNING', 'CANCEL_REQUESTED', 'SUCCEEDED', 'FAILED', 'CANCELLED', 'INTERRUPTED']
      .includes(String(state))
    || (Object.hasOwn(value, 'request_id')
      && (typeof value.request_id !== 'string' || !UUID_RE.test(value.request_id)))
    || (Object.hasOwn(value, 'request_digest')
      && (typeof value.request_digest !== 'string' || !SHA256_RE.test(value.request_digest)))
    || (Object.hasOwn(value, 'action_id')
      && (typeof value.action_id !== 'string' || !/^[a-z][a-z0-9_.:-]{0,127}$/.test(value.action_id)))
    || (Object.hasOwn(value, 'arguments') && !boundedJsonValue(value.arguments, 64 * 1024))
    || (Object.hasOwn(value, 'attempt') && !boundedNonnegativeInteger(value.attempt, 1_000))
    || (Object.hasOwn(value, 'output') && !boundedJsonValue(value.output, 64 * 1024))
    || (Object.hasOwn(value, 'output_digest')
      && (typeof value.output_digest !== 'string' || !SHA256_RE.test(value.output_digest)))
    || (Object.hasOwn(value, 'artifact_ids')
      && !canonicalStringList(value.artifact_ids, 16, 73, /^artifact:[0-9a-f]{64}$/))
    || (Object.hasOwn(value, 'error_class') && !boundedCanonicalString(value.error_class, 128))
    || (Object.hasOwn(value, 'error_digest')
      && (typeof value.error_digest !== 'string' || !SHA256_RE.test(value.error_digest)))
    || (Object.hasOwn(value, 'reason_code') && !boundedCanonicalString(value.reason_code, 128))
    || (Object.hasOwn(value, 'cancel_request_id')
      && (typeof value.cancel_request_id !== 'string' || !UUID_RE.test(value.cancel_request_id)))
    || (Object.hasOwn(value, 'recovered_from') && value.recovered_from !== 'RUNNING')
  ) return null;
  // Arguments and raw output can contain private tool material. Their digests,
  // state, and artifact references are sufficient for the browser world model.
  return selected(value, [
    'job_id', 'state', 'request_id', 'request_digest', 'action_id', 'attempt',
    'output_digest', 'artifact_ids', 'error_class', 'error_digest', 'reason_code',
    'cancel_request_id', 'recovered_from',
  ]);
}

function normalizeSessionArtifact(value: unknown): JsonObject | null {
  if (!isObject(value) || !exactKeys(value, [
    'artifact_id', 'kind', 'title', 'content', 'content_digest', 'content_bytes',
    'refs', 'event_digest',
  ])) return null;
  if (
    typeof value.artifact_id !== 'string'
    || !/^artifact:[0-9a-f]{64}$/.test(value.artifact_id)
    || !boundedCanonicalString(value.kind, 128)
    || !boundedCanonicalString(value.title, 256)
    || !boundedJsonValue(value.content, 128 * 1024)
    || typeof value.content_digest !== 'string'
    || !SHA256_RE.test(value.content_digest)
    || !boundedNonnegativeInteger(value.content_bytes, 128 * 1024)
    || !canonicalStringList(value.refs, 32, 512)
    || typeof value.event_digest !== 'string'
    || !SHA256_RE.test(value.event_digest)
  ) return null;
  return selected(value, [
    'artifact_id', 'kind', 'title', 'content', 'content_digest', 'content_bytes',
    'refs', 'event_digest',
  ]);
}

function normalizeSessionCodeRequest(value: unknown): JsonObject | null {
  if (!isObject(value) || !exactKeys(value, [
    'objective', 'objective_digest', 'allowed_paths', 'allowed_paths_digest',
    'request_contract_digest', 'request_id', 'recorded_at', 'event_digest',
  ])) return null;
  if (
    !boundedCanonicalUtf8(value.objective, 32 * 1024)
    || typeof value.objective_digest !== 'string'
    || !SHA256_RE.test(value.objective_digest)
    || !canonicalCodePathList(value.allowed_paths)
    || typeof value.allowed_paths_digest !== 'string'
    || !SHA256_RE.test(value.allowed_paths_digest)
    || typeof value.request_contract_digest !== 'string'
    || !SHA256_RE.test(value.request_contract_digest)
    || typeof value.request_id !== 'string'
    || !UUID_RE.test(value.request_id)
    || !canonicalTimestamp(value.recorded_at)
    || typeof value.event_digest !== 'string'
    || !SHA256_RE.test(value.event_digest)
  ) return null;
  return selected(value, [
    'objective', 'objective_digest', 'allowed_paths', 'allowed_paths_digest',
    'request_contract_digest', 'request_id', 'recorded_at', 'event_digest',
  ]);
}

function normalizeSessionProposal(value: unknown): JsonObject | null {
  if (!isObject(value) || !exactKeys(value, [
    'proposal_digest', 'objective_digest', 'allowed_paths', 'state', 'runtime_state',
    'frame_digest', 'authority_digest', 'request_digest', 'approval_digest', 'test_state',
    'artifact_ids', 'request_id', 'recorded_at', 'event_digest',
  ])) return null;
  if (
    typeof value.proposal_digest !== 'string'
    || !SHA256_RE.test(value.proposal_digest)
    || typeof value.objective_digest !== 'string'
    || !SHA256_RE.test(value.objective_digest)
    || !canonicalCodePathList(value.allowed_paths)
    || !boundedCanonicalString(value.state, 128)
    || !boundedCanonicalString(value.runtime_state, 128)
    || !nullableDigest(value.frame_digest)
    || !nullableDigest(value.authority_digest)
    || !nullableDigest(value.request_digest)
    || !nullableDigest(value.approval_digest)
    || !boundedCanonicalString(value.test_state, 128)
    || !canonicalStringList(value.artifact_ids, 32, 73, /^artifact:[0-9a-f]{64}$/)
    || typeof value.request_id !== 'string'
    || !UUID_RE.test(value.request_id)
    || !canonicalTimestamp(value.recorded_at)
    || typeof value.event_digest !== 'string'
    || !SHA256_RE.test(value.event_digest)
  ) return null;
  return selected(value, [
    'proposal_digest', 'objective_digest', 'allowed_paths', 'state', 'runtime_state',
    'frame_digest', 'authority_digest', 'request_digest', 'approval_digest', 'test_state',
    'artifact_ids', 'request_id', 'recorded_at', 'event_digest',
  ]);
}

function normalizeSessionEffect(value: unknown): JsonObject | null {
  if (!isObject(value)) return null;
  const common = ['state', 'request_id', 'recorded_at', 'event_digest'];
  if (
    !boundedCanonicalString(value.state, 128)
    || typeof value.request_id !== 'string'
    || !UUID_RE.test(value.request_id)
    || !canonicalTimestamp(value.recorded_at)
    || typeof value.event_digest !== 'string'
    || !SHA256_RE.test(value.event_digest)
  ) return null;
  if (!Object.hasOwn(value, 'scope')) {
    if (!exactKeys(value, [
      ...common, 'proposal_digest', 'objective_digest', 'promotion_event_digest',
      'terminal_event_digest', 'rollback_event_digest', 'changed_paths', 'test_state',
      'artifact_ids', 'result_digest', 'result_metadata', 'result_metadata_bytes',
    ])) return null;
    if (
      typeof value.proposal_digest !== 'string'
      || !SHA256_RE.test(value.proposal_digest)
      || typeof value.objective_digest !== 'string'
      || !SHA256_RE.test(value.objective_digest)
      || !nullableDigest(value.promotion_event_digest)
      || typeof value.terminal_event_digest !== 'string'
      || !SHA256_RE.test(value.terminal_event_digest)
      || !nullableDigest(value.rollback_event_digest)
      || !canonicalCodePathList(value.changed_paths, true)
      || !boundedCanonicalString(value.test_state, 128)
      || !canonicalStringList(value.artifact_ids, 32, 73, /^artifact:[0-9a-f]{64}$/)
      || typeof value.result_digest !== 'string'
      || !SHA256_RE.test(value.result_digest)
      || !isObject(value.result_metadata)
      || !boundedJsonValue(value.result_metadata, 512 * 1024)
      || !boundedNonnegativeInteger(value.result_metadata_bytes, 512 * 1024)
    ) return null;
    return {
      kind: 'CODE_EFFECT',
      ...selected(value, [
        'request_id', 'proposal_digest', 'objective_digest', 'state',
        'promotion_event_digest', 'terminal_event_digest', 'rollback_event_digest',
        'changed_paths', 'test_state', 'artifact_ids', 'result_digest',
        'recorded_at', 'event_digest',
      ]),
    };
  }

  const scope = value.scope;
  if (scope === 'code_execution_failure' || scope === 'unknown_effect_state') {
    const required = [
      ...common, 'proposal_digest', 'scope', 'promotion_event_digest',
      'terminal_event_digest', 'rollback_event_digest', 'changed_paths', 'test_state',
      'failure_code', 'error_class', 'error_digest',
    ];
    if (
      !exactKeys(value, required)
      || value.proposal_digest !== null
      || value.promotion_event_digest !== null
      || typeof value.terminal_event_digest !== 'string'
      || !SHA256_RE.test(value.terminal_event_digest)
      || value.rollback_event_digest !== null
      || !Array.isArray(value.changed_paths)
      || value.changed_paths.length !== 0
      || value.test_state !== 'NOT_RUN'
      || !boundedCanonicalString(value.failure_code, 128)
      || !boundedCanonicalString(value.error_class, 128)
      || typeof value.error_digest !== 'string'
      || !SHA256_RE.test(value.error_digest)
      || (scope === 'code_execution_failure' && value.state !== 'FAILED')
      || (scope === 'unknown_effect_state' && value.state !== 'UNKNOWN_EFFECT_STATE')
    ) return null;
    return {
      kind: 'CODE_FAILURE',
      request_id: value.request_id,
      proposal_digest: null,
      state: value.state,
      promotion_event_digest: null,
      terminal_event_digest: value.terminal_event_digest,
      rollback_event_digest: null,
      changed_paths: [],
      test_state: 'NOT_RUN',
      scope,
      failure_code: value.failure_code,
      error_class: value.error_class,
      error_digest: value.error_digest,
      recorded_at: value.recorded_at,
      event_digest: value.event_digest,
    };
  }
  let required: string[];
  if (scope === 'isolated_execution') {
    required = [
      ...common, 'proposal_digest', 'scope', 'promotion_event_digest',
      'rollback_event_digest', 'test_state',
    ];
  } else if (scope === 'promoted_effect_compensation') {
    required = [
      ...common, 'proposal_digest', 'scope', 'promotion_event_digest',
      'rollback_event_digest', 'test_state', 'reason_code', 'settlement_error_class',
      'settlement_error_digest',
    ];
  } else if (scope === 'manual_promotion_rollback') {
    required = [
      ...common, 'scope', 'promotion_event_digest', 'rollback_event_digest', 'test_state',
      'result_digest', 'result_metadata', 'result_metadata_bytes',
    ];
  } else {
    return null;
  }
  if (!exactKeys(value, required)) return null;
  if (
    !nullableDigest(value.promotion_event_digest)
    || typeof value.rollback_event_digest !== 'string'
    || !SHA256_RE.test(value.rollback_event_digest)
    || !boundedCanonicalString(value.test_state, 128)
    || (Object.hasOwn(value, 'proposal_digest') && !nullableDigest(value.proposal_digest))
    || (Object.hasOwn(value, 'reason_code') && !boundedCanonicalString(value.reason_code, 128))
    || (Object.hasOwn(value, 'settlement_error_class')
      && !boundedCanonicalString(value.settlement_error_class, 128))
    || (Object.hasOwn(value, 'settlement_error_digest')
      && (typeof value.settlement_error_digest !== 'string'
        || !SHA256_RE.test(value.settlement_error_digest)))
    || (Object.hasOwn(value, 'result_digest')
      && (typeof value.result_digest !== 'string' || !SHA256_RE.test(value.result_digest)))
    || (Object.hasOwn(value, 'result_metadata')
      && (!isObject(value.result_metadata) || !boundedJsonValue(value.result_metadata, 512 * 1024)))
    || (Object.hasOwn(value, 'result_metadata_bytes')
      && !boundedNonnegativeInteger(value.result_metadata_bytes, 512 * 1024))
  ) return null;
  return {
    kind: 'ROLLBACK',
    request_id: value.request_id,
    proposal_digest: Object.hasOwn(value, 'proposal_digest') ? value.proposal_digest : null,
    state: value.state,
    promotion_event_digest: value.promotion_event_digest,
    terminal_event_digest: null,
    rollback_event_digest: value.rollback_event_digest,
    changed_paths: [],
    test_state: value.test_state,
    scope,
    ...(Object.hasOwn(value, 'reason_code') ? { reason_code: value.reason_code } : {}),
    ...(Object.hasOwn(value, 'settlement_error_class')
      ? { settlement_error_class: value.settlement_error_class } : {}),
    ...(Object.hasOwn(value, 'settlement_error_digest')
      ? { settlement_error_digest: value.settlement_error_digest } : {}),
    ...(Object.hasOwn(value, 'result_digest') ? { result_digest: value.result_digest } : {}),
    recorded_at: value.recorded_at,
    event_digest: value.event_digest,
  };
}

function normalizeSessionArray(
  value: unknown,
  normalizer: (entry: unknown) => JsonObject | null,
): JsonObject[] | null {
  if (!Array.isArray(value) || value.length > SESSION_UPSTREAM_ARRAY_LIMIT) return null;
  const projected = value.map(normalizer);
  return projected.some((entry) => entry === null) ? null : projected as JsonObject[];
}

function normalizeSurfaceTruncation(value: unknown): Record<string, JsonObject> | null {
  const surfaces = [
    'messages', 'turn_states', 'jobs', 'artifacts', 'code_requests', 'proposals', 'effects',
  ] as const;
  if (!isObject(value) || !exactKeys(value, surfaces)) return null;
  const result: Record<string, JsonObject> = {};
  for (const surface of surfaces) {
    const entry = value[surface];
    if (
      !isObject(entry)
      || !exactKeys(entry, ['total', 'visible', 'truncated'])
      || !boundedNonnegativeInteger(entry.total)
      || !boundedNonnegativeInteger(entry.visible)
      || Number(entry.visible) > Number(entry.total)
      || typeof entry.truncated !== 'boolean'
      || entry.truncated !== (entry.visible < entry.total)
    ) return null;
    result[surface] = {
      total: entry.total,
      visible: entry.visible,
      truncated: entry.truncated,
    };
  }
  return result;
}

function validateSessionWorkspace(
  value: unknown,
  credentialProfile: RuntimeCredentialProfile,
): value is JsonObject {
  if (!isObject(value)) return false;
  if (exactKeys(value, ['status', 'effect_authority'])) {
    return value.status === 'not_authorized' && value.effect_authority === 'NONE';
  }
  if (credentialProfile !== 'owner') return false;
  if (
    value.status === 'degraded_observation_failed'
    && exactKeys(value, ['status', 'error_class', 'error_digest'])
  ) {
    return boundedCanonicalString(value.error_class, 128)
      && typeof value.error_digest === 'string'
      && SHA256_RE.test(value.error_digest);
  }
  if (
    value.status === 'source_prestate_unavailable'
    && exactKeys(value, ['workspace_ref', 'effect_mode', 'status'])
  ) {
    return typeof value.workspace_ref === 'string'
      && SHA256_RE.test(value.workspace_ref)
      && (value.effect_mode === 'journaled_tested_atomic_promotion' || value.effect_mode === 'proposal_only');
  }
  if (!exactKeys(value, [
    'workspace_ref', 'effect_mode', 'status', 'head_commit', 'base_commit', 'base_tree',
    'status_digest', 'unstaged_diff_digest', 'staged_diff_digest', 'untracked_digest',
    'prestate_digest',
  ])) return false;
  return value.status === 'observed'
    && typeof value.workspace_ref === 'string'
    && SHA256_RE.test(value.workspace_ref)
    && (value.effect_mode === 'journaled_tested_atomic_promotion' || value.effect_mode === 'proposal_only')
    && boundedCanonicalString(value.head_commit, 128)
    && boundedCanonicalString(value.base_commit, 128)
    && boundedCanonicalString(value.base_tree, 128)
    && ['status_digest', 'unstaged_diff_digest', 'staged_diff_digest', 'untracked_digest', 'prestate_digest']
      .every((key) => typeof value[key] === 'string' && SHA256_RE.test(value[key] as string));
}

function boundedTail(
  value: JsonObject[],
  maximumItems: number,
  maximumBytes: number,
): { items: JsonObject[]; truncated: boolean } {
  const items: JsonObject[] = [];
  let usedBytes = 2;
  let truncated = false;
  for (let index = value.length - 1; index >= 0; index -= 1) {
    const entry = value[index];
    if (!entry) continue;
    const encoded = JSON.stringify(entry);
    const bytes = Buffer.byteLength(encoded, 'utf8') + (items.length > 0 ? 1 : 0);
    if (items.length >= maximumItems || usedBytes + bytes > maximumBytes) {
      truncated = true;
      break;
    }
    items.unshift(entry);
    usedBytes += bytes;
  }
  return { items, truncated };
}

function normalizeSessionSnapshot(
  value: unknown,
  expectedSessionId: string,
  credentialProfile: RuntimeCredentialProfile,
): RuntimeSessionSnapshot | null {
  if (!isObject(value)) return null;
  const otherKeys = [
    'schema_version', 'title', 'created_at', 'updated_at',
    'event_count', 'events_truncated', 'tip_digest', 'messages', 'jobs', 'artifacts',
    'turn_states', 'code_requests', 'proposals', 'effects', 'surface_truncation',
    'world', 'workspace',
  ];
  if (!exactKeys(value, ['session_id', ...otherKeys]) || value.session_id !== expectedSessionId) {
    return null;
  }
  const world = value.world;
  const turnStates = normalizeSessionArray(value.turn_states, normalizeSessionTurnState);
  const jobs = normalizeSessionArray(value.jobs, normalizeSessionJob);
  const artifacts = normalizeSessionArray(value.artifacts, normalizeSessionArtifact);
  const codeRequests = normalizeSessionArray(value.code_requests, normalizeSessionCodeRequest);
  const proposals = normalizeSessionArray(value.proposals, normalizeSessionProposal);
  const effects = normalizeSessionArray(value.effects, normalizeSessionEffect);
  const upstreamTruncation = normalizeSurfaceTruncation(value.surface_truncation);
  if (
    value.schema_version !== 'apocv4.workspace-session-snapshot.v1'
    || !boundedCanonicalString(value.title, 256)
    || !canonicalTimestamp(value.created_at)
    || !canonicalTimestamp(value.updated_at)
    || !boundedNonnegativeInteger(value.event_count)
    || typeof value.events_truncated !== 'boolean'
    || typeof value.tip_digest !== 'string'
    || !SHA256_RE.test(value.tip_digest)
    || !Array.isArray(value.messages)
    || value.messages.length > SESSION_UPSTREAM_ARRAY_LIMIT
    || turnStates === null
    || jobs === null
    || artifacts === null
    || codeRequests === null
    || proposals === null
    || effects === null
    || upstreamTruncation === null
    || upstreamTruncation.messages?.visible !== value.messages.length
    || upstreamTruncation.turn_states?.visible !== turnStates.length
    || upstreamTruncation.jobs?.visible !== jobs.length
    || upstreamTruncation.artifacts?.visible !== artifacts.length
    || upstreamTruncation.code_requests?.visible !== codeRequests.length
    || upstreamTruncation.proposals?.visible !== proposals.length
    || upstreamTruncation.effects?.visible !== effects.length
    || !isObject(world)
    || !exactKeys(world, [
      'message_count', 'pending_turn_count', 'failed_turn_count', 'active_job_count',
      'artifact_count', 'code_request_count', 'proposal_count', 'effect_count',
      'last_event_type', 'last_event_digest',
    ])
    || !boundedNonnegativeInteger(world.message_count)
    || world.message_count < value.messages.length
    || !boundedNonnegativeInteger(world.pending_turn_count)
    || !boundedNonnegativeInteger(world.failed_turn_count)
    || !boundedNonnegativeInteger(world.active_job_count)
    || !boundedNonnegativeInteger(world.artifact_count)
    || !boundedNonnegativeInteger(world.code_request_count)
    || !boundedNonnegativeInteger(world.proposal_count)
    || !boundedNonnegativeInteger(world.effect_count)
    || world.message_count !== upstreamTruncation.messages?.total
    || Number(world.pending_turn_count) + Number(world.failed_turn_count)
      !== upstreamTruncation.turn_states?.total
    || world.artifact_count !== upstreamTruncation.artifacts?.total
    || world.code_request_count !== upstreamTruncation.code_requests?.total
    || world.proposal_count !== upstreamTruncation.proposals?.total
    || world.effect_count !== upstreamTruncation.effects?.total
    || Number(world.active_job_count) > Number(upstreamTruncation.jobs?.total)
    || !boundedCanonicalString(world.last_event_type, 64)
    || typeof world.last_event_digest !== 'string'
    || !SHA256_RE.test(world.last_event_digest)
    || !validateSessionWorkspace(value.workspace, credentialProfile)
    || !boundedJsonValue(value, SESSION_RESPONSE_LIMIT)
  ) return null;
  const messages = value.messages.map((message) => (
    normalizeSessionMessage(message, credentialProfile)
  ));
  if (messages.some((entry) => entry === null)) return null;
  const messageProjection = boundedTail(
    messages as JsonObject[], SESSION_MESSAGE_LIMIT, SESSION_MESSAGE_BYTES,
  );
  const turnStateProjection = boundedTail(
    turnStates, SESSION_TURN_STATE_LIMIT, SESSION_TURN_STATE_BYTES,
  );
  const jobProjection = boundedTail(jobs, SESSION_JOB_LIMIT, SESSION_JOB_BYTES);
  const artifactProjection = boundedTail(
    artifacts, SESSION_ARTIFACT_LIMIT, SESSION_ARTIFACT_BYTES,
  );
  const codeRequestProjection = boundedTail(
    codeRequests, SESSION_CODE_REQUEST_LIMIT, SESSION_CODE_REQUEST_BYTES,
  );
  const proposalProjection = boundedTail(
    proposals, SESSION_PROPOSAL_LIMIT, SESSION_PROPOSAL_BYTES,
  );
  const effectProjection = boundedTail(effects, SESSION_EFFECT_LIMIT, SESSION_EFFECT_BYTES);
  const localProjections = {
    messages: messageProjection,
    turn_states: turnStateProjection,
    jobs: jobProjection,
    artifacts: artifactProjection,
    code_requests: codeRequestProjection,
    proposals: proposalProjection,
    effects: effectProjection,
  };
  const surfaceTruncation: JsonObject = {};
  for (const [surface, projection] of Object.entries(localProjections)) {
    const upstream = upstreamTruncation[surface];
    if (!upstream) return null;
    surfaceTruncation[surface] = {
      total: upstream.total,
      visible: projection.items.length,
      truncated: upstream.truncated === true || projection.truncated,
    };
  }
  const projected: RuntimeSessionSnapshot = {
    schema_version: 'apocv4.workspace-session-snapshot.v1',
    session_id: expectedSessionId,
    title: value.title,
    created_at: value.created_at,
    updated_at: value.updated_at,
    event_count: value.event_count,
    events_truncated: value.events_truncated
      || Object.values(upstreamTruncation).some((entry) => entry.truncated === true)
      || messageProjection.truncated
      || turnStateProjection.truncated
      || jobProjection.truncated
      || artifactProjection.truncated
      || codeRequestProjection.truncated
      || proposalProjection.truncated
      || effectProjection.truncated,
    tip_digest: value.tip_digest,
    messages: messageProjection.items,
    turn_states: turnStateProjection.items,
    jobs: jobProjection.items,
    artifacts: artifactProjection.items,
    code_requests: codeRequestProjection.items,
    proposals: proposalProjection.items,
    effects: effectProjection.items,
    surface_truncation: surfaceTruncation,
    world,
    workspace: value.workspace,
  };
  return boundedJsonValue(projected, SESSION_PROJECTION_LIMIT) ? projected : null;
}

function validateV2ChatIdentity(value: unknown): value is RuntimeChatIdentity {
  return isObject(value)
    && exactKeys(value, [
      'schema_version', 'system_id', 'architecture', 'compiler_version',
      'identity_digest', 'learned_model_role', 'lineage',
    ])
    && value.schema_version === 'apocv4.identity.v1'
    && value.system_id === 'apocrypha'
    && value.architecture === 'governed_hybrid_digital_intelligence'
    && boundedCanonicalString(value.compiler_version, 128)
    && typeof value.identity_digest === 'string'
    && SHA256_RE.test(value.identity_digest)
    && value.learned_model_role === 'replaceable_faculty_not_system_identity'
    && boundedCanonicalString(value.lineage, 128);
}

function validateV2ChatContext(value: unknown): value is RuntimeChatContext {
  if (!isObject(value) || !exactKeys(value, [
    'frame_id', 'frame_digest', 'provenance_spine_digest',
    'retrieval', 'memory', 'capabilities',
  ])) return false;
  const retrieval = value.retrieval;
  const memory = value.memory;
  const capabilities = value.capabilities;
  const baseMemoryKeys = ['provider', 'status', 'records_used', 'receipt_digest', 'refs'];
  const ownerMemoryKeys = [
    ...baseMemoryKeys,
    'project_id', 'queries', 'owner_profile_status', 'owner_profile_record_digests',
  ];
  const ownerMemory = isObject(memory) && exactKeys(memory, ownerMemoryKeys);
  return boundedCanonicalString(value.frame_id, 256)
    && value.frame_id.startsWith('acf-')
    && typeof value.frame_digest === 'string'
    && SHA256_RE.test(value.frame_digest)
    && typeof value.provenance_spine_digest === 'string'
    && SHA256_RE.test(value.provenance_spine_digest)
    && isObject(retrieval)
    && exactKeys(retrieval, ['status', 'count', 'refs'])
    && boundedCanonicalString(retrieval.status, 128)
    && Number.isInteger(retrieval.count)
    && Number(retrieval.count) >= 0
    && Number(retrieval.count) <= 128
    && Array.isArray(retrieval.refs)
    && retrieval.refs.length <= 128
    && boundedJsonValue(retrieval.refs, 64 * 1024)
    && isObject(memory)
    && (exactKeys(memory, baseMemoryKeys) || ownerMemory)
    && boundedCanonicalString(memory.provider, 128)
    && boundedCanonicalString(memory.status, 128)
    && Number.isInteger(memory.records_used)
    && Number(memory.records_used) >= 0
    && Number(memory.records_used) <= 128
    && (
      memory.receipt_digest === null
      || (typeof memory.receipt_digest === 'string' && SHA256_RE.test(memory.receipt_digest))
    )
    && Array.isArray(memory.refs)
    && memory.refs.length <= 128
    && boundedJsonValue(memory.refs, 64 * 1024)
    && (!ownerMemory || (
      memory.project_id === 'apocv4-owner'
      && isObject(memory.queries)
      && boundedJsonValue(memory.queries, 64 * 1024)
      && boundedCanonicalString(memory.owner_profile_status, 128)
      && Array.isArray(memory.owner_profile_record_digests)
      && memory.owner_profile_record_digests.length <= 16
      && memory.owner_profile_record_digests.every(
        (digest) => typeof digest === 'string' && SHA256_RE.test(digest),
      )
    ))
    && Array.isArray(capabilities)
    && capabilities.length <= 64
    && capabilities.every((entry) => isObject(entry)
      && exactKeys(entry, ['id', 'status', 'authority', 'evidence'])
      && boundedCanonicalString(entry.id, 128)
      && boundedCanonicalString(entry.status, 128)
      && boundedCanonicalString(entry.authority, 128)
      && boundedCanonicalString(entry.evidence, 512));
}

function canonicalRuntimeOrigin(raw: string | undefined): string {
  if (!raw || raw !== raw.trim() || raw.endsWith('/')) {
    throw new RuntimeProxyError('runtime_configuration_invalid', 503);
  }
  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch {
    throw new RuntimeProxyError('runtime_configuration_invalid', 503);
  }
  const expectedIp = process.env.APOCV4_RUNTIME_DIRECT_IP?.trim();
  const expectedPort = process.env.APOCV4_RUNTIME_DIRECT_PORT?.trim();
  const transport = process.env.APOCV4_RUNTIME_TRANSPORT?.trim();
  const testTransport = transport === 'test-fetch' && process.env.NODE_ENV !== 'production';
  if (
    transport !== 'direct-tls'
    && !testTransport
  ) {
    throw new RuntimeProxyError('runtime_configuration_invalid', 503);
  }
  if (
    parsed.protocol !== 'https:'
    || parsed.username !== ''
    || parsed.password !== ''
    || !expectedIp
    || isIP(expectedIp) !== 4
    || parsed.hostname !== expectedIp
    || !expectedPort
    || !PORT_RE.test(expectedPort)
    || parsed.port !== expectedPort
    || parsed.pathname !== '/'
    || parsed.search !== ''
    || parsed.hash !== ''
    || parsed.origin !== raw
  ) {
    throw new RuntimeProxyError('runtime_configuration_invalid', 503);
  }
  return parsed.origin;
}

function runtimeCa(): Buffer {
  const encoded = process.env.APOCV4_RUNTIME_CA_B64?.trim();
  if (!encoded || encoded.length > 32_768 || !BASE64_RE.test(encoded)) {
    throw new RuntimeProxyError('runtime_configuration_invalid', 503);
  }
  const certificate = Buffer.from(encoded, 'base64');
  const pem = certificate.toString('utf8');
  if (
    certificate.length < 256
    || certificate.length > 24_576
    || !pem.startsWith('-----BEGIN CERTIFICATE-----\n')
    || !pem.endsWith('-----END CERTIFICATE-----\n')
  ) {
    throw new RuntimeProxyError('runtime_configuration_invalid', 503);
  }
  return certificate;
}

async function directTlsRequest(
  url: string,
  init: RequestInit,
  maximumBytes: number,
  deadlineMs: number,
): Promise<Response> {
  const parsed = new URL(url);
  const headers = Object.fromEntries(new Headers(init.headers).entries());
  const body = typeof init.body === 'string' ? Buffer.from(init.body, 'utf8') : null;
  return new Promise<Response>((resolve, reject) => {
    let settled = false;
    const fail = (error: Error): void => {
      if (settled) return;
      settled = true;
      reject(error);
    };
    const request = httpsRequest({
      protocol: 'https:',
      hostname: parsed.hostname,
      port: Number(parsed.port),
      path: `${parsed.pathname}${parsed.search}`,
      method: init.method,
      headers,
      ca: runtimeCa(),
      rejectUnauthorized: true,
      timeout: deadlineMs,
    }, (incoming) => {
      const chunks: Buffer[] = [];
      let total = 0;
      incoming.on('data', (chunk: Buffer | string) => {
        const bytes = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
        total += bytes.length;
        if (total > maximumBytes) {
          incoming.destroy();
          fail(new RuntimeProxyError('runtime_response_too_large', 502, incoming.statusCode ?? null));
          return;
        }
        chunks.push(bytes);
      });
      incoming.on('error', fail);
      incoming.on('end', () => {
        if (settled) return;
        const responseHeaders = new Headers();
        for (const [name, value] of Object.entries(incoming.headers)) {
          if (Array.isArray(value)) {
            for (const member of value) responseHeaders.append(name, member);
          } else if (value !== undefined) {
            responseHeaders.set(name, String(value));
          }
        }
        settled = true;
        resolve(new Response(Buffer.concat(chunks, total), {
          status: incoming.statusCode ?? 502,
          headers: responseHeaders,
        }));
      });
    });
    request.once('timeout', () => {
      request.destroy();
      fail(new RuntimeProxyError('runtime_deadline_exceeded', 504, null, deadlineMs));
    });
    request.once('error', (error) => {
      fail(error instanceof RuntimeProxyError
        ? error
        : new RuntimeProxyError('runtime_unreachable', 502));
    });
    if (init.signal) {
      const abort = (): void => {
        request.destroy();
        fail(new RuntimeProxyError('runtime_deadline_exceeded', 504, null, deadlineMs));
      };
      if (init.signal.aborted) abort();
      else init.signal.addEventListener('abort', abort, { once: true });
    }
    if (body) request.write(body);
    request.end();
  });
}

async function runtimeRequest(
  url: string,
  init: RequestInit,
  maximumBytes: number,
  deadlineMs: number,
): Promise<Response> {
  if (
    process.env.APOCV4_RUNTIME_TRANSPORT === 'test-fetch'
    && process.env.NODE_ENV !== 'production'
  ) {
    return fetch(url, init);
  }
  return directTlsRequest(url, init, maximumBytes, deadlineMs);
}

export function validateRuntimeUrl(raw: string): string {
  return canonicalRuntimeOrigin(raw);
}

function runtimeToken(profile: RuntimeCredentialProfile = 'owner'): string {
  const token = profile === 'public'
    ? process.env.APOCV4_PUBLIC_API_TOKEN
    : process.env.APOCV4_API_TOKEN;
  if (!token || !TOKEN_RE.test(token)) {
    throw new RuntimeProxyError('runtime_credential_unavailable', 503);
  }
  return token;
}

function boundedHeaderRef(headers: Headers, name: string): string | null {
  const value = headers.get(name);
  return value && SHA256_RE.test(value) ? value : null;
}

function boundedMode(headers: Headers): string | null {
  const value = headers.get('x-apocv4-auth-mode');
  return value && /^[A-Z_]{1,64}$/.test(value) ? value : null;
}

async function readBoundedJson(
  response: Response,
  maximumBytes: number,
  protectedValues: readonly string[],
): Promise<JsonObject> {
  const contentType = response.headers.get('content-type')?.split(';', 1)[0]?.trim().toLowerCase();
  if (contentType !== 'application/json') {
    throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
  }
  const contentEncoding = response.headers.get('content-encoding');
  if (contentEncoding && contentEncoding.toLowerCase() !== 'identity') {
    throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
  }
  const declared = response.headers.get('content-length');
  if (declared !== null) {
    if (!/^(0|[1-9][0-9]*)$/.test(declared) || Number(declared) > maximumBytes) {
      throw new RuntimeProxyError('runtime_response_too_large', 502, response.status);
    }
  }
  if (!response.body) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  try {
    while (true) {
      const chunk = await reader.read();
      if (chunk.done) break;
      total += chunk.value.byteLength;
      if (total > maximumBytes) {
        await reader.cancel();
        throw new RuntimeProxyError('runtime_response_too_large', 502, response.status);
      }
      chunks.push(chunk.value);
    }
  } finally {
    reader.releaseLock();
  }
  if (declared !== null && total !== Number(declared)) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
  }
  const joined = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    joined.set(chunk, offset);
    offset += chunk.byteLength;
  }
  let text: string;
  try {
    text = new TextDecoder('utf-8', { fatal: true }).decode(joined);
  } catch {
    throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
  }
  if (protectedValues.some((value) => value.length > 0 && text.includes(value))) {
    throw new RuntimeProxyError('runtime_reflected_credential', 502, response.status);
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
  }
  if (!isObject(parsed) || parsed.schema_version !== RUNTIME_SCHEMA) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
  }
  return parsed;
}

async function callRuntime(
  path: RuntimePath,
  body: JsonObject | null,
  traceparent?: string,
  credentialProfile: RuntimeCredentialProfile = 'owner',
): Promise<RuntimeCall> {
  const origin = canonicalRuntimeOrigin(process.env.APOCV4_RUNTIME_URL);
  const token = runtimeToken(credentialProfile);
  const objective = path === '/v1/objectives';
  const chat = path === '/v1/chat';
  const code = path === '/v1/code';
  const rollback = path === '/v1/code/rollback';
  const session = path.startsWith('/v1/sessions/');
  const job = path.startsWith('/v1/jobs/');
  const vision = path === '/v1/vision';
  const deadlineMs = code
    ? RUNPOD_CODE_DEADLINE_MS
    : rollback
      ? ROLLBACK_DEADLINE_MS
      : objective
        ? RUNPOD_SYNC_DEADLINE_MS
        : chat
          ? CHAT_DEADLINE_MS
          : session
            ? SESSION_DEADLINE_MS
            : job
              ? JOB_DEADLINE_MS
              : vision
                ? RUNPOD_SYNC_DEADLINE_MS
            : HEALTH_DEADLINE_MS;
  const responseLimit = code
    ? CODE_RESPONSE_LIMIT
    : rollback
      ? ROLLBACK_RESPONSE_LIMIT
      : objective
        ? OBJECTIVE_RESPONSE_LIMIT
        : chat
          ? CHAT_RESPONSE_LIMIT
          : session
            ? SESSION_RESPONSE_LIMIT
            : job
              ? JOB_RESPONSE_LIMIT
              : vision
                ? VISION_RESPONSE_LIMIT
            : HEALTH_RESPONSE_LIMIT;
  const controller = new AbortController();
  const deadline = setTimeout(() => controller.abort(), deadlineMs);
  const started = Date.now();
  const traceMatch = traceparent ? TRACEPARENT_RE.exec(traceparent) : null;
  const transmittedBody = body === null ? null : withSessionBinding(path, body);
  const bindingMac = transmittedBody === null || typeof transmittedBody.session_binding_mac !== 'string'
    ? null
    : transmittedBody.session_binding_mac;
  const bindingSecret = bindingMac === null ? null : process.env.APOCV4_SESSION_BINDING_SECRET ?? null;
  try {
    const response = await runtimeRequest(`${origin}${path}`, {
      method: transmittedBody === null ? 'GET' : 'POST',
      headers: {
        Accept: 'application/json',
        'Accept-Encoding': 'identity',
        Authorization: `Bearer ${token}`,
        ...(traceMatch ? {
          Traceparent: traceparent,
          'X-Apocky-Trace-Id': traceMatch[1],
        } : {}),
        ...(transmittedBody === null ? {} : { 'Content-Type': 'application/json' }),
      },
      body: transmittedBody === null ? undefined : JSON.stringify(transmittedBody),
      cache: 'no-store',
      redirect: 'error',
      signal: controller.signal,
    }, responseLimit, deadlineMs);
    const data = await readBoundedJson(
      response,
      responseLimit,
      bindingMac === null ? [token] : [token, bindingMac, bindingSecret ?? ''],
    );
    if (!response.ok) {
      if (
        response.status === 404
        && exactKeys(data, ['schema_version', 'error'])
        && data.error === 'session_not_found'
      ) {
        throw new RuntimeProxyError('session_not_found', 404, response.status);
      }
      throw new RuntimeProxyError('runtime_http_error', 502, response.status);
    }
    if (
      bindingMac !== null
      && response.headers.get('x-apocv4-session-binding') !== 'VERIFIED'
    ) {
      throw new RuntimeProxyError('runtime_session_binding_invalid', 502, response.status);
    }
    const expectedKeys = objective || chat || code || rollback || session || job || vision
      ? ['schema_version', 'result']
      : ['schema_version', 'status', 'engine', 'vision'];
    if (!exactKeys(data, expectedKeys)) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
    }
    if (
      ((objective || chat || code || rollback || session || job || vision) && !isObject(data.result))
      || (!objective && !chat && !code && !rollback && !session && !job && !vision
        && (data.status !== 'READY' || !isObject(data.engine) || typeof data.vision !== 'boolean'))
    ) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
    }
    return {
      data,
      rollback_lease_ref: boundedHeaderRef(response.headers, 'x-apocv4-rollback-lease-ref'),
      receipt: {
        observed_at: new Date().toISOString(),
        latency_ms: Math.max(0, Date.now() - started),
        upstream_status: response.status,
        auth_mode: boundedMode(response.headers),
        auth_registry_ref: boundedHeaderRef(response.headers, 'x-apocv4-auth-registry-ref'),
        binding_ref: boundedHeaderRef(response.headers, 'x-apocv4-binding-ref'),
        principal_ref: boundedHeaderRef(response.headers, 'x-apocv4-principal-ref'),
        privacy_partition_ref: boundedHeaderRef(response.headers, 'x-apocv4-privacy-partition-ref'),
        effect_scope_ref: boundedHeaderRef(response.headers, 'x-apocv4-effect-scope-ref'),
      },
    };
  } catch (error) {
    if (error instanceof RuntimeProxyError) throw error;
    const timedOut = controller.signal.aborted;
    throw new RuntimeProxyError(
      timedOut ? 'runtime_deadline_exceeded' : 'runtime_unreachable',
      timedOut ? 504 : 502,
      null,
      timedOut ? deadlineMs : null,
    );
  } finally {
    clearTimeout(deadline);
  }
}

function selected(source: JsonObject, keys: readonly string[]): JsonObject {
  const result: JsonObject = {};
  for (const key of keys) {
    if (Object.hasOwn(source, key)) result[key] = source[key];
  }
  return result;
}

function requireStrictEffectReceipt(receipt: RuntimeReceipt, requireEffectScope: boolean): void {
  if (
    receipt.auth_mode !== 'STRICT_REGISTRY'
    || !receipt.auth_registry_ref
    || !receipt.binding_ref
    || !receipt.principal_ref
    || !receipt.privacy_partition_ref
    || (requireEffectScope && !receipt.effect_scope_ref)
  ) {
    throw new RuntimeProxyError('runtime_effect_attestation_invalid', 502, receipt.upstream_status);
  }
}

export async function fetchRuntimeHealth(traceparent?: string): Promise<RuntimeHealthProjection> {
  const call = await callRuntime('/health', null, traceparent);
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'health',
    observed: {
      evidence_lane: 'observed_runtime_http',
      receipt: call.receipt,
      runtime: call.data,
    },
    model_reported: {
      evidence_lane: 'model_reported',
      present: false,
      note: 'Health contains runtime observations only; it is not a model reasoning claim.',
    },
  };
}

export async function submitRuntimeObjective(objective: string, traceparent?: string): Promise<RuntimeObjectiveProjection> {
  if (
    typeof objective !== 'string'
    || objective !== objective.trim()
    || objective.length < 1
    || objective.length > 16_384
  ) {
    throw new RuntimeProxyError('objective_invalid', 400);
  }
  const call = await callRuntime('/v1/objectives', { max_iterations: 1, objective }, traceparent);
  const runtimeResult = call.data.result;
  if (!isObject(runtimeResult)) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  const rawAttempts = Array.isArray(runtimeResult.attempts)
    ? runtimeResult.attempts.filter(isObject).slice(0, 100)
    : [];
  const observedAttempts = rawAttempts.map((attempt) => selected(attempt, [
    'sequence',
    'kind',
    'cycle_status',
    'test_run_digest',
    'test_passed',
    'failed_oracles',
    'evidence_spine_digest',
    'error_class',
    'error_digest',
    'attempt_digest',
  ]));
  const modelAttempts = rawAttempts.map((attempt) => selected(attempt, [
    'sequence',
    'active_model_id',
    'candidate_digest',
    'model_route_digest',
    'model_attempts_digest',
    'model_transitions_digest',
    'council_decision',
    'council_decision_digest',
  ]));
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'objective',
    observed: {
      evidence_lane: 'observed_runtime_http_and_test_receipts',
      receipt: call.receipt,
      runtime: selected(runtimeResult, [
        'schema_version',
        'status',
        'terminal_reason',
        'max_iterations',
        'iterations_completed',
        'accepted_candidate_digest',
        'last_test_run_digest',
        'checkpoint_digest',
        'perception_frame_digest',
        'vision_observation_digests',
        'faculty_team_id',
      ]),
      attempts: observedAttempts,
    },
    model_reported: {
      evidence_lane: 'model_reported_not_observed_fact',
      note: 'Faculty and council fields are model-reported outputs, not hidden chain-of-thought or independently observed facts.',
      attempts: modelAttempts,
    },
  };
}

export async function submitRuntimeChat(
  input: RuntimeChatInput,
  traceparent?: string,
): Promise<RuntimeChatProjection> {
  const {
    message, conversationId, requestId, sessionId, sessionPrincipal, privacyPartition,
    credentialProfile = 'owner',
  } = input;
  if (
    typeof message !== 'string'
    || message !== message.trim()
    || Buffer.byteLength(message, 'utf8') < 1
    || Buffer.byteLength(message, 'utf8') > 16_384
    || !UUID_RE.test(conversationId)
    || !UUID_RE.test(requestId)
    || ((sessionId === undefined) !== (sessionPrincipal === undefined))
    || (sessionId !== undefined && !UUID_RE.test(sessionId))
    || (sessionPrincipal !== undefined && !isRuntimeSessionPrincipal(sessionPrincipal))
    || typeof privacyPartition !== 'string'
    || privacyPartition !== privacyPartition.trim()
    || privacyPartition.length < 1
    || privacyPartition.length > 256
    || (credentialProfile !== 'owner' && credentialProfile !== 'public')
  ) {
    throw new RuntimeProxyError('chat_request_invalid', 400);
  }
  if (
    (credentialProfile === 'owner' && privacyPartition !== 'owner:apocky')
    || (credentialProfile === 'public' && privacyPartition !== 'public:apocrypha')
  ) {
    throw new RuntimeProxyError('chat_request_invalid', 400);
  }
  const call = await callRuntime('/v1/chat', {
    message,
    conversation_id: conversationId,
    request_id: requestId,
    ...(sessionId === undefined ? {} : {
      session_id: sessionId,
      session_principal: sessionPrincipal,
    }),
    privacy_partition: privacyPartition,
  }, traceparent, credentialProfile);
  const result = call.data.result;
  if (!isObject(result)) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  const responseV2 = result.schema_version === 'apocv4.chat-response.v2';
  const expectedKeys = responseV2
    ? [
      'schema_version', 'text', 'model_reported', 'observed', 'authority',
      'identity', 'context', 'conversation_id', 'request_id', 'privacy_partition_ref',
      'outcome', 'learned_faculty_used', 'duplicate_effect_protection',
    ]
    : [
      'schema_version', 'text', 'model_reported', 'observed', 'authority',
      'conversation_id', 'request_id', 'privacy_partition_ref', 'outcome',
      'learned_faculty_used', 'duplicate_effect_protection',
    ];
  if (!exactKeys(result, expectedKeys)) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  const model = result.model_reported;
  const observed = result.observed;
  const authority = result.authority;
  if (
    (result.schema_version !== 'apocv4.chat-response.v1' && !responseV2)
    || result.conversation_id !== conversationId
    || result.request_id !== requestId
    || typeof result.privacy_partition_ref !== 'string'
    || !SHA256_RE.test(result.privacy_partition_ref)
    || result.outcome !== 'completed'
    || result.learned_faculty_used !== true
    || result.duplicate_effect_protection !== 'not_applicable_no_effect_authority'
    || !isObject(model)
    || model.evidence_lane !== 'model_reported_not_observed_fact'
    || typeof result.text !== 'string'
    || result.text !== result.text.trim()
    || Buffer.byteLength(result.text, 'utf8') < 1
    || Buffer.byteLength(result.text, 'utf8') > 128 * 1024
    || !isObject(observed)
    || observed.evidence_lane !== 'observed_runtime_transport'
    || !isObject(authority)
    || !exactKeys(authority, [
      'effect_authority', 'tool_authority', 'memory_scope',
      'conversation_history', 'training_consent',
    ])
    || authority.effect_authority !== 'NONE'
    || authority.training_consent !== false
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  if (responseV2) {
    const expectedMemoryScope = credentialProfile === 'owner'
      ? 'owner_partitioned_retrieval'
      : 'public_safe_retrieval';
    if (
      authority.tool_authority !== 'READ_ONLY_CONTEXT'
      || authority.memory_scope !== expectedMemoryScope
      || authority.conversation_history !== (
        sessionId === undefined ? 'session_bounded' : 'durable_principal_bound'
      )
      || !validateV2ChatIdentity(result.identity)
      || !validateV2ChatContext(result.context)
    ) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
    }
  } else if (
    authority.tool_authority !== 'NONE'
    || authority.memory_scope !== 'ephemeral'
    || authority.conversation_history !== 'not_retained'
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  for (const key of ['model_id', 'model_revision', 'model_family', 'response_id'] as const) {
    if (typeof model[key] !== 'string' || model[key] !== model[key].trim() || model[key].length < 1) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
    }
  }
  for (const key of ['serving_profile_digest', 'prompt_digest', 'response_digest'] as const) {
    if (typeof model[key] !== 'string' || !SHA256_RE.test(model[key])) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
    }
  }
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'chat',
    observed: {
      evidence_lane: 'observed_runtime_http_and_transport',
      receipt: call.receipt,
      runtime: {
        schema_version: result.schema_version,
        conversation_id: result.conversation_id,
        request_id: result.request_id,
        privacy_partition_ref: result.privacy_partition_ref,
        outcome: result.outcome,
        learned_faculty_used: result.learned_faculty_used,
        duplicate_effect_protection: result.duplicate_effect_protection,
        authority,
        ...(responseV2 ? { identity: result.identity, context: result.context } : {}),
        observed,
      },
    },
    model_reported: {
      ...model,
      evidence_lane: 'model_reported_not_observed_fact',
      text: result.text,
    },
    authority: authority as unknown as RuntimeChatAuthority,
    identity: responseV2 ? result.identity as RuntimeChatIdentity : null,
    context: responseV2 ? result.context as RuntimeChatContext : null,
  };
}

function sessionObservation(
  receipt: RuntimeReceipt,
  requestContract: RuntimeSessionObservation['request_contract'] = 'not_applicable',
): RuntimeSessionObservation {
  return {
    evidence_lane: 'observed_runtime_http_and_principal_bound_session',
    receipt,
    request_contract: requestContract,
  };
}

export async function listRuntimeSessions(
  input: RuntimeSessionListInput,
  traceparent?: string,
): Promise<RuntimeSessionListProjection> {
  validateSessionBinding(input);
  const limit = input.limit ?? 24;
  if (!Number.isInteger(limit) || limit < 1 || limit > 128) {
    throw new RuntimeProxyError('session_request_invalid', 400);
  }
  const call = await callRuntime('/v1/sessions/list', {
    privacy_partition: input.privacyPartition,
    session_principal: input.sessionPrincipal,
    limit,
  }, traceparent, input.credentialProfile ?? 'owner');
  const result = call.data.result;
  const listKeys = ['schema_version', 'sessions', 'count'];
  if (
    !isObject(result)
    || !exactKeys(result, listKeys)
    || result.schema_version !== 'apocv4.workspace-sessions.v1'
    || !Array.isArray(result.sessions)
    || result.sessions.length > limit
    || !boundedNonnegativeInteger(result.count, 128)
    || result.count !== result.sessions.length
    || !boundedJsonValue(result, SESSION_RESPONSE_LIMIT)
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  const sessions = result.sessions.map(normalizeSessionSummary);
  if (
    sessions.some((entry) => entry === null)
    || new Set(sessions.map((entry) => entry?.session_id)).size !== sessions.length
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'session_list',
    observed: sessionObservation(call.receipt),
    sessions: sessions as RuntimeSessionSummary[],
    count: result.count as number,
  };
}

export async function getRuntimeSession(
  input: RuntimeSessionGetInput,
  traceparent?: string,
): Promise<RuntimeSessionGetProjection> {
  validateSessionBinding(input);
  if (!CLIENT_SESSION_UUID_RE.test(input.sessionId)) {
    throw new RuntimeProxyError('session_request_invalid', 400);
  }
  const credentialProfile = input.credentialProfile ?? 'owner';
  const call = await callRuntime('/v1/sessions/get', {
    privacy_partition: input.privacyPartition,
    session_principal: input.sessionPrincipal,
    session_id: input.sessionId,
  }, traceparent, credentialProfile);
  const session = normalizeSessionSnapshot(call.data.result, input.sessionId, credentialProfile);
  if (!session) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'session_get',
    observed: sessionObservation(call.receipt, 'session_id'),
    session,
  };
}

export async function deleteRuntimeSession(
  input: RuntimeSessionDeleteInput,
  traceparent?: string,
): Promise<RuntimeSessionDeleteProjection> {
  validateSessionBinding(input);
  if (!CLIENT_SESSION_UUID_RE.test(input.sessionId) || !UUID_RE.test(input.requestId)) {
    throw new RuntimeProxyError('session_request_invalid', 400);
  }
  const call = await callRuntime('/v1/sessions/delete', {
    privacy_partition: input.privacyPartition,
    session_principal: input.sessionPrincipal,
    request_id: input.requestId,
    session_id: input.sessionId,
  }, traceparent, input.credentialProfile ?? 'owner');
  const result = call.data.result;
  if (
    !isObject(result)
    || !exactKeys(result, ['schema_version', 'session_id', 'deleted', 'event_digest'])
    || result.schema_version !== 'apocv4.workspace-session-deletion.v1'
    || result.session_id !== input.sessionId
    || result.deleted !== true
    || typeof result.event_digest !== 'string'
    || !SHA256_RE.test(result.event_digest)
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'session_delete',
    observed: sessionObservation(call.receipt, 'session_id'),
    session_id: input.sessionId,
    deleted: true,
    event_digest: result.event_digest,
  };
}

function canonicalCodePaths(value: unknown): string[] {
  if (!Array.isArray(value) || value.length < 1 || value.length > 32) {
    throw new RuntimeProxyError('code_request_invalid', 400);
  }
  const paths: string[] = [];
  for (const member of value) {
    if (
      typeof member !== 'string'
      || member !== member.trim()
      || member.length < 1
      || member.length > 4_096
      || !CODE_PATH_RE.test(member)
    ) {
      throw new RuntimeProxyError('code_request_invalid', 400);
    }
    const parts = member.split('/');
    const firstPart = parts[0] ?? '';
    const folded = member.toLowerCase();
    const leaf = (parts.at(-1) ?? '').toLowerCase();
    if (
      parts.some((part) => !part || part === '.' || part === '..')
      || firstPart.toLowerCase() === '.git'
      || parts.some((part) => (
        part.trimEnd() !== part
        || part.endsWith('.')
        || part.includes(':')
        || WINDOWS_RESERVED_PATH_STEMS.has((part.split('.', 1)[0] ?? '').toUpperCase())
      ))
      || PROTECTED_CODE_PATHS.has(member)
      || PROTECTED_CODE_PREFIXES.some((prefix) => member.startsWith(prefix))
      || leaf === 'env.txt'
      || leaf === '.env'
      || leaf.startsWith('.env.')
      || /\.(?:pem|key|p12|pfx|crt|cer)$/.test(leaf)
      || /(?:^|[._-])(?:secret|credential|private[-_]?key)(?:[._-]|$)/.test(leaf)
      || folded.includes('/.git/')
    ) {
      throw new RuntimeProxyError('code_request_invalid', 400);
    }
    paths.push(member);
  }
  const sorted = [...paths].sort();
  if (
    new Set(paths).size !== paths.length
    || new Set(paths.map((path) => path.toLowerCase())).size !== paths.length
    || paths.some((path, index) => path !== sorted[index])
  ) {
    throw new RuntimeProxyError('code_request_invalid', 400);
  }
  return paths;
}

export function validateRuntimeCodePaths(value: unknown): string[] {
  return canonicalCodePaths(value);
}

interface DurableEffectBinding {
  sessionId: string;
  sessionPrincipal: RuntimeSessionPrincipal;
  requestId: string;
}

function durableEffectBinding(
  input: {
    sessionId?: string;
    sessionPrincipal?: RuntimeSessionPrincipal;
    requestId?: string;
    privacyPartition: string;
  },
  errorCode: 'code_request_invalid' | 'rollback_request_invalid',
): DurableEffectBinding | null {
  const values = [input.sessionId, input.sessionPrincipal, input.requestId];
  const present = values.filter((value) => value !== undefined).length;
  if (present === 0) return null;
  if (
    present !== values.length
    || typeof input.sessionId !== 'string'
    || !CLIENT_SESSION_UUID_RE.test(input.sessionId)
    || !isRuntimeSessionPrincipal(input.sessionPrincipal)
    || typeof input.requestId !== 'string'
    || !UUID_RE.test(input.requestId)
    || input.privacyPartition !== 'owner:apocky'
  ) {
    throw new RuntimeProxyError(errorCode, 400);
  }
  return {
    sessionId: input.sessionId,
    sessionPrincipal: input.sessionPrincipal,
    requestId: input.requestId,
  };
}

function projectDurableCodeReceipt(
  result: JsonObject,
  binding: DurableEffectBinding | null,
): JsonObject | null {
  const fields = [
    'session_id', 'request_id', 'session_event_digests', 'session_tip_digest',
    'durable_replay',
  ];
  if (binding === null) {
    return fields.some((key) => Object.hasOwn(result, key)) ? null : {};
  }
  const events = result.session_event_digests;
  if (
    result.session_id !== binding.sessionId
    || result.request_id !== binding.requestId
    || !isObject(events)
    || !exactKeys(events, ['code_request', 'code_proposal', 'code_effect', 'rollback'])
    || typeof events.code_request !== 'string'
    || !SHA256_RE.test(events.code_request)
    || typeof events.code_proposal !== 'string'
    || !SHA256_RE.test(events.code_proposal)
    || typeof events.code_effect !== 'string'
    || !SHA256_RE.test(events.code_effect)
    || !nullableDigest(events.rollback)
    || typeof result.session_tip_digest !== 'string'
    || !SHA256_RE.test(result.session_tip_digest)
    || typeof result.durable_replay !== 'boolean'
    || (result.state === 'EXECUTION_ROLLED_BACK') !== (events.rollback !== null)
  ) return null;
  return {
    session_id: binding.sessionId,
    request_id: binding.requestId,
    session_event_digests: {
      code_request: events.code_request,
      code_proposal: events.code_proposal,
      code_effect: events.code_effect,
      rollback: events.rollback,
    },
    session_tip_digest: result.session_tip_digest,
    durable_replay: result.durable_replay,
  };
}

function projectDurableRollbackReceipt(
  result: JsonObject,
  binding: DurableEffectBinding | null,
): JsonObject | null {
  const fields = [
    'session_id', 'request_id', 'session_event_digests', 'session_tip_digest',
    'durable_replay',
  ];
  if (binding === null) {
    return fields.some((key) => Object.hasOwn(result, key)) ? null : {};
  }
  const events = result.session_event_digests;
  if (
    result.session_id !== binding.sessionId
    || result.request_id !== binding.requestId
    || !isObject(events)
    || !exactKeys(events, ['rollback'])
    || typeof events.rollback !== 'string'
    || !SHA256_RE.test(events.rollback)
    || typeof result.session_tip_digest !== 'string'
    || !SHA256_RE.test(result.session_tip_digest)
    || typeof result.durable_replay !== 'boolean'
  ) return null;
  return {
    session_id: binding.sessionId,
    request_id: binding.requestId,
    session_event_digests: { rollback: events.rollback },
    session_tip_digest: result.session_tip_digest,
    durable_replay: result.durable_replay,
  };
}

function digestValue(value: unknown): string | null {
  return typeof value === 'string' && SHA256_RE.test(value) ? value : null;
}

function boundedOptionalString(value: unknown, maximum = 256): string | null {
  return typeof value === 'string'
    && value === value.trim()
    && value.length > 0
    && value.length <= maximum
    ? value
    : null;
}

function projectFacultyAttempts(value: unknown): JsonObject[] {
  if (!Array.isArray(value) || value.length > 16) {
    throw new RuntimeProxyError('runtime_response_invalid', 502);
  }
  return value.map((member) => {
    if (!isObject(member)) throw new RuntimeProxyError('runtime_response_invalid', 502);
    const index = member.index;
    const identity = digestValue(member.faculty_identity_digest);
    const status = member.status;
    const proposalDigest = member.proposal_digest === null
      ? null
      : digestValue(member.proposal_digest);
    const errorClass = member.error_class === null
      ? null
      : boundedOptionalString(member.error_class);
    const errorDigest = member.error_digest === null
      ? null
      : digestValue(member.error_digest);
    if (
      !Number.isSafeInteger(index)
      || Number(index) < 0
      || !identity
      || !['ACCEPTED_FOR_ADMISSION', 'FAILED'].includes(String(status))
      || (member.proposal_digest !== null && !proposalDigest)
      || (member.error_class !== null && !errorClass)
      || (member.error_digest !== null && !errorDigest)
    ) {
      throw new RuntimeProxyError('runtime_response_invalid', 502);
    }
    return {
      index: Number(index),
      faculty_identity_digest: identity,
      status,
      proposal_digest: proposalDigest,
      error_class: errorClass,
      error_digest: errorDigest,
    };
  });
}

function projectTestReceipt(value: unknown): JsonObject | null {
  if (value === null || value === undefined) return null;
  if (!isObject(value)) throw new RuntimeProxyError('runtime_response_invalid', 502);
  const commandDigest = digestValue(value.command_digest);
  const contractDigest = digestValue(value.runner_contract_digest);
  const stdoutDigest = digestValue(value.stdout_sha256);
  const stderrDigest = digestValue(value.stderr_sha256);
  const receiptDigest = digestValue(value.receipt_digest);
  if (
    !commandDigest
    || !contractDigest
    || !stdoutDigest
    || !stderrDigest
    || !receiptDigest
    || !Number.isSafeInteger(value.exit_code)
    || typeof value.timed_out !== 'boolean'
    || typeof value.passed !== 'boolean'
    || !Number.isSafeInteger(value.stdout_bytes)
    || Number(value.stdout_bytes) < 0
    || !Number.isSafeInteger(value.stderr_bytes)
    || Number(value.stderr_bytes) < 0
    || typeof value.elapsed_ms !== 'number'
    || !Number.isFinite(value.elapsed_ms)
    || Number(value.elapsed_ms) < 0
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502);
  }
  const errorClass = value.error_class === null
    ? null
    : boundedOptionalString(value.error_class);
  if (value.error_class !== null && !errorClass) {
    throw new RuntimeProxyError('runtime_response_invalid', 502);
  }
  return {
    command_digest: commandDigest,
    runner_contract_digest: contractDigest,
    exit_code: Number(value.exit_code),
    timed_out: value.timed_out,
    error_class: errorClass,
    stdout_sha256: stdoutDigest,
    stdout_bytes: Number(value.stdout_bytes),
    stderr_sha256: stderrDigest,
    stderr_bytes: Number(value.stderr_bytes),
    elapsed_ms: Number(value.elapsed_ms),
    passed: value.passed,
    receipt_digest: receiptDigest,
  };
}

function normalizeRuntimeJob(value: unknown): JsonObject {
  if (
    !isObject(value)
    || !JOB_ID_RE.test(String(value.job_id ?? ''))
    || typeof value.action_id !== 'string'
    || !/^[a-z][a-z0-9_.:-]{0,127}$/.test(value.action_id)
    || !['QUEUED', 'RUNNING', 'CANCEL_REQUESTED', 'SUCCEEDED', 'FAILED', 'CANCELLED', 'INTERRUPTED_REVIEW_REQUIRED'].includes(String(value.state))
    || !SHA256_RE.test(String(value.request_digest ?? ''))
    || !SHA256_RE.test(String(value.action_manifest_digest ?? ''))
    || !boundedNonnegativeInteger(value.attempt, 1_000)
    || !boundedJsonValue(value, 128 * 1024)
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502);
  }
  return { ...value };
}

function validateRuntimeJobBinding(input: RuntimeSessionGetInput): void {
  validateSessionBinding(input);
  if (!CLIENT_SESSION_UUID_RE.test(input.sessionId)) {
    throw new RuntimeProxyError('job_request_invalid', 400);
  }
}

export async function submitRuntimeBackgroundJob(
  input: RuntimeJobSubmitInput,
  traceparent?: string,
): Promise<RuntimeJobProjection> {
  validateRuntimeJobBinding(input);
  const objective = input.objective;
  const maxIterations = input.maxIterations ?? 1;
  if (
    typeof objective !== 'string'
    || objective !== objective.trim()
    || Buffer.byteLength(objective, 'utf8') < 1
    || Buffer.byteLength(objective, 'utf8') > 16_384
    || !UUID_RE.test(input.requestId)
    || !Number.isInteger(maxIterations)
    || maxIterations < 1
    || maxIterations > 8
  ) {
    throw new RuntimeProxyError('job_request_invalid', 400);
  }
  const call = await callRuntime('/v1/jobs/submit', {
    privacy_partition: input.privacyPartition,
    session_principal: input.sessionPrincipal,
    session_id: input.sessionId,
    request_id: input.requestId,
    action_id: 'objective.proposal_council.v1',
    arguments: { objective, max_iterations: maxIterations },
  }, traceparent, input.credentialProfile ?? 'owner');
  const result = call.data.result;
  if (
    !isObject(result)
    || !exactKeys(result, ['schema_version', 'job'])
    || result.schema_version !== 'apocv4.background-job.v1'
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'job',
    observed: sessionObservation(call.receipt, 'session_id'),
    job: normalizeRuntimeJob(result.job),
  };
}

export async function listRuntimeBackgroundJobs(
  input: RuntimeSessionGetInput,
  traceparent?: string,
): Promise<RuntimeJobListProjection> {
  validateRuntimeJobBinding(input);
  const call = await callRuntime('/v1/jobs/list', {
    privacy_partition: input.privacyPartition,
    session_principal: input.sessionPrincipal,
    session_id: input.sessionId,
  }, traceparent, input.credentialProfile ?? 'owner');
  const result = call.data.result;
  if (
    !isObject(result)
    || !exactKeys(result, ['schema_version', 'jobs', 'count'])
    || result.schema_version !== 'apocv4.background-jobs.v1'
    || !Array.isArray(result.jobs)
    || !boundedNonnegativeInteger(result.count, 4_096)
    || result.count !== result.jobs.length
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'job_list',
    observed: sessionObservation(call.receipt, 'session_id'),
    jobs: result.jobs.map(normalizeRuntimeJob),
    count: result.count as number,
  };
}

export async function getRuntimeBackgroundJob(
  input: RuntimeJobStatusInput,
  traceparent?: string,
): Promise<RuntimeJobProjection> {
  validateRuntimeJobBinding(input);
  if (!JOB_ID_RE.test(input.jobId)) {
    throw new RuntimeProxyError('job_request_invalid', 400);
  }
  const call = await callRuntime('/v1/jobs/status', {
    privacy_partition: input.privacyPartition,
    session_principal: input.sessionPrincipal,
    session_id: input.sessionId,
    job_id: input.jobId,
  }, traceparent, input.credentialProfile ?? 'owner');
  const result = call.data.result;
  if (
    !isObject(result)
    || !exactKeys(result, ['schema_version', 'job'])
    || result.schema_version !== 'apocv4.background-job.v1'
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'job',
    observed: sessionObservation(call.receipt, 'session_id'),
    job: normalizeRuntimeJob(result.job),
  };
}

export async function cancelRuntimeBackgroundJob(
  input: RuntimeJobCancelInput,
  traceparent?: string,
): Promise<RuntimeJobProjection> {
  validateRuntimeJobBinding(input);
  if (!JOB_ID_RE.test(input.jobId) || !UUID_RE.test(input.requestId)) {
    throw new RuntimeProxyError('job_request_invalid', 400);
  }
  const call = await callRuntime('/v1/jobs/cancel', {
    privacy_partition: input.privacyPartition,
    session_principal: input.sessionPrincipal,
    session_id: input.sessionId,
    request_id: input.requestId,
    job_id: input.jobId,
  }, traceparent, input.credentialProfile ?? 'owner');
  const result = call.data.result;
  if (
    !isObject(result)
    || !exactKeys(result, ['schema_version', 'job'])
    || result.schema_version !== 'apocv4.background-job.v1'
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'job',
    observed: sessionObservation(call.receipt, 'session_id'),
    job: normalizeRuntimeJob(result.job),
  };
}

export async function submitRuntimeVision(
  input: RuntimeVisionInput,
  traceparent?: string,
): Promise<RuntimeVisionProjection> {
  const observedAt = Date.parse(input.observedAt);
  if (
    typeof input.imageB64 !== 'string'
    || input.imageB64.length < 1
    || input.imageB64.length > 5_600_000
    || !BASE64_RE.test(input.imageB64)
    || !['image/jpeg', 'image/png', 'image/webp'].includes(input.mimeType)
    || !Number.isFinite(observedAt)
    || new Date(observedAt).toISOString() !== input.observedAt
    || !UUID_RE.test(input.perceptId)
    || !boundedCanonicalString(input.provenanceRef, 512)
    || !boundedCanonicalString(input.question, 32_768)
    || !boundedCanonicalString(input.privacyPartition, 256)
    || (input.credentialProfile !== undefined && input.credentialProfile !== 'owner')
  ) {
    throw new RuntimeProxyError('vision_request_invalid', 400);
  }
  const call = await callRuntime('/v1/vision', {
    image_b64: input.imageB64,
    mime_type: input.mimeType,
    observed_at: input.observedAt,
    percept_id: input.perceptId,
    privacy_partition: input.privacyPartition,
    provenance_ref: input.provenanceRef,
    question: input.question,
  }, traceparent, 'owner');
  const result = call.data.result;
  const observation = isObject(result) ? result.observation : null;
  const runtimeState = isObject(result) ? result.runtime_state : null;
  const observationKeys = [
    'schema_version', 'summary', 'entities', 'visible_text', 'spatial_relations',
    'affordances', 'uncertainties', 'safety_notes', 'confidence', 'percept_id',
    'percept_digest', 'model_id', 'model_revision', 'model_family',
    'serving_profile_digest', 'prompt_digest', 'response_digest', 'latency_ms',
    'evidence_lane', 'effect_authority', 'observation_digest',
  ];
  if (
    !isObject(result)
    || !exactKeys(result, ['observation', 'perception_digest', 'runtime_state'])
    || !isObject(observation)
    || !exactKeys(observation, observationKeys)
    || observation.schema_version !== 'apocv4.vision-observation.v1'
    || observation.evidence_lane !== 'reported'
    || observation.effect_authority !== 'NONE'
    || observation.percept_id !== input.perceptId
    || !boundedCanonicalString(observation.summary, 16_384)
    || !SHA256_RE.test(String(observation.percept_digest ?? ''))
    || !SHA256_RE.test(String(observation.observation_digest ?? ''))
    || !SHA256_RE.test(String(result.perception_digest ?? ''))
    || !isObject(runtimeState)
    || !boundedJsonValue(observation, VISION_RESPONSE_LIMIT)
    || !boundedJsonValue(runtimeState, VISION_RESPONSE_LIMIT)
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  for (const key of ['entities', 'visible_text', 'spatial_relations', 'affordances', 'uncertainties', 'safety_notes']) {
    const value = observation[key];
    if (!Array.isArray(value) || value.length > 128 || value.some((item) => typeof item !== 'string')) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
    }
  }
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'vision',
    observed: {
      evidence_lane: 'model_reported_visual_observation_over_observed_runtime_transport',
      receipt: call.receipt,
      perception_digest: String(result.perception_digest),
      observation,
      runtime_state: runtimeState,
    },
  };
}

export async function submitRuntimeCode(
  input: RuntimeCodeInput,
  traceparent?: string,
): Promise<RuntimeCodeProjection> {
  const objective = input.objective;
  const privacyPartition = input.privacyPartition;
  if (
    typeof objective !== 'string'
    || objective !== objective.trim()
    || objective.length < 1
    || Buffer.byteLength(objective, 'utf8') > 32_768
    || typeof privacyPartition !== 'string'
    || privacyPartition !== privacyPartition.trim()
    || privacyPartition.length < 1
    || privacyPartition.length > 256
  ) {
    throw new RuntimeProxyError('code_request_invalid', 400);
  }
  const allowedPaths = canonicalCodePaths(input.allowedPaths);
  const durableBinding = durableEffectBinding(input, 'code_request_invalid');
  const call = await callRuntime('/v1/code', {
    objective,
    privacy_partition: privacyPartition,
    allowed_paths: allowedPaths,
    ...(durableBinding === null ? {} : {
      session_id: durableBinding.sessionId,
      session_principal: durableBinding.sessionPrincipal,
      request_id: durableBinding.requestId,
    }),
  }, traceparent);
  const result = call.data.result;
  if (!isObject(result)) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  if (Object.hasOwn(result, 'ledger_tip_digest')) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  const durableReceipt = projectDurableCodeReceipt(result, durableBinding);
  if (durableReceipt === null) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  const state = result.state;
  const proposalDigest = digestValue(result.proposal_digest);
  const frameDigest = digestValue(result.frame_digest);
  const terminalDigest = digestValue(result.terminal_event_digest);
  const journalDigest = digestValue(result.journal_tip_digest);
  if (
    result.schema_version !== 'apocv4.journaled-patch-runtime.v1'
    || !['PROMOTED', 'PROMOTION_ABORTED', 'EXECUTION_ROLLED_BACK', 'SOURCE_DRIFT_BEFORE_ADMISSION', 'ADMISSION_REFUSED'].includes(String(state))
    || result.requested_objective !== objective
    || result.privacy_partition !== privacyPartition
    || !proposalDigest
    || !frameDigest
    || !terminalDigest
    || !journalDigest
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  requireStrictEffectReceipt(call.receipt, state === 'PROMOTED');
  const attempts = projectFacultyAttempts(result.faculty_attempts);
  const outcome = result.isolated_outcome;
  let projectedOutcome: JsonObject | null = null;
  let test: JsonObject | null = null;
  if (outcome !== undefined) {
    if (!isObject(outcome)) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
    }
    test = projectTestReceipt(outcome.test_receipt);
    const outcomeDigest = digestValue(outcome.outcome_digest);
    const outcomeProposalDigest = digestValue(outcome.proposal_digest);
    const sourceDigest = digestValue(outcome.source_prestate_digest);
    const leaseDigest = digestValue(outcome.lease_digest);
    const deltaDigest = outcome.delta_digest === null ? null : digestValue(outcome.delta_digest);
    const failureDigest = outcome.failure_digest === null ? null : digestValue(outcome.failure_digest);
    const failureClass = outcome.failure_class === null ? null : boundedOptionalString(outcome.failure_class);
    if (
      !outcomeDigest
      || !outcomeProposalDigest
      || outcomeProposalDigest !== proposalDigest
      || !sourceDigest
      || !leaseDigest
      || (outcome.delta_digest !== null && !deltaDigest)
      || (outcome.failure_digest !== null && !failureDigest)
      || (outcome.failure_class !== null && !failureClass)
      || !boundedOptionalString(outcome.state)
    ) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
    }
    projectedOutcome = {
      state: outcome.state,
      source_prestate_digest: sourceDigest,
      lease_digest: leaseDigest,
      delta_digest: deltaDigest,
      failure_class: failureClass,
      failure_digest: failureDigest,
      outcome_digest: outcomeDigest,
    };
  }
  const promotionDigest = result.promotion_event_digest === null || result.promotion_event_digest === undefined
    ? null
    : digestValue(result.promotion_event_digest);
  if ((state === 'PROMOTED' && !promotionDigest) || (state !== 'PROMOTED' && promotionDigest)) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  const authorityDigest = digestValue(result.authority_digest);
  const requestDigest = digestValue(result.request_digest);
  const approvalDigest = digestValue(result.approval_digest);
  const reservationDigest = digestValue(result.reservation_event_digest);
  const preparedDigest = digestValue(result.promotion_prepared_event_digest);
  const admission = isObject(result.admission) ? result.admission : null;
  if (
    state === 'PROMOTED'
    && (
      !projectedOutcome
      || projectedOutcome.state !== 'ACCEPTED_ISOLATED'
      || !projectedOutcome.delta_digest
      || !test
      || test.passed !== true
      || !authorityDigest
      || !requestDigest
      || !approvalDigest
      || !reservationDigest
      || !preparedDigest
      || promotionDigest !== terminalDigest
      || !admission
      || admission.allowed !== true
      || admission.reason_code !== 'admitted'
      || admission.frame_digest !== frameDigest
      || admission.authority_digest !== authorityDigest
      || admission.request_digest !== requestDigest
      || admission.approval_digest !== approvalDigest
    )
  ) {
    throw new RuntimeProxyError('runtime_effect_attestation_invalid', 502, call.receipt.upstream_status);
  }
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'code',
    observed: {
      evidence_lane: 'observed_runtime_http_effect_and_test_receipts',
      receipt: call.receipt,
      runtime: {
        schema_version: result.schema_version,
        state,
        frame_digest: frameDigest,
        authority_digest: authorityDigest,
        request_digest: requestDigest,
        approval_digest: approvalDigest,
        reservation_event_digest: reservationDigest,
        promotion_prepared_event_digest: result.promotion_prepared_event_digest === null
          ? null
          : preparedDigest,
        promotion_event_digest: promotionDigest,
        terminal_event_digest: terminalDigest,
        journal_tip_digest: journalDigest,
        isolated_outcome: projectedOutcome,
        perception_frame_digest: digestValue(result.perception_frame_digest),
        faculty_team_id: boundedOptionalString(result.faculty_team_id),
        ...durableReceipt,
      },
      test,
    },
    generated: {
      evidence_lane: 'model_generated_artifact_identity_not_source_diff',
      proposal_digest: proposalDigest,
      requested_allowed_paths: allowedPaths,
      faculty_attempts: attempts,
    },
  };
}

export async function submitRuntimeRollback(
  input: string | RuntimeRollbackInput,
  traceparent?: string,
): Promise<RuntimeRollbackProjection> {
  const promotionEventDigest = typeof input === 'string'
    ? input
    : input.promotionEventDigest;
  if (!SHA256_RE.test(promotionEventDigest)) {
    throw new RuntimeProxyError('rollback_request_invalid', 400);
  }
  const durableBinding = typeof input === 'string'
    ? null
    : durableEffectBinding({
      sessionId: input.sessionId,
      sessionPrincipal: input.sessionPrincipal,
      requestId: input.requestId,
      privacyPartition: input.privacyPartition ?? '',
    }, 'rollback_request_invalid');
  const call = await callRuntime('/v1/code/rollback', {
    promotion_event_digest: promotionEventDigest,
    ...(durableBinding === null ? {} : {
      session_id: durableBinding.sessionId,
      session_principal: durableBinding.sessionPrincipal,
      request_id: durableBinding.requestId,
    }),
  }, traceparent);
  requireStrictEffectReceipt(call.receipt, false);
  if (!call.rollback_lease_ref) {
    throw new RuntimeProxyError('runtime_effect_attestation_invalid', 502, call.receipt.upstream_status);
  }
  const result = call.data.result;
  const durableReceipt = isObject(result)
    ? projectDurableRollbackReceipt(result, durableBinding)
    : null;
  const expectedKeys = [
    'schema_version', 'state', 'promotion_event_digest', 'rollback_event_digest',
    'journal_tip_digest', 'operation_ref',
    ...(durableBinding === null ? [] : [
      'session_id', 'request_id', 'session_event_digests', 'session_tip_digest',
      'durable_replay',
    ]),
  ];
  if (
    !isObject(result)
    || Object.hasOwn(result, 'ledger_tip_digest')
    || !exactKeys(result, expectedKeys)
    || result.schema_version !== 'apocv4.journaled-patch-runtime.v1'
    || result.state !== 'ROLLED_BACK'
    || result.promotion_event_digest !== promotionEventDigest
    || !digestValue(result.rollback_event_digest)
    || !digestValue(result.journal_tip_digest)
    || !digestValue(result.operation_ref)
    || durableReceipt === null
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'rollback',
    observed: {
      evidence_lane: 'observed_runtime_http_rollback_receipt',
      receipt: call.receipt,
      runtime: {
        schema_version: result.schema_version,
        state: result.state,
        promotion_event_digest: result.promotion_event_digest,
        rollback_event_digest: result.rollback_event_digest,
        journal_tip_digest: result.journal_tip_digest,
        operation_ref: result.operation_ref,
        ...durableReceipt,
      },
    },
    model_reported: {
      evidence_lane: 'model_reported',
      present: false,
      note: 'Rollback is a runtime effect receipt, not a model reasoning claim.',
    },
  };
}

export function publicRuntimeError(error: unknown): {
  error: string;
  upstream_status?: number;
  observed?: {
    evidence_lane: 'observed_runtime_transport_failure';
    receipt: {
      observed_at: string;
      outcome: string;
      upstream_status: number | null;
      deadline_ms: number | null;
    };
  };
} {
  if (!(error instanceof RuntimeProxyError)) return { error: 'runtime_proxy_failure' };
  return {
    error: error.code,
    ...(error.upstreamStatus === null ? {} : { upstream_status: error.upstreamStatus }),
    observed: {
      evidence_lane: 'observed_runtime_transport_failure',
      receipt: {
        observed_at: error.observedAt,
        outcome: error.code,
        upstream_status: error.upstreamStatus,
        deadline_ms: error.deadlineMs,
      },
    },
  };
}
