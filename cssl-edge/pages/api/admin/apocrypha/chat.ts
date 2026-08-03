// One owner-authenticated response-only turn through the direct Apocv4 runtime.

import { createHash } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';

import {
  RuntimeProxyError,
  publicRuntimeError,
  submitRuntimeChat,
} from '@/lib/apocv4/runtime-proxy';
import {
  expectedConversationRef,
  isOpaqueClientRequestId,
  isOpaqueConversationId,
  requireApocryphaOwner,
  scopeConversationId,
  scopeRequestId,
  setPrivateNoStore,
} from '@/lib/apocrypha/proxy';
import { hasSameOrigin } from '@/lib/auth-session';
import { envelope } from '@/lib/response';
import { createServerTrace, emitOperationalTelemetry, traceparentFor } from '@/lib/telemetry/server';

const MAX_TEXT_BYTES = 16_384;
const MAX_RESPONSE_TEXT_BYTES = 128 * 1024;
const TURN_SOURCE_REF = 'public:apocky.com/chat';
const OWNER_PRIVACY_PARTITION = 'owner:apocky';
const OWNER_PRIVACY_PARTITION_REF = createHash('sha256')
  .update(JSON.stringify(OWNER_PRIVACY_PARTITION), 'utf8')
  .digest('hex');
const OWNER_CHAT_RESPONSE_SCHEMA = 'apocky.apocv4-owner-chat.v1';
const RUNTIME_CHAT_RESPONSE_SCHEMA = 'apocv4.chat-response.v1';
const MODEL_EVIDENCE_LANE = 'model_reported_not_observed_fact';
const TRANSPORT_EVIDENCE_LANE = 'observed_runtime_transport';
const DUPLICATE_EFFECT_PROTECTION = 'not_applicable_no_effect_authority';
const SHA256_RE = /^[0-9a-f]{64}$/;

interface TurnBody {
  text?: unknown;
  conversation_id?: unknown;
  request_id?: unknown;
}

type JsonObject = Record<string, unknown>;

function boundedText(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  const text = value.trim();
  if (!text || Buffer.byteLength(text, 'utf8') > MAX_TEXT_BYTES) return null;
  return text;
}

function isObject(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function exactKeys(value: JsonObject, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return actual.length === wanted.length && actual.every((key, index) => key === wanted[index]);
}

function stringField(body: JsonObject, key: string, maximumBytes = 8_192): string | null {
  const value = body[key];
  return typeof value === 'string'
    && value === value.trim()
    && value.length > 0
    && Buffer.byteLength(value, 'utf8') <= maximumBytes
    ? value
    : null;
}

function digestField(body: JsonObject, key: string): string | null {
  const value = body[key];
  return typeof value === 'string' && SHA256_RE.test(value) ? value : null;
}

function nonnegativeInteger(body: JsonObject, key: string): number | null {
  const value = body[key];
  return Number.isSafeInteger(value) && Number(value) >= 0 ? Number(value) : null;
}

function contractFailure(error: unknown): boolean {
  return error instanceof RuntimeProxyError && [
    'runtime_reflected_credential',
    'runtime_response_invalid',
    'runtime_response_too_large',
  ].includes(error.code);
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  const trace = createServerTrace(req);
  const started = performance.now();
  setPrivateNoStore(res);
  res.setHeader('X-Apocky-Trace-Id', trace.traceId);
  res.setHeader('Traceparent', traceparentFor(trace));
  res.setHeader('Allow', 'POST');
  if (req.method !== 'POST') {
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocrypha.owner_chat.denied', source: 'pages.api.admin.apocrypha.chat', plane: 'security',
      severity: 'warn', outcome: 'denied', status: 405, durationMs: Math.round(performance.now() - started),
      message: 'Owner chat method denied.', authority: 'owner-admin-required',
    });
    return;
  }
  if (!hasSameOrigin(req)) {
    res.status(403).json({ error: 'Same-origin request required', ...envelope() });
    await emitOperationalTelemetry({
      trace, kind: 'security.apocrypha.owner_chat.origin_denied', source: 'pages.api.admin.apocrypha.chat', plane: 'security',
      severity: 'warn', outcome: 'denied', status: 403, durationMs: Math.round(performance.now() - started),
      message: 'Owner chat origin denied.', authority: 'same-origin-owner-admin',
    });
    return;
  }

  const owner = await requireApocryphaOwner(req, res);
  if (!owner) {
    await emitOperationalTelemetry({
      trace, kind: 'security.apocrypha.owner_chat.auth_denied', source: 'pages.api.admin.apocrypha.chat', plane: 'security',
      severity: 'warn', outcome: 'denied', status: res.statusCode || 401, durationMs: Math.round(performance.now() - started),
      message: 'Owner chat authorization denied.', authority: 'owner-admin-required',
    });
    return;
  }

  const body = isObject(req.body) ? req.body as TurnBody : {};
  const text = boundedText(body.text);
  if (!text) {
    res.status(400).json({
      error: `text must contain 1-${MAX_TEXT_BYTES} UTF-8 bytes after trimming`,
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocrypha.owner_chat.rejected', source: 'pages.api.admin.apocrypha.chat', plane: 'edge',
      severity: 'warn', outcome: 'denied', status: 400, durationMs: Math.round(performance.now() - started),
      message: 'Owner chat text envelope rejected.', authority: 'owner-admin',
    });
    return;
  }
  if (!isOpaqueConversationId(body.conversation_id)) {
    res.status(400).json({
      error: 'conversation_id must be an opaque UUIDv4 minted by this client',
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocrypha.owner_chat.rejected', source: 'pages.api.admin.apocrypha.chat', plane: 'edge',
      severity: 'warn', outcome: 'denied', status: 400, durationMs: Math.round(performance.now() - started),
      message: 'Owner chat conversation identifier rejected.', authority: 'owner-admin',
    });
    return;
  }
  if (!isOpaqueClientRequestId(body.request_id)) {
    res.status(400).json({
      error: 'request_id must be an opaque UUIDv4 minted once for this client turn',
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocrypha.owner_chat.rejected', source: 'pages.api.admin.apocrypha.chat', plane: 'edge',
      severity: 'warn', outcome: 'denied', status: 400, durationMs: Math.round(performance.now() - started),
      message: 'Owner chat request identifier rejected.', authority: 'owner-admin',
    });
    return;
  }

  const clientConversationId = body.conversation_id.toLowerCase();
  const clientRequestId = body.request_id.toLowerCase();
  const scopedConversationId = scopeConversationId(owner.principalRef, clientConversationId);
  const scopedRequestId = scopeRequestId(owner.principalRef, clientRequestId);
  const requiredConversationRef = expectedConversationRef(scopedConversationId, TURN_SOURCE_REF);
  const baseTelemetry = {
    conversation_ref: requiredConversationRef,
    request_ref: scopedRequestId,
    privacy_partition_ref: OWNER_PRIVACY_PARTITION_REF,
  };

  await emitOperationalTelemetry({
    trace, kind: 'runtime.chat.started', source: 'apocv4-runtime-proxy', plane: 'runtime',
    severity: 'info', outcome: 'started', status: null, durationMs: Math.round(performance.now() - started),
    message: 'Owner response-only chat admitted for direct runtime dispatch.',
    effectClass: 'apocv4.owner.chat.response_only', authority: 'owner-admin-no-tools-no-effects',
    attributes: { ...baseTelemetry, message_bytes: Buffer.byteLength(text, 'utf8'), upstream_stage: 'dispatch' },
  });

  try {
    const upstream = await submitRuntimeChat({
      message: text,
      conversationId: scopedConversationId,
      requestId: scopedRequestId,
      privacyPartition: OWNER_PRIVACY_PARTITION,
    }, traceparentFor(trace));
    const runtime = upstream.observed.runtime;
    const model = upstream.model_reported;
    const authority = isObject(runtime.authority) ? runtime.authority : null;
    const transport = isObject(runtime.observed) ? runtime.observed : null;
    const usage = isObject(model.usage) ? model.usage : null;
    const responseText = stringField(model, 'text', MAX_RESPONSE_TEXT_BYTES);
    const modelId = stringField(model, 'model_id');
    const modelRevision = stringField(model, 'model_revision');
    const modelFamily = stringField(model, 'model_family');
    const responseId = stringField(model, 'response_id');
    const servingProfileDigest = digestField(model, 'serving_profile_digest');
    const promptDigest = digestField(model, 'prompt_digest');
    const responseDigest = digestField(model, 'response_digest');
    const rationalePresent = model.rationale_present;
    const rationaleDigest = model.rationale_digest;
    const promptTokens = usage ? nonnegativeInteger(usage, 'prompt_tokens') : null;
    const completionTokens = usage ? nonnegativeInteger(usage, 'completion_tokens') : null;
    const transportKind = transport ? stringField(transport, 'transport_kind') : null;
    const transportReceiptDigest = transport?.transport_receipt_digest;
    const validTransportReceiptDigest = transportReceiptDigest === null
      || (typeof transportReceiptDigest === 'string' && SHA256_RE.test(transportReceiptDigest));
    const projectedTransportReceiptDigest = typeof transportReceiptDigest === 'string'
      ? transportReceiptDigest
      : null;
    const modelLatencyMs = transport?.latency_ms;
    const runtimePrivacyPartitionRef = digestField(runtime, 'privacy_partition_ref');
    const committedEnvelope = Boolean(
      exactKeys(runtime, [
        'schema_version', 'conversation_id', 'request_id', 'privacy_partition_ref', 'outcome',
        'learned_faculty_used', 'duplicate_effect_protection', 'authority', 'observed',
      ])
      && runtime.schema_version === RUNTIME_CHAT_RESPONSE_SCHEMA
      && runtime.conversation_id === scopedConversationId
      && runtime.request_id === scopedRequestId
      && runtimePrivacyPartitionRef === OWNER_PRIVACY_PARTITION_REF
      && runtime.outcome === 'completed'
      && runtime.learned_faculty_used === true
      && runtime.duplicate_effect_protection === DUPLICATE_EFFECT_PROTECTION
      && authority
      && exactKeys(authority, [
        'effect_authority', 'tool_authority', 'memory_scope', 'conversation_history', 'training_consent',
      ])
      && authority.effect_authority === 'NONE'
      && authority.tool_authority === 'NONE'
      && authority.memory_scope === 'ephemeral'
      && authority.conversation_history === 'not_retained'
      && authority.training_consent === false
      && exactKeys(model, [
        'evidence_lane', 'model_id', 'model_revision', 'model_family', 'serving_profile_digest',
        'response_id', 'prompt_digest', 'response_digest', 'rationale_present', 'rationale_digest',
        'usage', 'text',
      ])
      && model.evidence_lane === MODEL_EVIDENCE_LANE
      && responseText
      && modelId
      && modelRevision
      && modelFamily
      && responseId
      && servingProfileDigest
      && promptDigest
      && responseDigest
      && typeof rationalePresent === 'boolean'
      && ((rationalePresent && typeof rationaleDigest === 'string' && SHA256_RE.test(rationaleDigest))
        || (!rationalePresent && rationaleDigest === null))
      && usage
      && exactKeys(usage, ['prompt_tokens', 'completion_tokens'])
      && promptTokens !== null
      && completionTokens !== null
      && transport
      && exactKeys(transport, [
        'evidence_lane', 'latency_ms', 'transport_kind', 'transport_receipt_digest',
      ])
      && transport.evidence_lane === TRANSPORT_EVIDENCE_LANE
      && typeof modelLatencyMs === 'number'
      && Number.isFinite(modelLatencyMs)
      && modelLatencyMs >= 0
      && transportKind
      && validTransportReceiptDigest,
    );
    if (!committedEnvelope) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, upstream.observed.receipt.upstream_status);
    }

    res.status(200).json({
      schema_version: OWNER_CHAT_RESPONSE_SCHEMA,
      text: responseText,
      text_evidence_lane: MODEL_EVIDENCE_LANE,
      conversation_id: clientConversationId,
      conversation_ref: requiredConversationRef,
      request_id: clientRequestId,
      request_ref: scopedRequestId,
      outcome: 'completed',
      learned_faculty_used: true,
      duplicate_effect_protection: DUPLICATE_EFFECT_PROTECTION,
      effect_authority: 'NONE',
      tool_authority: 'NONE',
      memory_scope: 'ephemeral',
      conversation_history: 'not_retained_by_public_interface',
      training_consent: false,
      model_id: modelId,
      response_id: responseId,
      response_digest: responseDigest,
      serving_profile_digest: servingProfileDigest,
      authority: {
        effect_authority: 'NONE',
        tool_authority: 'NONE',
        memory_scope: 'ephemeral',
        conversation_history: 'not_retained',
        training_consent: false,
      },
      observed: {
        evidence_lane: upstream.observed.evidence_lane,
        upstream_status: upstream.observed.receipt.upstream_status,
        edge_latency_ms: upstream.observed.receipt.latency_ms,
        runtime_latency_ms: modelLatencyMs,
        transport_kind: transportKind,
        transport_receipt_digest: projectedTransportReceiptDigest,
      },
      model_reported: {
        evidence_lane: MODEL_EVIDENCE_LANE,
        model_id: modelId,
        model_revision: modelRevision,
        model_family: modelFamily,
        serving_profile_digest: servingProfileDigest,
        response_id: responseId,
        prompt_digest: promptDigest,
        response_digest: responseDigest,
        rationale_present: rationalePresent,
        rationale_digest: rationaleDigest,
        usage: { prompt_tokens: promptTokens, completion_tokens: completionTokens },
      },
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'runtime.chat.completed', source: 'apocv4-runtime-proxy', plane: 'runtime',
      severity: 'info', outcome: 'succeeded', status: 200,
      durationMs: Math.round(performance.now() - started),
      message: 'Owner response-only runtime chat completed with exact evidence projection.',
      effectClass: 'apocv4.owner.chat.response_only', authority: 'owner-admin-no-tools-no-effects',
      receiptRef: projectedTransportReceiptDigest ?? upstream.observed.receipt.binding_ref,
      attributes: {
        ...baseTelemetry,
        auth_partition_ref: upstream.observed.receipt.privacy_partition_ref,
        model_id: modelId,
        model_revision: modelRevision,
        model_family: modelFamily,
        serving_profile_digest: servingProfileDigest,
        prompt_digest: promptDigest,
        response_digest: responseDigest,
        prompt_tokens: promptTokens,
        completion_tokens: completionTokens,
        edge_latency_ms: upstream.observed.receipt.latency_ms,
        runtime_latency_ms: modelLatencyMs,
        transport_kind: transportKind,
        transport_receipt_digest: projectedTransportReceiptDigest,
        effect_authority: 'NONE',
        tool_authority: 'NONE',
        upstream_status: upstream.observed.receipt.upstream_status,
        upstream_stage: 'response_projected',
      },
    });
  } catch (error) {
    const rejected = contractFailure(error);
    const status = error instanceof RuntimeProxyError ? error.publicStatus : 502;
    res.status(status).json({
      ...publicRuntimeError(error),
      conversation_id: clientConversationId,
      request_id: clientRequestId,
      duplicate_effect_protection: DUPLICATE_EFFECT_PROTECTION,
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace,
      kind: rejected ? 'runtime.chat.contract_rejected' : 'runtime.chat.failed',
      source: 'apocv4-runtime-proxy',
      plane: 'runtime',
      severity: 'error',
      outcome: 'failed',
      status,
      durationMs: Math.round(performance.now() - started),
      message: error instanceof RuntimeProxyError ? error.code : 'runtime_proxy_failure',
      effectClass: 'apocv4.owner.chat.response_only',
      authority: 'owner-admin-no-tools-no-effects',
      attributes: {
        ...baseTelemetry,
        effect_authority: 'NONE',
        tool_authority: 'NONE',
        upstream_status: error instanceof RuntimeProxyError ? error.upstreamStatus : null,
        upstream_stage: rejected ? 'response_contract' : 'transport',
      },
    });
  }
}
