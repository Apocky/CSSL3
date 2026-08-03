import Link from 'next/link';
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent as ReactKeyboardEvent,
} from 'react';

import { authFetch } from '@/lib/browser-auth';
import { useSiteSession } from '@/components/hub/SiteSession';
import CyberDreamField from '@/components/cyber/CyberDreamField';
import styles from '@/styles/PublicApocrypha.module.css';

type MessageRole = 'user' | 'apocrypha';

interface TurnReceipt {
  modelId: string;
  responseId: string;
  responseDigest: string;
  servingProfileDigest: string;
  memoryScope: string;
  conversationHistory: string;
  identity: IdentityReceipt | null;
  context: ContextReceipt | null;
}

interface IdentityReceipt {
  schema_version: string;
  system_id: string;
  architecture: string;
  compiler_version: string;
  identity_digest: string;
  learned_model_role: string;
  lineage: string;
}

interface CapabilityReceipt {
  id: string;
  status: string;
  authority: string;
  evidence: string;
}

interface ContextReceipt {
  frame_id: string;
  frame_digest: string;
  provenance_spine_digest: string;
  retrieval: { status: string; count: number; refs: unknown[] };
  memory: { provider: string; status: string; records_used: number; receipt_digest: string | null; refs: unknown[] };
  capabilities: CapabilityReceipt[];
}

interface ChatMessage {
  id: string;
  role: MessageRole;
  text: string;
  requestId?: string;
  recordedAt?: string;
  receipt?: TurnReceipt;
  codeEffect?: CodeEffectReceipt;
  turnState?: DurableTurnState;
}

interface CodeEffectReceipt {
  state: string;
  allowedPaths: string[];
  proposalDigest: string | null;
  requestContractDigest?: string | null;
  sessionEventDigest?: string | null;
  promotionEventDigest: string | null;
  terminalEventDigest: string | null;
  testPassed: boolean | null;
  testState?: string | null;
  testExitCode: string | null;
  latencyMs: string;
  rollbackEventDigest?: string | null;
  durableReplay?: boolean;
  settledEffectCount?: number;
}

interface DurableTurnState {
  request_id: string;
  state: 'PENDING' | 'FAILED';
  recorded_at: string;
  user_event_digest: string;
  terminal_event_digest: string | null;
  error_class?: string;
  error_digest?: string;
  failure_code?: string;
  rejected_result_digest?: string;
}

interface DurableCodeRequest {
  request_id: string;
  objective: string;
  objective_digest: string;
  allowed_paths: string[];
  allowed_paths_digest: string;
  request_contract_digest: string;
  recorded_at: string;
  event_digest: string;
}

interface DurableCodeProposal {
  request_id: string;
  proposal_digest: string;
  allowed_paths: string[];
  state: string;
  runtime_state: string;
  test_state: string;
  recorded_at: string;
  event_digest: string;
}

interface DurableCodeEffect {
  request_id: string;
  proposal_digest: string | null;
  state: string;
  promotion_event_digest: string | null;
  terminal_event_digest: string | null;
  rollback_event_digest: string | null;
  changed_paths: string[];
  test_state: string;
  scope?: string;
  recorded_at: string;
  event_digest: string;
}

interface SurfaceTruncation {
  total: number;
  visible: number;
  truncated: boolean;
}

interface DurableWorldState {
  message_count: number;
  pending_turn_count: number;
  failed_turn_count: number;
  active_job_count: number;
  artifact_count: number;
  code_request_count: number;
  proposal_count: number;
  effect_count: number;
  last_event_type: string;
  last_event_digest: string;
}

interface PendingTurn {
  messageId: string;
  requestId: string;
  text: string;
}

interface CodeApproval {
  binding: string;
  authGeneration: number;
  subjectKey: string;
  conversationId: string;
  objective: string;
  allowedPaths: string[];
}

type InteractionSurface =
  | { kind: 'intent'; x: number; y: number; placement: 'above' | 'point' }
  | { kind: 'conversation'; x: number; y: number }
  | { kind: 'message'; messageId: string; x: number; y: number }
  | { kind: 'scope'; x: number; y: number }
  | null;

interface TurnResponse {
  text?: unknown;
  error?: unknown;
  conversation_id?: unknown;
  request_id?: unknown;
  model_id?: unknown;
  response_id?: unknown;
  response_digest?: unknown;
  serving_profile_digest?: unknown;
  effect_authority?: unknown;
  tool_authority?: unknown;
  outcome?: unknown;
  learned_faculty_used?: unknown;
  memory_scope?: unknown;
  conversation_history?: unknown;
  training_consent?: unknown;
  duplicate_effect_protection?: unknown;
  retry_after_seconds?: unknown;
  identity?: unknown;
  context?: unknown;
}

interface CodeResponse {
  kind?: unknown;
  error?: unknown;
  observed?: unknown;
  generated?: unknown;
}

interface RollbackResponse {
  kind?: unknown;
  error?: unknown;
  observed?: unknown;
}

interface DurableSessionSummary {
  session_id: string;
  title: string;
  updated_at: string;
  message_count: number;
  active_job_count: number;
}

interface DurableSessionSnapshot {
  session_id: string;
  events_truncated: boolean;
  messages: Array<{
    role: 'user' | 'assistant';
    content: string;
    request_id: string;
    recorded_at: string;
    event_digest: string;
    receipt?: TurnReceipt;
  }>;
  turn_states: DurableTurnState[];
  jobs: Array<Record<string, unknown>>;
  artifacts: Array<Record<string, unknown>>;
  code_requests: DurableCodeRequest[];
  proposals: DurableCodeProposal[];
  effects: DurableCodeEffect[];
  surface_truncation: Record<string, SurfaceTruncation>;
  world: DurableWorldState;
}

type GenerativeMode = 'general' | 'code' | 'analyze' | 'write' | 'explain';

const GENERATIVE_MODES: ReadonlyArray<{
  id: GenerativeMode;
  label: string;
  verb: string;
  icon: string;
  description: string;
  frame: string;
  typedRoute?: string;
  starter: string;
  placeholder: string;
  dispatch: string;
}> = [
  { id: 'general', label: 'Open Field', verb: 'Explore', icon: '✦', description: 'Let the problem reveal its shape through conversation', frame: 'leaves your request open without adding a lead instruction', starter: '', placeholder: 'Bring Apocrypha a question, tension, or unfinished thought…', dispatch: 'Enter' },
  { id: 'analyze', label: 'Prism', verb: 'Interrogate', icon: '◇', description: 'Split claims into evidence, tensions, and falsifiers', frame: 'adds a rigorous-analysis instruction to the request', starter: 'Analyze this rigorously:\n', placeholder: 'Place a claim, system, or decision under the prism…', dispatch: 'Refract' },
  { id: 'write', label: 'Atelier', verb: 'Shape', icon: '✎', description: 'Compose language, structure, rhythm, and voice', frame: 'adds a drafting instruction to the request', starter: 'Draft this for me:\n', placeholder: 'Describe what the atelier should shape…', dispatch: 'Shape' },
  { id: 'explain', label: 'Lantern', verb: 'Reveal', icon: '◉', description: 'Make hidden structure visible, one layer at a time', frame: 'adds a clear-and-precise explanation instruction to the request', starter: 'Explain this clearly and precisely:\n', placeholder: 'What should the lantern make visible?…', dispatch: 'Illuminate' },
  { id: 'code', label: 'Forge', verb: 'Make', icon: '⌘', description: 'Turn intent into a governed, testable change', frame: 'adds an implementation instruction to the request', typedRoute: 'Owner effects cross the typed governed code route after the airlock.', starter: 'Help me implement this:\n', placeholder: 'Describe what the forge should build or repair…', dispatch: 'Temper' },
];

const CHAT_BROWSER_DEADLINE_MS = 85_000;
const CODE_BROWSER_DEADLINE_MS = 250_000;
const MAX_TEXT_BYTES = 16_384;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const CLIENT_UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const DURABLE_UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[45][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const ACTIVE_SESSION_KEY = 'apocky.apocrypha.active-session.v1';
const SNAPSHOT_POLL_INTERVAL_MS = 2_500;
const SNAPSHOT_POLL_LIMIT = 24;
const EMPTY_WORLD_STATE: DurableWorldState = {
  message_count: 0,
  pending_turn_count: 0,
  failed_turn_count: 0,
  active_job_count: 0,
  artifact_count: 0,
  code_request_count: 0,
  proposal_count: 0,
  effect_count: 0,
  last_event_type: 'NONE',
  last_event_digest: '0'.repeat(64),
};

function stringValue(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function byteLength(value: string): number {
  return new TextEncoder().encode(value).byteLength;
}

function safeError(body: TurnResponse, status: number): string {
  return stringValue(body.error)
    ?? `Apocrypha could not complete this turn (HTTP ${status}).`;
}

function recordValue(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function digestValue(value: unknown): string | null {
  return typeof value === 'string' && SHA256_PATTERN.test(value) ? value : null;
}

function timestampValue(value: unknown): string | null {
  return typeof value === 'string' && !Number.isNaN(Date.parse(value)) ? value : null;
}

function nonnegativeInteger(value: unknown): number | null {
  return Number.isInteger(value) && Number(value) >= 0 ? Number(value) : null;
}

function stringArray(value: unknown, maximum = 32): string[] | null {
  if (!Array.isArray(value) || value.length > maximum) return null;
  const items = value.filter((item): item is string => typeof item === 'string' && item.length > 0);
  return items.length === value.length ? items : null;
}

function nullableDigest(value: unknown): string | null | undefined {
  return value === null || value === undefined ? null : digestValue(value) ?? undefined;
}

function durableSessionSummary(value: unknown): DurableSessionSummary | null {
  const record = recordValue(value);
  if (
    !record
    || typeof record.session_id !== 'string'
    || !CLIENT_UUID_PATTERN.test(record.session_id)
    || !stringValue(record.title)
    || !stringValue(record.updated_at)
    || !Number.isInteger(record.message_count)
    || Number(record.message_count) < 0
    || !Number.isInteger(record.active_job_count)
    || Number(record.active_job_count) < 0
  ) return null;
  return {
    session_id: record.session_id,
    title: String(record.title),
    updated_at: String(record.updated_at),
    message_count: Number(record.message_count),
    active_job_count: Number(record.active_job_count),
  };
}

function durableSessionSnapshot(
  value: unknown,
  expectedSessionId: string,
): DurableSessionSnapshot | null {
  const record = recordValue(value);
  if (
    !record
    || record.session_id !== expectedSessionId
    || typeof record.events_truncated !== 'boolean'
    || !Array.isArray(record.messages)
    || !Array.isArray(record.turn_states)
    || !Array.isArray(record.jobs)
    || !Array.isArray(record.artifacts)
    || !Array.isArray(record.code_requests)
    || !Array.isArray(record.proposals)
    || !Array.isArray(record.effects)
  ) return null;
  const messages: DurableSessionSnapshot['messages'] = [];
  for (const valueMessage of record.messages) {
    const message = recordValue(valueMessage);
    const restoredReceipt = message && Object.hasOwn(message, 'receipt')
      ? durableTurnReceipt(message.receipt)
      : undefined;
    if (
      !message
      || (message.role !== 'user' && message.role !== 'assistant')
      || !stringValue(message.content)
      || typeof message.request_id !== 'string'
      || !DURABLE_UUID_PATTERN.test(message.request_id)
      || !timestampValue(message.recorded_at)
      || typeof message.event_digest !== 'string'
      || !SHA256_PATTERN.test(message.event_digest)
      || (Object.hasOwn(message, 'receipt') && !restoredReceipt)
    ) return null;
    messages.push({
      role: message.role,
      content: String(message.content),
      request_id: message.request_id,
      recorded_at: String(message.recorded_at),
      event_digest: message.event_digest,
      ...(restoredReceipt ? { receipt: restoredReceipt } : {}),
    });
  }

  const turnStates: DurableTurnState[] = [];
  for (const valueState of record.turn_states) {
    const state = recordValue(valueState);
    const terminalDigest = state ? nullableDigest(state.terminal_event_digest) : undefined;
    if (
      !state
      || typeof state.request_id !== 'string'
      || !DURABLE_UUID_PATTERN.test(state.request_id)
      || (state.state !== 'PENDING' && state.state !== 'FAILED')
      || !timestampValue(state.recorded_at)
      || !digestValue(state.user_event_digest)
      || terminalDigest === undefined
      || (state.state === 'FAILED' && terminalDigest === null)
      || (state.error_digest !== undefined && !digestValue(state.error_digest))
      || (state.rejected_result_digest !== undefined && !digestValue(state.rejected_result_digest))
    ) return null;
    turnStates.push({
      request_id: state.request_id,
      state: state.state,
      recorded_at: String(state.recorded_at),
      user_event_digest: String(state.user_event_digest),
      terminal_event_digest: terminalDigest,
      ...(stringValue(state.error_class) ? { error_class: String(state.error_class) } : {}),
      ...(digestValue(state.error_digest) ? { error_digest: String(state.error_digest) } : {}),
      ...(stringValue(state.failure_code) ? { failure_code: String(state.failure_code) } : {}),
      ...(digestValue(state.rejected_result_digest)
        ? { rejected_result_digest: String(state.rejected_result_digest) }
        : {}),
    });
  }

  const codeRequests: DurableCodeRequest[] = [];
  for (const valueRequest of record.code_requests) {
    const request = recordValue(valueRequest);
    const paths = stringArray(request?.allowed_paths);
    if (
      !request
      || typeof request.request_id !== 'string'
      || !DURABLE_UUID_PATTERN.test(request.request_id)
      || !stringValue(request.objective)
      || !digestValue(request.objective_digest)
      || !paths
      || paths.length < 1
      || !digestValue(request.allowed_paths_digest)
      || !digestValue(request.request_contract_digest)
      || !timestampValue(request.recorded_at)
      || !digestValue(request.event_digest)
    ) return null;
    codeRequests.push({
      request_id: request.request_id,
      objective: String(request.objective),
      objective_digest: String(request.objective_digest),
      allowed_paths: paths,
      allowed_paths_digest: String(request.allowed_paths_digest),
      request_contract_digest: String(request.request_contract_digest),
      recorded_at: String(request.recorded_at),
      event_digest: String(request.event_digest),
    });
  }

  const proposals: DurableCodeProposal[] = [];
  for (const valueProposal of record.proposals) {
    const proposal = recordValue(valueProposal);
    const paths = stringArray(proposal?.allowed_paths);
    if (
      !proposal
      || typeof proposal.request_id !== 'string'
      || !DURABLE_UUID_PATTERN.test(proposal.request_id)
      || !digestValue(proposal.proposal_digest)
      || !paths
      || !stringValue(proposal.state)
      || !stringValue(proposal.runtime_state)
      || !stringValue(proposal.test_state)
      || !timestampValue(proposal.recorded_at)
      || !digestValue(proposal.event_digest)
    ) return null;
    proposals.push({
      request_id: proposal.request_id,
      proposal_digest: String(proposal.proposal_digest),
      allowed_paths: paths,
      state: String(proposal.state),
      runtime_state: String(proposal.runtime_state),
      test_state: String(proposal.test_state),
      recorded_at: String(proposal.recorded_at),
      event_digest: String(proposal.event_digest),
    });
  }

  const effects: DurableCodeEffect[] = [];
  for (const valueEffect of record.effects) {
    const effect = recordValue(valueEffect);
    const proposalDigest = effect ? nullableDigest(effect.proposal_digest) : undefined;
    const promotionDigest = effect ? nullableDigest(effect.promotion_event_digest) : undefined;
    const terminalDigest = effect ? nullableDigest(effect.terminal_event_digest) : undefined;
    const rollbackDigest = effect ? nullableDigest(effect.rollback_event_digest) : undefined;
    const paths = stringArray(effect?.changed_paths);
    if (
      !effect
      || typeof effect.request_id !== 'string'
      || !DURABLE_UUID_PATTERN.test(effect.request_id)
      || proposalDigest === undefined
      || promotionDigest === undefined
      || terminalDigest === undefined
      || rollbackDigest === undefined
      || !stringValue(effect.state)
      || !paths
      || !stringValue(effect.test_state)
      || !timestampValue(effect.recorded_at)
      || !digestValue(effect.event_digest)
    ) return null;
    effects.push({
      request_id: effect.request_id,
      proposal_digest: proposalDigest,
      state: String(effect.state),
      promotion_event_digest: promotionDigest,
      terminal_event_digest: terminalDigest,
      rollback_event_digest: rollbackDigest,
      changed_paths: paths,
      test_state: String(effect.test_state),
      ...(stringValue(effect.scope) ? { scope: String(effect.scope) } : {}),
      recorded_at: String(effect.recorded_at),
      event_digest: String(effect.event_digest),
    });
  }

  const surfaceRecord = recordValue(record.surface_truncation);
  const surfaceTruncation: Record<string, SurfaceTruncation> = {};
  if (!surfaceRecord) return null;
  for (const [surface, valueTruncation] of Object.entries(surfaceRecord)) {
    const truncation = recordValue(valueTruncation);
    const total = nonnegativeInteger(truncation?.total);
    const visible = nonnegativeInteger(truncation?.visible);
    if (!truncation || total === null || visible === null || visible > total || typeof truncation.truncated !== 'boolean') {
      return null;
    }
    surfaceTruncation[surface] = { total, visible, truncated: truncation.truncated };
  }
  const world = recordValue(record.world);
  const worldCounts = world && {
    message_count: nonnegativeInteger(world.message_count),
    pending_turn_count: nonnegativeInteger(world.pending_turn_count),
    failed_turn_count: nonnegativeInteger(world.failed_turn_count),
    active_job_count: nonnegativeInteger(world.active_job_count),
    artifact_count: nonnegativeInteger(world.artifact_count),
    code_request_count: nonnegativeInteger(world.code_request_count),
    proposal_count: nonnegativeInteger(world.proposal_count),
    effect_count: nonnegativeInteger(world.effect_count),
  };
  if (
    !world
    || !worldCounts
    || Object.values(worldCounts).some((count) => count === null)
    || !stringValue(world.last_event_type)
    || !digestValue(world.last_event_digest)
    || record.jobs.length > 128
    || record.artifacts.length > 128
    || record.jobs.some((job) => !recordValue(job))
    || record.artifacts.some((artifact) => !recordValue(artifact))
  ) return null;
  return {
    session_id: expectedSessionId,
    events_truncated: record.events_truncated,
    messages,
    turn_states: turnStates,
    jobs: record.jobs as Array<Record<string, unknown>>,
    artifacts: record.artifacts as Array<Record<string, unknown>>,
    code_requests: codeRequests,
    proposals,
    effects,
    surface_truncation: surfaceTruncation,
    world: {
      message_count: worldCounts.message_count as number,
      pending_turn_count: worldCounts.pending_turn_count as number,
      failed_turn_count: worldCounts.failed_turn_count as number,
      active_job_count: worldCounts.active_job_count as number,
      artifact_count: worldCounts.artifact_count as number,
      code_request_count: worldCounts.code_request_count as number,
      proposal_count: worldCounts.proposal_count as number,
      effect_count: worldCounts.effect_count as number,
      last_event_type: String(world.last_event_type),
      last_event_digest: String(world.last_event_digest),
    },
  };
}

function durableTurnReceipt(value: unknown): TurnReceipt | null {
  const record = recordValue(value);
  if (
    !record
    || !stringValue(record.model_id)
    || !stringValue(record.response_id)
    || typeof record.response_digest !== 'string'
    || !SHA256_PATTERN.test(record.response_digest)
    || typeof record.serving_profile_digest !== 'string'
    || !SHA256_PATTERN.test(record.serving_profile_digest)
    || !stringValue(record.memory_scope)
    || record.conversation_history !== 'durable_principal_bound'
  ) return null;
  const identity = identityReceipt(record.identity);
  const context = contextReceipt(record.context);
  if (!identity || !context) return null;
  return {
    modelId: String(record.model_id),
    responseId: String(record.response_id),
    responseDigest: record.response_digest,
    servingProfileDigest: record.serving_profile_digest,
    memoryScope: String(record.memory_scope),
    conversationHistory: String(record.conversation_history),
    identity,
    context,
  };
}

function testResultFromState(state: string): boolean | null {
  if (state.startsWith('PASSED')) return true;
  if (state.startsWith('FAILED')) return false;
  return null;
}

function restoredMessages(snapshot: DurableSessionSnapshot): ChatMessage[] {
  const turnStates = new Map(snapshot.turn_states.map((state) => [state.request_id, state]));
  const timeline: ChatMessage[] = snapshot.messages.map((message) => ({
    id: `history-${message.event_digest}`,
    role: message.role === 'user' ? 'user' : 'apocrypha',
    text: message.content,
    requestId: message.request_id,
    recordedAt: message.recorded_at,
    ...(message.receipt ? { receipt: message.receipt } : {}),
    ...(message.role === 'user' && turnStates.has(message.request_id)
      ? { turnState: turnStates.get(message.request_id) }
      : {}),
  }));
  const proposalsByRequest = new Map(snapshot.proposals.map((proposal) => [proposal.request_id, proposal]));
  const consumedEffects = new Set<string>();
  const promotedMessages = new Map<string, ChatMessage>();

  for (const request of snapshot.code_requests) {
    const proposal = proposalsByRequest.get(request.request_id);
    const requestEffects = snapshot.effects.filter((effect) => effect.request_id === request.request_id);
    requestEffects.forEach((effect) => consumedEffects.add(effect.event_digest));
    const firstEffect = requestEffects[0];
    const latestEffect = requestEffects[requestEffects.length - 1];
    const latestRollback = [...requestEffects].reverse().find((effect) => effect.rollback_event_digest);
    const effectReceipt: CodeEffectReceipt = {
      state: latestEffect?.state ?? 'PENDING',
      allowedPaths: proposal?.allowed_paths ?? request.allowed_paths,
      proposalDigest: proposal?.proposal_digest ?? firstEffect?.proposal_digest ?? null,
      requestContractDigest: request.request_contract_digest,
      sessionEventDigest: latestEffect?.event_digest ?? request.event_digest,
      promotionEventDigest: requestEffects.find((effect) => effect.promotion_event_digest)?.promotion_event_digest ?? null,
      terminalEventDigest: firstEffect?.terminal_event_digest ?? null,
      rollbackEventDigest: latestRollback?.rollback_event_digest ?? null,
      testPassed: testResultFromState(latestEffect?.test_state ?? proposal?.test_state ?? 'NOT_RUN'),
      testState: latestEffect?.test_state ?? proposal?.test_state ?? 'NOT_RUN',
      testExitCode: null,
      latencyMs: 'restored',
    };
    const userMessage: ChatMessage = {
      id: `history-code-request-${request.event_digest}`,
      role: 'user',
      text: request.objective,
      requestId: request.request_id,
      recordedAt: request.recorded_at,
    };
    const effectMessage: ChatMessage = {
      id: `history-code-effect-${latestEffect?.event_digest ?? request.event_digest}`,
      role: 'apocrypha',
      text: codeEffectSummary(effectReceipt),
      requestId: request.request_id,
      recordedAt: latestEffect?.recorded_at ?? request.recorded_at,
      codeEffect: effectReceipt,
    };
    timeline.push(userMessage, effectMessage);
    if (effectReceipt.promotionEventDigest) {
      promotedMessages.set(effectReceipt.promotionEventDigest, effectMessage);
    }
  }

  for (const effect of snapshot.effects) {
    if (consumedEffects.has(effect.event_digest)) continue;
    const promoted = effect.promotion_event_digest
      ? promotedMessages.get(effect.promotion_event_digest)
      : undefined;
    if (promoted?.codeEffect) {
      promoted.codeEffect = {
        ...promoted.codeEffect,
        state: effect.state,
        rollbackEventDigest: effect.rollback_event_digest,
        sessionEventDigest: effect.event_digest,
        testPassed: testResultFromState(effect.test_state),
        testState: effect.test_state,
      };
      promoted.text = codeEffectSummary(promoted.codeEffect);
      promoted.recordedAt = effect.recorded_at;
      continue;
    }
    const standalone: CodeEffectReceipt = {
      state: effect.state,
      allowedPaths: effect.changed_paths,
      proposalDigest: effect.proposal_digest,
      sessionEventDigest: effect.event_digest,
      promotionEventDigest: effect.promotion_event_digest,
      terminalEventDigest: effect.terminal_event_digest,
      rollbackEventDigest: effect.rollback_event_digest,
      testPassed: testResultFromState(effect.test_state),
      testState: effect.test_state,
      testExitCode: null,
      latencyMs: 'restored',
    };
    timeline.push({
      id: `history-code-effect-${effect.event_digest}`,
      role: 'apocrypha',
      text: codeEffectSummary(standalone),
      requestId: effect.request_id,
      recordedAt: effect.recorded_at,
      codeEffect: standalone,
    });
  }

  return timeline.sort((left, right) => (
    Date.parse(left.recordedAt ?? '') - Date.parse(right.recordedAt ?? '')
  ));
}

function parseCodePaths(value: string): string[] {
  return value
    .split(/[\r\n,]+/)
    .map((path) => path.trim())
    .filter(Boolean)
    .sort();
}

function codeApprovalBinding(
  subjectKey: string,
  authGeneration: number,
  conversationId: string,
  objective: string,
  allowedPaths: string[],
): string {
  return JSON.stringify({
    schema: 'apocky.apocrypha.code-approval.v1',
    subject: subjectKey,
    auth_generation: authGeneration,
    conversation_id: conversationId,
    objective: objective.trim(),
    allowed_paths: [...allowedPaths],
  });
}

function codeEffectReceipt(
  value: CodeResponse,
  expectedSessionId: string,
  expectedRequestId: string,
): CodeEffectReceipt | null {
  if (value.kind !== 'code') return null;
  const observed = recordValue(value.observed);
  const generated = recordValue(value.generated);
  const receipt = recordValue(observed?.receipt);
  const runtime = recordValue(observed?.runtime);
  const test = observed?.test === null ? null : recordValue(observed?.test);
  const state = stringValue(runtime?.state);
  const proposalDigest = digestValue(generated?.proposal_digest);
  const sessionDigests = recordValue(runtime?.session_event_digests);
  const requestEventDigest = digestValue(sessionDigests?.code_request);
  const proposalEventDigest = digestValue(sessionDigests?.code_proposal);
  const effectEventDigest = digestValue(sessionDigests?.code_effect);
  const rollbackSessionDigest = sessionDigests?.rollback === null
    ? null
    : digestValue(sessionDigests?.rollback);
  const durableReplay = runtime?.durable_replay;
  const paths = Array.isArray(generated?.requested_allowed_paths)
    ? generated.requested_allowed_paths.filter((path): path is string => typeof path === 'string')
    : [];
  if (
    !receipt
    || !runtime
    || !state
    || !proposalDigest
    || paths.length < 1
    || runtime.session_id !== expectedSessionId
    || runtime.request_id !== expectedRequestId
    || typeof durableReplay !== 'boolean'
    || !sessionDigests
    || !requestEventDigest
    || !proposalEventDigest
    || !effectEventDigest
    || (sessionDigests.rollback !== null && !rollbackSessionDigest)
  ) return null;
  const promotionEventDigest = digestValue(runtime.promotion_event_digest);
  if (state === 'PROMOTED' && !promotionEventDigest) return null;
  return {
    state,
    allowedPaths: paths,
    proposalDigest,
    sessionEventDigest: rollbackSessionDigest ?? effectEventDigest,
    promotionEventDigest,
    terminalEventDigest: digestValue(runtime.terminal_event_digest),
    testPassed: test ? test.passed === true : null,
    testExitCode: test && (typeof test.exit_code === 'string' || typeof test.exit_code === 'number')
      ? String(test.exit_code)
      : null,
    latencyMs: typeof receipt.latency_ms === 'number' ? String(receipt.latency_ms) : '—',
    durableReplay,
    settledEffectCount: rollbackSessionDigest ? 2 : 1,
  };
}

function codeEffectSummary(receipt: CodeEffectReceipt): string {
  const pathSummary = `${receipt.allowedPaths.length} admitted file${receipt.allowedPaths.length === 1 ? '' : 's'}`;
  if (receipt.state === 'PENDING') {
    return `The forge request is durably held at the effect airlock: ${pathSummary}, awaiting a terminal receipt. It has not been represented as complete.`;
  }
  if (receipt.state === 'ROLLED_BACK' || receipt.state === 'EXECUTION_ROLLED_BACK') {
    return `The forge rewound this governed change to its recorded prestate. ${pathSummary} remain linked to the rollback evidence.`;
  }
  const testSummary = receipt.testPassed === true
    ? 'Isolated tests passed.'
    : receipt.testPassed === false
      ? 'Isolated tests failed; promotion was not accepted.'
      : 'No isolated-test result was returned for this terminal state.';
  if (receipt.state === 'PROMOTED') {
    return `The forge crossed the effect airlock and promoted a governed change. ${testSummary} ${pathSummary}.`;
  }
  return `The forge reached ${receipt.state}. ${testSummary} ${pathSummary}.`;
}

function identityReceipt(value: unknown): IdentityReceipt | null {
  const record = recordValue(value);
  if (!record) return null;
  const receipt = {
    schema_version: stringValue(record.schema_version),
    system_id: stringValue(record.system_id),
    architecture: stringValue(record.architecture),
    compiler_version: stringValue(record.compiler_version),
    identity_digest: stringValue(record.identity_digest),
    learned_model_role: stringValue(record.learned_model_role),
    lineage: stringValue(record.lineage),
  };
  return receipt.schema_version === 'apocv4.identity.v1'
    && receipt.system_id === 'apocrypha'
    && receipt.architecture === 'governed_hybrid_digital_intelligence'
    && receipt.identity_digest !== null
    && SHA256_PATTERN.test(receipt.identity_digest)
    && receipt.compiler_version !== null
    && receipt.learned_model_role === 'replaceable_faculty_not_system_identity'
    && receipt.lineage !== null
    ? receipt as IdentityReceipt
    : null;
}

function contextReceipt(value: unknown): ContextReceipt | null {
  const record = recordValue(value);
  const retrieval = recordValue(record?.retrieval);
  const memory = recordValue(record?.memory);
  if (
    !record || !retrieval || !memory
    || !stringValue(record.frame_id)
    || typeof record.frame_digest !== 'string'
    || !SHA256_PATTERN.test(record.frame_digest)
    || typeof record.provenance_spine_digest !== 'string'
    || !SHA256_PATTERN.test(record.provenance_spine_digest)
    || !stringValue(retrieval.status)
    || !Number.isInteger(retrieval.count)
    || !Array.isArray(retrieval.refs)
    || !stringValue(memory.provider)
    || !stringValue(memory.status)
    || !Number.isInteger(memory.records_used)
    || !Array.isArray(memory.refs)
    || !Array.isArray(record.capabilities)
  ) return null;
  const capabilities: CapabilityReceipt[] = [];
  for (const candidate of record.capabilities) {
    const capability = recordValue(candidate);
    if (!capability) return null;
    const id = stringValue(capability.id);
    const status = stringValue(capability.status);
    const authority = stringValue(capability.authority);
    const evidence = stringValue(capability.evidence);
    if (!id || !status || !authority || !evidence) return null;
    capabilities.push({ id, status, authority, evidence });
  }
  const memoryReceipt = memory.receipt_digest;
  if (memoryReceipt !== null && (
    typeof memoryReceipt !== 'string' || !SHA256_PATTERN.test(memoryReceipt)
  )) return null;
  return {
    frame_id: String(record.frame_id),
    frame_digest: record.frame_digest,
    provenance_spine_digest: record.provenance_spine_digest,
    retrieval: {
      status: String(retrieval.status),
      count: Number(retrieval.count),
      refs: retrieval.refs,
    },
    memory: {
      provider: String(memory.provider),
      status: String(memory.status),
      records_used: Number(memory.records_used),
      receipt_digest: memoryReceipt as string | null,
      refs: memory.refs,
    },
    capabilities,
  };
}

function isExactTurn(
  body: TurnResponse,
  conversationId: string,
  requestId: string,
): body is TurnResponse & {
  text: string;
  model_id: string;
  response_id: string;
  response_digest: string;
  serving_profile_digest: string;
} {
  const legacyBoundary = body.tool_authority === 'NONE'
    && body.memory_scope === 'ephemeral'
    && body.conversation_history === 'not_retained_by_public_interface'
    && body.identity === null
    && body.context === null;
  const governedBoundary = body.tool_authority === 'READ_ONLY_CONTEXT'
    && (body.memory_scope === 'owner_partitioned_retrieval'
      || body.memory_scope === 'public_safe_retrieval')
    && (body.conversation_history === 'session_bounded'
      || body.conversation_history === 'durable_principal_bound')
    && identityReceipt(body.identity) !== null
    && contextReceipt(body.context) !== null;
  return body.conversation_id === conversationId
    && body.request_id === requestId
    && Boolean(stringValue(body.text))
    && Boolean(stringValue(body.model_id))
    && Boolean(stringValue(body.response_id))
    && typeof body.response_digest === 'string'
    && SHA256_PATTERN.test(body.response_digest)
    && typeof body.serving_profile_digest === 'string'
    && SHA256_PATTERN.test(body.serving_profile_digest)
    && body.effect_authority === 'NONE'
    && body.outcome === 'completed'
    && body.learned_faculty_used === true
    && (legacyBoundary || governedBoundary)
    && body.training_consent === false
    && body.duplicate_effect_protection === 'not_applicable_no_effect_authority';
}

function scrollBehavior(): ScrollBehavior {
  if (
    typeof window !== 'undefined'
    && window.matchMedia('(prefers-reduced-motion: reduce)').matches
  ) {
    return 'auto';
  }
  return 'smooth';
}

function replaceIntentStarter(value: string, nextStarter: string): string {
  const previous = GENERATIVE_MODES.find(
    (candidate) => candidate.starter && value.startsWith(candidate.starter),
  );
  const body = previous ? value.slice(previous.starter.length) : value;
  return nextStarter ? `${nextStarter}${body}` : body;
}

function stripIntentStarter(value: string): string {
  const previous = GENERATIVE_MODES.find(
    (candidate) => candidate.starter && value.startsWith(candidate.starter),
  );
  return (previous ? value.slice(previous.starter.length) : value).trim();
}

function clampSurfacePoint(
  x: number,
  y: number,
  width = 360,
  height = 430,
): { x: number; y: number } {
  if (typeof window === 'undefined') return { x, y };
  return {
    x: Math.max(12, Math.min(x, window.innerWidth - width - 12)),
    y: Math.max(12, Math.min(y, window.innerHeight - height - 12)),
  };
}

function clampAboveSurfaceAnchor(
  x: number,
  anchorY: number,
  width: number,
  estimatedHeight: number,
): { x: number; y: number } {
  if (typeof window === 'undefined') return { x, y: anchorY };
  const effectiveHeight = Math.min(estimatedHeight, window.innerHeight - 24);
  return {
    x: Math.max(12, Math.min(x, window.innerWidth - width - 12)),
    y: Math.max(effectiveHeight + 12, Math.min(anchorY, window.innerHeight - 12)),
  };
}

function surfaceStyle(surface: Exclude<InteractionSurface, null>): CSSProperties {
  return { left: surface.x, top: surface.y };
}

export function PublicChat(): JSX.Element {
  const { access, authenticated, subjectKey, refresh } = useSiteSession();
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [sessionSummaries, setSessionSummaries] = useState<DurableSessionSummary[]>([]);
  const [sessionReady, setSessionReady] = useState(false);
  const [sessionLoading, setSessionLoading] = useState(false);
  const [historyHydrating, setHistoryHydrating] = useState(false);
  const [restorationNotice, setRestorationNotice] = useState('');
  const [deletingSession, setDeletingSession] = useState(false);
  const [currentSessionRecorded, setCurrentSessionRecorded] = useState(false);
  const [historyTruncated, setHistoryTruncated] = useState(false);
  const [worldState, setWorldState] = useState<DurableWorldState>(EMPTY_WORLD_STATE);
  const [worldlineJobs, setWorldlineJobs] = useState<Array<Record<string, unknown>>>([]);
  const [worldlineArtifacts, setWorldlineArtifacts] = useState<Array<Record<string, unknown>>>([]);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState('');
  const [waiting, setWaiting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pendingTurn, setPendingTurn] = useState<PendingTurn | null>(null);
  const [lastModel, setLastModel] = useState<string | null>(null);
  const [mode, setMode] = useState<GenerativeMode>('general');
  const [codePathInput, setCodePathInput] = useState('');
  const [codeApproval, setCodeApproval] = useState<CodeApproval | null>(null);
  const [rollingBackId, setRollingBackId] = useState<string | null>(null);
  const [surface, setSurface] = useState<InteractionSurface>(null);
  const [inspectedMessageId, setInspectedMessageId] = useState<string | null>(null);
  const [copiedMessageId, setCopiedMessageId] = useState<string | null>(null);
  const inFlightRef = useRef(false);
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const surfaceRef = useRef<HTMLDivElement>(null);
  const surfaceTriggerRef = useRef<HTMLElement | null>(null);
  const lensButtonRef = useRef<HTMLButtonElement>(null);
  const sessionGenerationRef = useRef(0);
  const authGenerationRef = useRef(0);
  const conversationIdRef = useRef<string | null>(null);
  const activeControllersRef = useRef<Set<AbortController>>(new Set());
  const snapshotPollAttemptsRef = useRef(0);
  const snapshotPollSessionRef = useRef<string | null>(null);
  const sessionBootstrappedRef = useRef(false);
  const loadedStoredSessionRef = useRef(false);
  const sessionSubjectRef = useRef<string | undefined>(undefined);

  const abortActiveOperations = useCallback(() => {
    for (const controller of activeControllersRef.current) controller.abort();
    activeControllersRef.current.clear();
    inFlightRef.current = false;
  }, []);

  const isCurrentOperation = useCallback((generation: number, sessionId: string) => (
    generation === sessionGenerationRef.current
    && sessionId === conversationIdRef.current
  ), []);

  const closeSurface = useCallback((restoreFocus = false) => {
    setSurface(null);
    if (restoreFocus) {
      requestAnimationFrame(() => {
        if (surfaceTriggerRef.current?.isConnected) surfaceTriggerRef.current.focus();
      });
    }
  }, []);

  const openIntentLens = useCallback((
    trigger: HTMLElement,
    point?: { x: number; y: number },
  ) => {
    surfaceTriggerRef.current = trigger;
    if (point) {
      const clamped = clampSurfacePoint(point.x, point.y);
      setSurface({ kind: 'intent', ...clamped, placement: 'point' });
      return;
    }
    const rect = trigger.getBoundingClientRect();
    const x = typeof window === 'undefined'
      ? rect.left
      : Math.max(12, Math.min(rect.left, window.innerWidth - 372));
    setSurface({ kind: 'intent', x, y: rect.top - 10, placement: 'above' });
  }, []);

  useEffect(() => {
    let stored: string | null = null;
    try {
      stored = window.localStorage.getItem(ACTIVE_SESSION_KEY);
    } catch {
      // A blocked storage surface must not block a new durable runtime ID.
    }
    loadedStoredSessionRef.current = Boolean(stored && CLIENT_UUID_PATTERN.test(stored));
    const initialSessionId = loadedStoredSessionRef.current && stored
      ? stored
      : crypto.randomUUID().toLowerCase();
    conversationIdRef.current = initialSessionId;
    setConversationId(initialSessionId);
  }, []);

  const rememberSession = useCallback((sessionId: string) => {
    conversationIdRef.current = sessionId;
    setConversationId(sessionId);
    try {
      window.localStorage.setItem(ACTIVE_SESSION_KEY, sessionId);
    } catch {
      // The server-side principal binding remains durable without local storage.
    }
  }, []);

  useEffect(() => {
    if (!subjectKey) {
      if (!authenticated && sessionSubjectRef.current !== undefined) {
        abortActiveOperations();
        ++authGenerationRef.current;
        ++sessionGenerationRef.current;
        sessionSubjectRef.current = undefined;
        setCodeApproval(null);
        setWaiting(false);
        setRollingBackId(null);
      }
      return;
    }
    if (sessionSubjectRef.current === undefined) {
      sessionSubjectRef.current = subjectKey;
      return;
    }
    if (sessionSubjectRef.current === subjectKey) return;
    abortActiveOperations();
    sessionSubjectRef.current = subjectKey;
    ++authGenerationRef.current;
    ++sessionGenerationRef.current;
    sessionBootstrappedRef.current = false;
    loadedStoredSessionRef.current = false;
    setSessionReady(false);
    setSessionLoading(false);
    setHistoryHydrating(false);
    setSessionSummaries([]);
    setMessages([]);
    setDraft('');
    setPendingTurn(null);
    setError(null);
    setHistoryTruncated(false);
    setWorldState(EMPTY_WORLD_STATE);
    setWorldlineJobs([]);
    setWorldlineArtifacts([]);
    setCurrentSessionRecorded(false);
    setCodeApproval(null);
    setWaiting(false);
    setRollingBackId(null);
    setRestorationNotice('Account changed. A separate worldline is being restored.');
    rememberSession(crypto.randomUUID().toLowerCase());
  }, [abortActiveOperations, authenticated, rememberSession, subjectKey]);

  useEffect(() => () => abortActiveOperations(), [abortActiveOperations]);

  const fetchSessionSummaries = useCallback(async (
    signal?: AbortSignal,
  ): Promise<DurableSessionSummary[]> => {
    const response = await authFetch('/api/apocrypha/sessions', {
      method: 'GET',
      cache: 'no-store',
      credentials: 'same-origin',
      signal,
    });
    const body = recordValue(await response.json());
    if (!response.ok) {
      if (response.status === 401) await refresh();
      throw new Error(stringValue(body?.error) ?? 'Recent conversations are unavailable.');
    }
    if (!body || !Array.isArray(body.sessions)) {
      throw new Error('The durable conversation list returned an invalid envelope.');
    }
    const summaries = body.sessions.map(durableSessionSummary);
    if (summaries.some((summary) => summary === null)) {
      throw new Error('The durable conversation list returned an invalid thread.');
    }
    return summaries as DurableSessionSummary[];
  }, [refresh]);

  const fetchSessionSnapshot = useCallback(async (
    sessionId: string,
    signal?: AbortSignal,
  ): Promise<DurableSessionSnapshot> => {
    const response = await authFetch(
      `/api/apocrypha/sessions?session_id=${encodeURIComponent(sessionId)}`,
      { method: 'GET', cache: 'no-store', credentials: 'same-origin', signal },
    );
    const body = recordValue(await response.json());
    if (!response.ok) {
      if (response.status === 401) await refresh();
      throw new Error(stringValue(body?.error) ?? 'This conversation could not be restored.');
    }
    const snapshot = durableSessionSnapshot(body?.session, sessionId);
    if (!snapshot) {
      throw new Error('The durable conversation returned an invalid history envelope.');
    }
    return snapshot;
  }, [refresh]);

  const adoptSnapshot = useCallback((snapshot: DurableSessionSnapshot) => {
    setMessages(restoredMessages(snapshot));
    setHistoryTruncated(
      snapshot.events_truncated
      || Object.values(snapshot.surface_truncation).some((entry) => entry.truncated),
    );
    setWorldState(snapshot.world);
    setWorldlineJobs(snapshot.jobs);
    setWorldlineArtifacts(snapshot.artifacts);
    setCurrentSessionRecorded(true);
    setPendingTurn(null);
  }, []);

  const refreshRecentSessions = useCallback(async (): Promise<void> => {
    const generation = sessionGenerationRef.current;
    try {
      const summaries = await fetchSessionSummaries();
      if (generation === sessionGenerationRef.current) setSessionSummaries(summaries);
    } catch {
      // Opening the menu must not replace a working conversation with a list error.
    }
  }, [fetchSessionSummaries]);

  const refreshCurrentSnapshot = useCallback(async (): Promise<boolean> => {
    const sessionId = conversationIdRef.current;
    if (!authenticated || !sessionId) return false;
    const generation = sessionGenerationRef.current;
    const controller = new AbortController();
    activeControllersRef.current.add(controller);
    try {
      const snapshot = await fetchSessionSnapshot(sessionId, controller.signal);
      if (!isCurrentOperation(generation, sessionId)) return false;
      adoptSnapshot(snapshot);
      return true;
    } catch {
      return false;
    } finally {
      activeControllersRef.current.delete(controller);
    }
  }, [adoptSnapshot, authenticated, fetchSessionSnapshot, isCurrentOperation]);

  const openDurableSession = useCallback(async (sessionId: string): Promise<void> => {
    if (waiting || rollingBackId || sessionLoading || sessionId === conversationId) {
      closeSurface(true);
      return;
    }
    abortActiveOperations();
    const generation = ++sessionGenerationRef.current;
    setCodeApproval(null);
    setSessionLoading(true);
    setHistoryHydrating(true);
    setRestorationNotice('');
    setError(null);
    try {
      const snapshot = await fetchSessionSnapshot(sessionId);
      if (generation !== sessionGenerationRef.current) return;
      rememberSession(sessionId);
      adoptSnapshot(snapshot);
      setLastModel(null);
      closeSurface();
      requestAnimationFrame(() => {
        if (generation !== sessionGenerationRef.current) return;
        setHistoryHydrating(false);
        setRestorationNotice(
          `Restored ${snapshot.messages.length} message${snapshot.messages.length === 1 ? '' : 's'} from this worldline.`,
        );
        composerRef.current?.focus();
      });
    } catch (cause) {
      if (generation === sessionGenerationRef.current) {
        setHistoryHydrating(false);
        setError(cause instanceof Error ? cause.message : 'This conversation could not be restored.');
      }
    } finally {
      if (generation === sessionGenerationRef.current) setSessionLoading(false);
    }
  }, [abortActiveOperations, adoptSnapshot, closeSurface, conversationId, fetchSessionSnapshot, rememberSession, rollingBackId, sessionLoading, waiting]);

  useEffect(() => {
    if (!authenticated || !conversationId) {
      sessionBootstrappedRef.current = false;
      setSessionReady(false);
      if (access === 'signed-out' || access === 'unavailable') {
        abortActiveOperations();
        ++sessionGenerationRef.current;
        setSessionSummaries([]);
        setMessages([]);
        setHistoryHydrating(false);
        setRestorationNotice('');
        setHistoryTruncated(false);
        setWorldState(EMPTY_WORLD_STATE);
        setWorldlineJobs([]);
        setWorldlineArtifacts([]);
        setCurrentSessionRecorded(false);
        setCodeApproval(null);
        setWaiting(false);
        setRollingBackId(null);
        try {
          window.localStorage.removeItem(ACTIVE_SESSION_KEY);
        } catch {
          // There is no client-side thread identifier to clear when storage is blocked.
        }
      }
      return undefined;
    }
    if (sessionBootstrappedRef.current) return undefined;
    sessionBootstrappedRef.current = true;
    const generation = ++sessionGenerationRef.current;
    let active = true;
    const controller = new AbortController();
    activeControllersRef.current.add(controller);
    setSessionLoading(true);
    setHistoryHydrating(true);
    setRestorationNotice('');
    const recover = async () => {
      try {
        const storedSessionId = loadedStoredSessionRef.current ? conversationId : null;
        let selected: string | undefined = storedSessionId ?? undefined;
        let snapshot: DurableSessionSnapshot | null = null;
        let summaries: DurableSessionSummary[] = [];
        let recentListAttempted = false;

        if (storedSessionId) {
          try {
            snapshot = await fetchSessionSnapshot(storedSessionId, controller.signal);
          } catch (directCause) {
            recentListAttempted = true;
            summaries = await fetchSessionSummaries(controller.signal);
            const newest = summaries[0]?.session_id;
            if (!newest || newest === storedSessionId) throw directCause;
            selected = newest;
            snapshot = await fetchSessionSnapshot(newest, controller.signal);
          }
        } else {
          recentListAttempted = true;
          summaries = await fetchSessionSummaries(controller.signal);
          selected = summaries[0]?.session_id;
          if (selected) snapshot = await fetchSessionSnapshot(selected, controller.signal);
        }

        if (!active || generation !== sessionGenerationRef.current) return;
        if (snapshot && selected) {
          adoptSnapshot(snapshot);
          setHistoryHydrating(false);
          setRestorationNotice(
            `Restored ${snapshot.messages.length} message${snapshot.messages.length === 1 ? '' : 's'} from your active worldline.`,
          );
          rememberSession(selected);
        } else {
          setHistoryTruncated(false);
          setWorldState(EMPTY_WORLD_STATE);
          setWorldlineJobs([]);
          setWorldlineArtifacts([]);
          setCurrentSessionRecorded(false);
          setHistoryHydrating(false);
        }

        if (storedSessionId && snapshot && !recentListAttempted) {
          try {
            recentListAttempted = true;
            summaries = await fetchSessionSummaries(controller.signal);
          } catch {
            // A transient recent-list failure cannot replace a directly restored pointer.
          }
        }
        if (!active || generation !== sessionGenerationRef.current) return;
        if (summaries.length > 0) setSessionSummaries(summaries);
      } catch (cause) {
        if (!active || generation !== sessionGenerationRef.current) return;
        const isolatedSessionId = crypto.randomUUID().toLowerCase();
        loadedStoredSessionRef.current = false;
        rememberSession(isolatedSessionId);
        setMessages([]);
        setHistoryHydrating(false);
        setHistoryTruncated(false);
        setWorldState(EMPTY_WORLD_STATE);
        setWorldlineJobs([]);
        setWorldlineArtifacts([]);
        setCurrentSessionRecorded(false);
        setCodeApproval(null);
        setError(
          cause instanceof Error
            ? `${cause.message} A new isolated conversation has been prepared; hidden history will not be reused.`
            : 'Durable conversation recovery is unavailable. A new isolated conversation has been prepared.',
        );
      } finally {
        activeControllersRef.current.delete(controller);
        if (active && generation === sessionGenerationRef.current) {
          setSessionLoading(false);
          setSessionReady(true);
        }
      }
    };
    void recover();
    return () => {
      active = false;
      controller.abort();
      activeControllersRef.current.delete(controller);
    };
  }, [abortActiveOperations, access, adoptSnapshot, authenticated, conversationId, fetchSessionSnapshot, fetchSessionSummaries, rememberSession]);

  useEffect(() => {
    const shouldPoll = Boolean(
      authenticated
      && conversationId
      && sessionReady
      && !sessionLoading
      && !waiting
      && !rollingBackId
      && (worldState.active_job_count > 0 || worldState.pending_turn_count > 0),
    );
    if (snapshotPollSessionRef.current !== conversationId) {
      snapshotPollSessionRef.current = conversationId;
      snapshotPollAttemptsRef.current = 0;
    }
    if (!shouldPoll || !conversationId) {
      if (worldState.active_job_count === 0 && worldState.pending_turn_count === 0) {
        snapshotPollAttemptsRef.current = 0;
      }
      return undefined;
    }

    const generation = sessionGenerationRef.current;
    const sessionId = conversationId;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let controller: AbortController | null = null;

    const poll = async (): Promise<void> => {
      if (cancelled || snapshotPollAttemptsRef.current >= SNAPSHOT_POLL_LIMIT) return;
      snapshotPollAttemptsRef.current += 1;
      controller = new AbortController();
      activeControllersRef.current.add(controller);
      try {
        const snapshot = await fetchSessionSnapshot(sessionId, controller.signal);
        if (cancelled || !isCurrentOperation(generation, sessionId)) return;
        adoptSnapshot(snapshot);
        const remainsActive = snapshot.world.active_job_count > 0
          || snapshot.world.pending_turn_count > 0;
        if (remainsActive && snapshotPollAttemptsRef.current < SNAPSHOT_POLL_LIMIT) {
          timer = setTimeout(() => { void poll(); }, SNAPSHOT_POLL_INTERVAL_MS);
        }
      } catch {
        if (
          !cancelled
          && isCurrentOperation(generation, sessionId)
          && snapshotPollAttemptsRef.current < SNAPSHOT_POLL_LIMIT
        ) {
          timer = setTimeout(() => { void poll(); }, SNAPSHOT_POLL_INTERVAL_MS);
        }
      } finally {
        if (controller) activeControllersRef.current.delete(controller);
        controller = null;
      }
    };

    timer = setTimeout(() => { void poll(); }, SNAPSHOT_POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
      if (controller) {
        controller.abort();
        activeControllersRef.current.delete(controller);
      }
    };
  }, [
    adoptSnapshot,
    authenticated,
    conversationId,
    fetchSessionSnapshot,
    isCurrentOperation,
    rollingBackId,
    sessionLoading,
    sessionReady,
    waiting,
    worldState.active_job_count,
    worldState.pending_turn_count,
  ]);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: scrollBehavior(), block: 'end' });
  }, [messages, waiting, error]);

  useEffect(() => {
    if (!surface) return undefined;
    const dismiss = (event: PointerEvent) => {
      const target = event.target as Node;
      if (
        surfaceRef.current?.contains(target)
        || surfaceTriggerRef.current?.contains(target)
      ) return;
      closeSurface();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') closeSurface(true);
    };
    const onResize = () => {
      if (
        surface.kind === 'scope'
        && surfaceRef.current?.contains(document.activeElement)
      ) return;
      closeSurface();
    };
    const onScroll = (event: Event) => {
      if (surfaceRef.current?.contains(event.target as Node)) return;
      closeSurface();
    };
    document.addEventListener('pointerdown', dismiss);
    document.addEventListener('keydown', onKeyDown);
    window.addEventListener('resize', onResize);
    window.addEventListener('scroll', onScroll, true);
    requestAnimationFrame(() => {
      const focusTarget = surface.kind === 'scope'
        ? surfaceRef.current?.querySelector<HTMLElement>('textarea')
        : surfaceRef.current?.querySelector<HTMLElement>(
          surface.kind === 'message' ? '[role="menuitem"]' : 'button',
        );
      focusTarget?.focus();
    });
    return () => {
      document.removeEventListener('pointerdown', dismiss);
      document.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('resize', onResize);
      window.removeEventListener('scroll', onScroll, true);
    };
  }, [closeSurface, surface]);

  const navigateMessageMenu = useCallback((event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Tab') {
      closeSurface();
      return;
    }
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
    const items = Array.from(
      event.currentTarget.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)'),
    );
    if (items.length === 0) return;
    event.preventDefault();
    const current = items.indexOf(document.activeElement as HTMLElement);
    const next = event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? items.length - 1
        : event.key === 'ArrowUp'
          ? (current <= 0 ? items.length - 1 : current - 1)
          : (current + 1) % items.length;
    items[next]?.focus();
  }, [closeSurface]);

  useEffect(() => {
    const openFromKeyboard = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const editing = target?.matches('input, textarea, select, [contenteditable="true"]');
      if (
        !authenticated
        || editing
        || !lensButtonRef.current
        || !((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k')
      ) return;
      event.preventDefault();
      openIntentLens(lensButtonRef.current);
    };
    document.addEventListener('keydown', openFromKeyboard);
    return () => document.removeEventListener('keydown', openFromKeyboard);
  }, [authenticated, openIntentLens]);

  const newConversation = useCallback(() => {
    if (waiting || rollingBackId) return;
    abortActiveOperations();
    ++sessionGenerationRef.current;
    loadedStoredSessionRef.current = false;
    rememberSession(crypto.randomUUID().toLowerCase());
    setSessionReady(true);
    setSessionLoading(false);
    setHistoryHydrating(false);
    setRestorationNotice('Opened a fresh worldline.');
    setHistoryTruncated(false);
    setWorldState(EMPTY_WORLD_STATE);
    setWorldlineJobs([]);
    setWorldlineArtifacts([]);
    setCurrentSessionRecorded(false);
    setMessages([]);
    setDraft('');
    setCodePathInput('');
    setCodeApproval(null);
    setPendingTurn(null);
    setError(null);
    setLastModel(null);
    setInspectedMessageId(null);
    setCopiedMessageId(null);
    closeSurface();
    requestAnimationFrame(() => composerRef.current?.focus());
  }, [abortActiveOperations, closeSurface, rememberSession, rollingBackId, waiting]);

  const deleteCurrentSession = useCallback(async (): Promise<void> => {
    if (
      !authenticated
      || !conversationId
      || deletingSession
      || waiting
      || rollingBackId
      || !currentSessionRecorded
    ) return;
    if (!window.confirm('Archive this worldline from active conversations? Its prior rows remain in the local audit ledger.')) {
      return;
    }
    abortActiveOperations();
    const generation = ++sessionGenerationRef.current;
    setDeletingSession(true);
    setSessionLoading(true);
    setSessionReady(false);
    setError(null);
    closeSurface();
    try {
      const response = await authFetch('/api/apocrypha/sessions', {
        method: 'DELETE',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          session_id: conversationId,
          request_id: crypto.randomUUID().toLowerCase(),
        }),
      });
      const body = recordValue(await response.json());
      if (!response.ok) {
        if (response.status === 401) await refresh();
        throw new Error(stringValue(body?.error) ?? 'This worldline could not be archived.');
      }
      if (
        body?.deleted !== true
        || body.session_id !== conversationId
        || typeof body.event_digest !== 'string'
        || !SHA256_PATTERN.test(body.event_digest)
      ) throw new Error('The deletion service returned an invalid receipt.');
      if (generation !== sessionGenerationRef.current) return;
      const nextSessionId = crypto.randomUUID().toLowerCase();
      loadedStoredSessionRef.current = false;
      rememberSession(nextSessionId);
      setSessionSummaries((current) => (
        current.filter((session) => session.session_id !== conversationId)
      ));
      setMessages([]);
      setHistoryTruncated(false);
      setWorldState(EMPTY_WORLD_STATE);
      setWorldlineJobs([]);
      setWorldlineArtifacts([]);
      setCurrentSessionRecorded(false);
      setHistoryHydrating(false);
      setPendingTurn(null);
      setLastModel(null);
      setRestorationNotice(`Worldline archived with tombstone receipt ${body.event_digest.slice(0, 12)}. Prior audit rows remain. A fresh worldline is ready.`);
      try {
        const summaries = await fetchSessionSummaries();
        if (generation === sessionGenerationRef.current) setSessionSummaries(summaries);
      } catch {
        // The durable deletion receipt is authoritative; list refresh is secondary.
      }
    } catch (cause) {
      if (generation === sessionGenerationRef.current) {
        setError(cause instanceof Error ? cause.message : 'This worldline could not be archived.');
      }
    } finally {
      if (generation === sessionGenerationRef.current) {
        setDeletingSession(false);
        setSessionLoading(false);
        setSessionReady(true);
      }
    }
  }, [abortActiveOperations, authenticated, closeSurface, conversationId, currentSessionRecorded, deletingSession, fetchSessionSummaries, refresh, rememberSession, rollingBackId, waiting]);

  const selectMode = useCallback((candidate: (typeof GENERATIVE_MODES)[number]) => {
    setMode(candidate.id);
    setCodeApproval(null);
    setDraft((current) => replaceIntentStarter(current, candidate.starter));
    if (candidate.id === 'code' && access === 'owner') {
      const trigger = surfaceTriggerRef.current ?? lensButtonRef.current;
      const rect = trigger?.getBoundingClientRect();
      const anchor = clampAboveSurfaceAnchor(
        rect?.left ?? 12,
        (rect?.top ?? 520) - 10,
        540,
        460,
      );
      setSurface({ kind: 'scope', ...anchor });
    } else {
      closeSurface();
      requestAnimationFrame(() => composerRef.current?.focus());
    }
  }, [access, closeSurface]);

  const openMessageMenu = useCallback((
    messageId: string,
    trigger: HTMLElement,
    point: { x: number; y: number },
  ) => {
    surfaceTriggerRef.current = trigger;
    const clamped = clampSurfacePoint(point.x, point.y, 230, 250);
    setSurface({ kind: 'message', messageId, ...clamped });
  }, []);

  const copyMessage = useCallback(async (message: ChatMessage) => {
    try {
      await navigator.clipboard.writeText(message.text);
      setCopiedMessageId(message.id);
      window.setTimeout(() => setCopiedMessageId((current) => (
        current === message.id ? null : current
      )), 1_600);
    } catch {
      setError('The browser did not allow this message to be copied.');
    } finally {
      closeSurface(true);
    }
  }, [closeSurface]);

  const reopenMessage = useCallback((message: ChatMessage) => {
    setDraft(message.role === 'user'
      ? message.text
      : `Continue from this response:\n${message.text}`);
    setPendingTurn(null);
    setError(null);
    closeSurface();
    requestAnimationFrame(() => composerRef.current?.focus());
  }, [closeSurface]);

  const send = useCallback(async (retry?: PendingTurn): Promise<void> => {
    const text = retry?.text ?? draft.trim();
    const runCodeEffect = !retry && access === 'owner' && mode === 'code';
    const allowedPaths = runCodeEffect ? parseCodePaths(codePathInput) : [];
    const duplicatePath = new Set(allowedPaths).size !== allowedPaths.length;
    const authGeneration = authGenerationRef.current;
    const expectedApprovalBinding = runCodeEffect && subjectKey && conversationId
      ? codeApprovalBinding(
        subjectKey,
        authGeneration,
        conversationId,
        text,
        allowedPaths,
      )
      : null;
    const exactApproval = Boolean(
      expectedApprovalBinding
      && codeApproval
      && codeApproval.binding === expectedApprovalBinding
      && codeApproval.authGeneration === authGeneration
      && codeApproval.subjectKey === subjectKey
      && codeApproval.conversationId === conversationId
      && codeApproval.objective === text
      && JSON.stringify(codeApproval.allowedPaths) === JSON.stringify(allowedPaths),
    );
    if (
      !authenticated
      || !conversationId
      || !sessionReady
      || !text
      || waiting
      || rollingBackId
      || inFlightRef.current
    ) {
      return;
    }
    if (runCodeEffect && (
      !exactApproval
      || allowedPaths.length < 1
      || allowedPaths.length > 32
      || duplicatePath
    )) {
      setError('Owner Code mode requires 1–32 unique repository-relative paths and explicit effect confirmation.');
      const trigger = lensButtonRef.current;
      const rect = trigger?.getBoundingClientRect();
      surfaceTriggerRef.current = trigger;
      const anchor = clampAboveSurfaceAnchor(
        rect?.left ?? 12,
        (rect?.top ?? 520) - 10,
        540,
        460,
      );
      setSurface({
        kind: 'scope',
        ...anchor,
      });
      return;
    }
    if (byteLength(text) > MAX_TEXT_BYTES) {
      setError(`Message exceeds the ${MAX_TEXT_BYTES.toLocaleString()}-byte turn limit.`);
      return;
    }

    const requestId = retry?.requestId ?? crypto.randomUUID().toLowerCase();
    const messageId = retry?.messageId ?? `turn-${requestId}`;
    const pending: PendingTurn = { messageId, requestId, text };
    const sentAt = new Date().toISOString();
    const dispatchGeneration = sessionGenerationRef.current;
    const dispatchConversationId = conversationId;

    inFlightRef.current = true;
    if (runCodeEffect) setCodeApproval(null);
    if (!retry) {
      setMessages((current) => [
        ...current,
        { id: messageId, role: 'user', text, requestId, recordedAt: sentAt },
      ]);
      setDraft('');
    }
    setPendingTurn(null);
    setWaiting(true);
    setError(null);

    const controller = new AbortController();
    activeControllersRef.current.add(controller);
    const deadline = setTimeout(
      () => controller.abort(),
      runCodeEffect ? CODE_BROWSER_DEADLINE_MS : CHAT_BROWSER_DEADLINE_MS,
    );
    let retryable = !runCodeEffect;
    try {
      if (runCodeEffect) {
        const response = await authFetch('/api/admin/apocv4/code', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          cache: 'no-store',
          credentials: 'same-origin',
          signal: controller.signal,
          body: JSON.stringify({
            objective: text,
            allowed_paths: allowedPaths,
            session_id: dispatchConversationId,
            request_id: requestId,
            confirm_apply: true,
          }),
        });
        const body = await response.json() as CodeResponse;
        if (!response.ok) {
          if (response.status === 401) await refresh();
          throw new Error(safeError(body, response.status));
        }
        if (!isCurrentOperation(dispatchGeneration, dispatchConversationId)) return;
        const codeReceipt = codeEffectReceipt(body, dispatchConversationId, requestId);
        if (!codeReceipt) {
          throw new Error('The runtime returned an invalid governed code-effect receipt.');
        }
        setMessages((current) => [
          ...current,
          {
            id: `code-reply-${requestId}`,
            role: 'apocrypha',
            text: codeEffectSummary(codeReceipt),
            requestId,
            recordedAt: new Date().toISOString(),
            codeEffect: codeReceipt,
          },
        ]);
        setWorldState((current) => ({
          ...current,
          code_request_count: current.code_request_count + (codeReceipt.durableReplay ? 0 : 1),
          proposal_count: current.proposal_count + (codeReceipt.durableReplay ? 0 : 1),
          effect_count: current.effect_count + (
            codeReceipt.durableReplay ? 0 : codeReceipt.settledEffectCount ?? 1
          ),
          last_event_type: codeReceipt.state === 'ROLLED_BACK' ? 'ROLLBACK' : 'CODE_EFFECT',
          last_event_digest: codeReceipt.sessionEventDigest ?? current.last_event_digest,
        }));
        setCurrentSessionRecorded(true);
        return;
      }
      const response = await authFetch('/api/apocrypha/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        cache: 'no-store',
        credentials: 'same-origin',
        signal: controller.signal,
        body: JSON.stringify({
          text,
          session_id: dispatchConversationId,
          request_id: requestId,
        }),
      });
      const body = await response.json() as TurnResponse;
      if (!response.ok) {
        retryable = response.status === 409
          || response.status === 429
          || response.status >= 500;
        if (response.status === 401) await refresh();
        throw new Error(safeError(body, response.status));
      }
      if (!isCurrentOperation(dispatchGeneration, dispatchConversationId)) return;
      if (!isExactTurn(body, dispatchConversationId, requestId)) {
        throw new Error('The native body returned an invalid public-turn envelope.');
      }
      const responseText = body.text;
      const identity = identityReceipt(body.identity);
      const context = contextReceipt(body.context);
      setLastModel(body.model_id);
      setMessages((current) => [
        ...current,
        {
          id: `reply-${requestId}`,
          role: 'apocrypha',
          text: responseText,
          requestId,
          recordedAt: new Date().toISOString(),
          receipt: {
            modelId: body.model_id,
            responseId: body.response_id,
            responseDigest: body.response_digest,
            servingProfileDigest: body.serving_profile_digest,
            memoryScope: String(body.memory_scope),
            conversationHistory: String(body.conversation_history),
            identity,
            context,
          },
        },
      ]);
      setWorldState((current) => ({
        ...current,
        message_count: current.message_count + 2,
        last_event_type: 'CHAT_ASSISTANT',
      }));
      setCurrentSessionRecorded(true);
    } catch (cause) {
      if (!isCurrentOperation(dispatchGeneration, dispatchConversationId)) return;
      const timedOut = cause instanceof DOMException && cause.name === 'AbortError';
      let reconciledCodeRequest = false;
      if (runCodeEffect) {
        const reconciliationController = new AbortController();
        const reconciliationDeadline = setTimeout(
          () => reconciliationController.abort(),
          12_000,
        );
        activeControllersRef.current.add(reconciliationController);
        try {
          const snapshot = await fetchSessionSnapshot(
            dispatchConversationId,
            reconciliationController.signal,
          );
          if (!isCurrentOperation(dispatchGeneration, dispatchConversationId)) return;
          reconciledCodeRequest = snapshot.code_requests.some(
            (request) => request.request_id === requestId,
          );
          if (reconciledCodeRequest) adoptSnapshot(snapshot);
        } catch {
          // The original request remains visible and uncertain; it is never reminted here.
        } finally {
          clearTimeout(reconciliationDeadline);
          activeControllersRef.current.delete(reconciliationController);
        }
      } else if (timedOut || retryable) {
        setPendingTurn(pending);
      } else {
        setMessages((current) => current.filter((message) => message.id !== messageId));
        setDraft(text);
      }
      if (!isCurrentOperation(dispatchGeneration, dispatchConversationId)) return;
      setError(
        timedOut && runCodeEffect
          ? reconciledCodeRequest
            ? 'The effect connection timed out, but the original request was recovered from the durable worldline. No second effect was sent.'
            : 'The effect outcome is uncertain. The original request remains visible and its one-run approval is consumed; no second effect was sent.'
          : runCodeEffect
            ? reconciledCodeRequest
              ? 'The original effect request was reconciled from the durable worldline. No second effect was sent.'
              : 'The effect request could not be reconciled yet. Its original intent remains visible and no second effect was sent.'
          : timedOut
          ? 'Apocrypha did not answer before the bounded turn deadline.'
          : cause instanceof Error ? cause.message : 'The turn could not be completed.',
      );
    } finally {
      clearTimeout(deadline);
      activeControllersRef.current.delete(controller);
      if (isCurrentOperation(dispatchGeneration, dispatchConversationId)) {
        inFlightRef.current = false;
        setWaiting(false);
        requestAnimationFrame(() => composerRef.current?.focus());
      }
    }
  }, [
    access,
    adoptSnapshot,
    authenticated,
    codeApproval,
    codePathInput,
    conversationId,
    draft,
    fetchSessionSnapshot,
    isCurrentOperation,
    mode,
    refresh,
    rollingBackId,
    sessionReady,
    subjectKey,
    waiting,
  ]);

  const rollbackCodeEffect = useCallback(async (
    messageId: string,
    promotionEventDigest: string,
  ): Promise<void> => {
    if (access !== 'owner' || !conversationId || waiting || rollingBackId) return;
    const requestId = crypto.randomUUID().toLowerCase();
    const dispatchGeneration = sessionGenerationRef.current;
    const dispatchConversationId = conversationId;
    const controller = new AbortController();
    activeControllersRef.current.add(controller);
    setRollingBackId(messageId);
    setError(null);
    try {
      const response = await authFetch('/api/admin/apocv4/code/rollback', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        cache: 'no-store',
        credentials: 'same-origin',
        signal: controller.signal,
        body: JSON.stringify({
          promotion_event_digest: promotionEventDigest,
          session_id: dispatchConversationId,
          request_id: requestId,
          confirm_rollback: true,
        }),
      });
      const body = await response.json() as RollbackResponse;
      if (!response.ok) {
        if (response.status === 401) await refresh();
        throw new Error(safeError(body, response.status));
      }
      if (!isCurrentOperation(dispatchGeneration, dispatchConversationId)) return;
      const observed = recordValue(body.observed);
      const runtime = recordValue(observed?.runtime);
      const rollbackDigest = digestValue(runtime?.rollback_event_digest);
      const sessionDigests = recordValue(runtime?.session_event_digests);
      const sessionRollbackDigest = digestValue(sessionDigests?.rollback);
      const durableReplay = runtime?.durable_replay;
      if (
        body.kind !== 'rollback'
        || runtime?.state !== 'ROLLED_BACK'
        || runtime.promotion_event_digest !== promotionEventDigest
        || runtime.session_id !== dispatchConversationId
        || runtime.request_id !== requestId
        || typeof durableReplay !== 'boolean'
        || !rollbackDigest
        || !sessionRollbackDigest
      ) {
        throw new Error('The runtime returned an invalid governed rollback receipt.');
      }
      setMessages((current) => current.map((message) => message.id === messageId && message.codeEffect
        ? {
          ...message,
          text: `Governed code change rolled back. ${message.codeEffect.allowedPaths.length} admitted file${message.codeEffect.allowedPaths.length === 1 ? '' : 's'} restored from the promotion snapshot.`,
          codeEffect: {
            ...message.codeEffect,
            state: 'ROLLED_BACK',
            rollbackEventDigest: rollbackDigest,
            sessionEventDigest: sessionRollbackDigest,
            durableReplay,
          },
        }
        : message));
      setWorldState((current) => ({
        ...current,
        effect_count: current.effect_count + (durableReplay ? 0 : 1),
        last_event_type: 'ROLLBACK',
        last_event_digest: sessionRollbackDigest,
      }));
      setCurrentSessionRecorded(true);
    } catch (cause) {
      if (!isCurrentOperation(dispatchGeneration, dispatchConversationId)) return;
      setError(cause instanceof Error ? cause.message : 'The rollback could not be completed.');
    } finally {
      activeControllersRef.current.delete(controller);
      if (isCurrentOperation(dispatchGeneration, dispatchConversationId)) {
        setRollingBackId(null);
      }
    }
  }, [access, conversationId, isCurrentOperation, refresh, rollingBackId, waiting]);

  const currentBytes = byteLength(draft);
  const selectedMode = GENERATIVE_MODES.find((candidate) => candidate.id === mode) ?? GENERATIVE_MODES[0]!;
  const codePaths = parseCodePaths(codePathInput);
  const duplicateCodePath = new Set(codePaths).size !== codePaths.length;
  const ownerCodeMode = access === 'owner' && mode === 'code';
  const currentApprovalBinding = subjectKey && conversationId
    ? codeApprovalBinding(
      subjectKey,
      authGenerationRef.current,
      conversationId,
      draft.trim(),
      codePaths,
    )
    : null;
  const codeApprovalCurrent = Boolean(
    currentApprovalBinding && codeApproval?.binding === currentApprovalBinding,
  );
  const sessionLabel = access === 'checking'
    ? 'Checking sign-in'
    : sessionLoading
      ? 'Restoring conversation'
    : authenticated
      ? 'Signed in'
      : access === 'unavailable'
        ? 'Verification unavailable'
        : 'Sign in required';
  const firstUserMessage = messages.find((message) => message.role === 'user');
  const conversationTitle = firstUserMessage
    ? stripIntentStarter(firstUserMessage.text).replace(/\s+/g, ' ').slice(0, 62)
    : 'New conversation';
  const turnCount = messages.filter((message) => message.role === 'user').length;
  const worldlineActivityLabel = worldState.active_job_count > 0
    ? `${worldState.active_job_count} background task${worldState.active_job_count === 1 ? '' : 's'} orbiting`
    : worldState.failed_turn_count > 0
      ? `${worldState.failed_turn_count} failed turn${worldState.failed_turn_count === 1 ? '' : 's'} visible`
      : worldState.pending_turn_count > 0
        ? `${worldState.pending_turn_count} pending turn${worldState.pending_turn_count === 1 ? '' : 's'} visible`
        : worldState.artifact_count > 0
          ? `${worldState.artifact_count} artifact${worldState.artifact_count === 1 ? '' : 's'} held here`
          : null;
  const latestReceipt = [...messages].reverse().find((message) => message.receipt)?.receipt;
  const historyLabel = latestReceipt?.conversationHistory === 'durable_principal_bound'
    ? historyTruncated ? 'Durable · older turns folded' : 'Durable runtime history'
    : historyTruncated
      ? 'Durable · older turns folded'
    : latestReceipt
      ? 'This open conversation'
      : 'Confirmed with the first response';
  const contextualMessage = surface?.kind === 'message'
    ? messages.find((message) => message.id === surface.messageId) ?? null
    : null;
  const codeScopeState = codeApprovalCurrent
    ? 'Authorized once'
    : codePaths.length > 0
      ? `${codePaths.length} path${codePaths.length === 1 ? '' : 's'} · confirm`
      : 'Set effect scope';

  return (
    <div className={styles.page} data-public-apocrypha="apocv4-chat-v1">
      <CyberDreamField variant="relay" activity={waiting ? 'thinking' : draft.trim() ? 'listening' : 'idle'} density={0.78} viewport />
      <a className={styles.skipLink} href="#apocrypha-conversation">Skip to conversation</a>

      <header className={styles.header}>
        <Link href="/" className={styles.brand} aria-label="Apocky home">
          <span className={styles.brandMark} aria-hidden="true">A</span>
          <span>APOCKY</span>
        </Link>
        <div className={styles.identity}>
          <span className={styles.identityName}>Apocrypha</span>
          <span className={styles.identityMeta}>Digital intelligence workspace</span>
        </div>
        <nav className={styles.nav} aria-label="Apocrypha navigation">
          <Link href="/clearing">The Clearing</Link>
          <Link href={authenticated ? '/account' : '/login?next=%2Fapocrypha'}>
            {authenticated ? 'Account' : 'Sign in'}
          </Link>
        </nav>
      </header>

      <main className={styles.workspace} id="apocrypha-conversation">
        <section className={styles.conversation} aria-label="Conversation with Apocrypha">
          <div className={styles.conversationHeader}>
            <div className={styles.threadHeading}>
              <span
                className={styles.sessionPulse}
                data-state={authenticated ? 'ready' : 'closed'}
                aria-hidden="true"
                title={sessionLabel}
              />
              <span className={styles.srOnly} role="status">{sessionLabel}</span>
              <div>
                <h1 title={conversationTitle}>{conversationTitle}</h1>
                <p>{turnCount === 0
                  ? worldlineActivityLabel ?? 'Ready when you are'
                  : `${turnCount} turn${turnCount === 1 ? '' : 's'}${worldlineActivityLabel ? ` · ${worldlineActivityLabel}` : ''}`}</p>
              </div>
            </div>
            <div className={styles.headerActions}>
              <button
                type="button"
                className={styles.contextTrigger}
                aria-label="Conversation actions"
                aria-haspopup="dialog"
                aria-expanded={surface?.kind === 'conversation'}
                onClick={(event) => {
                  if (surface?.kind === 'conversation') {
                    closeSurface(true);
                    return;
                  }
                  const rect = event.currentTarget.getBoundingClientRect();
                  surfaceTriggerRef.current = event.currentTarget;
                  setSurface({ kind: 'conversation', x: rect.right, y: rect.bottom + 8 });
                  void refreshRecentSessions();
                  void refreshCurrentSnapshot();
                }}
                disabled={!authenticated || waiting || Boolean(rollingBackId)}
              >
                <span aria-hidden="true">•••</span>
              </button>
            </div>
          </div>

          <div
            className={styles.messages}
            role="log"
            aria-live={historyHydrating ? 'off' : 'polite'}
            aria-relevant="additions"
            aria-busy={waiting}
            data-message-count={messages.length}
            onContextMenu={(event) => {
              if ((event.target as HTMLElement).closest('article')) return;
              event.preventDefault();
              openIntentLens(event.currentTarget, { x: event.clientX, y: event.clientY });
            }}
          >
            {messages.length === 0 && (
              <div className={styles.emptyState}>
                <div className={styles.emptyKicker} aria-hidden="true">A</div>
                <h2>Bring me something difficult.</h2>
                <p>Write naturally—or right-click the space around us to choose a different shape of thought.</p>
                <div className={styles.emptyHints} aria-label="Interaction hints">
                  <span><kbd>Ctrl K</kbd> approach constellation</span>
                  <span><kbd>Right-click</kbd> contextual actions</span>
                </div>
              </div>
            )}

            {messages.map((message) => (
              <article
                key={message.id}
                className={`${styles.message} ${
                  message.role === 'user' ? styles.userMessage : styles.apocryphaMessage
                }`}
                tabIndex={0}
                onContextMenu={(event) => {
                  event.preventDefault();
                  openMessageMenu(
                    message.id,
                    event.currentTarget,
                    { x: event.clientX, y: event.clientY },
                  );
                }}
                onKeyDown={(event) => {
                  if (event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10')) return;
                  event.preventDefault();
                  const rect = event.currentTarget.getBoundingClientRect();
                  openMessageMenu(
                    message.id,
                    event.currentTarget,
                    { x: rect.left + 24, y: rect.top + 34 },
                  );
                }}
              >
                <div className={styles.messageMeta}>
                  <p className={styles.role}>
                    {message.role === 'user' ? 'You' : 'Apocrypha'}
                  </p>
                  <div className={styles.messageSignals}>
                    {message.receipt && (
                      <button
                        type="button"
                        className={styles.evidenceMark}
                        onClick={() => setInspectedMessageId((current) => (
                          current === message.id ? null : message.id
                        ))}
                        aria-expanded={inspectedMessageId === message.id}
                      >
                        <span aria-hidden="true">✓</span> evidence
                      </button>
                    )}
                    {message.codeEffect && (
                      <button
                        type="button"
                        className={styles.effectMark}
                        onClick={() => setInspectedMessageId((current) => (
                          current === message.id ? null : message.id
                        ))}
                        aria-expanded={inspectedMessageId === message.id}
                      >
                        <span aria-hidden="true">◇</span> {message.codeEffect.state.toLowerCase()}
                      </button>
                    )}
                    {message.turnState && (
                      <button
                        type="button"
                        className={styles.turnStateMark}
                        data-state={message.turnState.state.toLowerCase()}
                        onClick={() => setInspectedMessageId((current) => (
                          current === message.id ? null : message.id
                        ))}
                        aria-expanded={inspectedMessageId === message.id}
                      >
                        <span aria-hidden="true">{message.turnState.state === 'FAILED' ? '!' : '◌'}</span>{' '}
                        {message.turnState.state.toLowerCase()}
                      </button>
                    )}
                    {copiedMessageId === message.id && <span className={styles.copiedMark}>Copied</span>}
                    <button
                      type="button"
                      className={styles.messageMenuButton}
                      aria-label={`${message.role === 'user' ? 'Your' : 'Apocrypha'} message actions`}
                      aria-haspopup="menu"
                      aria-expanded={surface?.kind === 'message' && surface.messageId === message.id}
                      onClick={(event) => {
                        const rect = event.currentTarget.getBoundingClientRect();
                        openMessageMenu(
                          message.id,
                          event.currentTarget,
                          { x: rect.right - 16, y: rect.bottom + 6 },
                        );
                      }}
                    >
                      <span aria-hidden="true">•••</span>
                    </button>
                  </div>
                </div>
                <div className={styles.messageText}>{message.text}</div>
                {message.receipt && inspectedMessageId === message.id && (
                  <section className={styles.receipt} aria-label="Verified response receipt">
                    <div className={styles.inspectorHeader}>
                      <strong>Verified response receipt</strong>
                      <button type="button" onClick={() => setInspectedMessageId(null)} aria-label="Close response details">×</button>
                    </div>
                    <dl>
                      <div><dt>Model</dt><dd>{message.receipt.modelId}</dd></div>
                      <div><dt>Response</dt><dd>{message.receipt.responseId}</dd></div>
                      <div><dt>Digest</dt><dd>{message.receipt.responseDigest.slice(0, 16)}…</dd></div>
                      <div><dt>Serving profile</dt><dd>{message.receipt.servingProfileDigest.slice(0, 16)}…</dd></div>
                      <div><dt>Memory</dt><dd>{message.receipt.memoryScope}</dd></div>
                      <div><dt>History</dt><dd>{message.receipt.conversationHistory}</dd></div>
                      {message.receipt.identity && (
                        <>
                          <div><dt>Identity</dt><dd>Apocrypha · governed hybrid</dd></div>
                          <div><dt>Compiler</dt><dd>{message.receipt.identity.compiler_version}</dd></div>
                          <div><dt>Identity digest</dt><dd>{message.receipt.identity.identity_digest.slice(0, 16)}…</dd></div>
                        </>
                      )}
                      {message.receipt.context && (
                        <>
                          <div><dt>Context frame</dt><dd>{message.receipt.context.frame_id}</dd></div>
                          <div><dt>Provenance</dt><dd>{message.receipt.context.provenance_spine_digest.slice(0, 16)}…</dd></div>
                          <div><dt>Retrieval</dt><dd>{message.receipt.context.retrieval.status} · {message.receipt.context.retrieval.count}</dd></div>
                          <div><dt>Memory bank</dt><dd>{message.receipt.context.memory.provider} · {message.receipt.context.memory.status} · {message.receipt.context.memory.records_used} used</dd></div>
                          <div><dt>Capabilities</dt><dd>{message.receipt.context.capabilities.map((capability) => `${capability.id}:${capability.status}`).join(' · ') || 'none'}</dd></div>
                        </>
                      )}
                    </dl>
                  </section>
                )}
                {message.codeEffect && inspectedMessageId === message.id && (
                  <section className={styles.codeReceipt} aria-label="Governed code-effect receipt">
                    <div className={styles.inspectorHeader}>
                      <strong>Governed code-effect receipt</strong>
                      {!message.receipt && <button type="button" onClick={() => setInspectedMessageId(null)} aria-label="Close effect details">×</button>}
                    </div>
                    <dl>
                      <div><dt>State</dt><dd>{message.codeEffect.state}</dd></div>
                      <div><dt>Isolated test</dt><dd>{message.codeEffect.testPassed === true ? 'Passed' : message.codeEffect.testPassed === false ? 'Failed' : 'Not returned'}</dd></div>
                      <div><dt>Exit</dt><dd>{message.codeEffect.testExitCode ?? '—'}</dd></div>
                      <div><dt>Latency</dt><dd>{message.codeEffect.latencyMs === 'restored' ? 'Restored receipt' : `${message.codeEffect.latencyMs} ms`}</dd></div>
                      <div><dt>Proposal</dt><dd>{message.codeEffect.proposalDigest ? `${message.codeEffect.proposalDigest.slice(0, 16)}…` : 'Awaiting proposal'}</dd></div>
                      {message.codeEffect.requestContractDigest && <div><dt>Request</dt><dd>{message.codeEffect.requestContractDigest.slice(0, 16)}…</dd></div>}
                      {message.codeEffect.durableReplay !== undefined && <div><dt>Delivery</dt><dd>{message.codeEffect.durableReplay ? 'Durable replay · no repeated effect' : 'First durable settlement'}</dd></div>}
                      <div><dt>Event</dt><dd>{(message.codeEffect.rollbackEventDigest ?? message.codeEffect.terminalEventDigest ?? message.codeEffect.promotionEventDigest ?? '—').slice(0, 16)}{message.codeEffect.rollbackEventDigest || message.codeEffect.terminalEventDigest || message.codeEffect.promotionEventDigest ? '…' : ''}</dd></div>
                    </dl>
                    <ul aria-label="Admitted code paths">
                      {message.codeEffect.allowedPaths.map((path) => <li key={path}><code>{path}</code></li>)}
                    </ul>
                    {access === 'owner'
                      && message.codeEffect.state === 'PROMOTED'
                      && message.codeEffect.promotionEventDigest && (
                        <button
                          type="button"
                          className={styles.rollbackButton}
                          onClick={() => { void rollbackCodeEffect(message.id, message.codeEffect!.promotionEventDigest!); }}
                          disabled={waiting || Boolean(rollingBackId)}
                        >
                          {rollingBackId === message.id ? 'Rolling back…' : 'Rollback this change'}
                        </button>
                    )}
                  </section>
                )}
                {message.turnState && inspectedMessageId === message.id && (
                  <section className={styles.turnStateReceipt} aria-label={`${message.turnState.state.toLowerCase()} turn evidence`}>
                    <div className={styles.inspectorHeader}>
                      <strong>{message.turnState.state === 'FAILED' ? 'Turn failed without a completed answer' : 'Turn remains pending'}</strong>
                      {!message.receipt && !message.codeEffect && <button type="button" onClick={() => setInspectedMessageId(null)} aria-label="Close turn details">×</button>}
                    </div>
                    <dl>
                      <div><dt>State</dt><dd>{message.turnState.state}</dd></div>
                      <div><dt>Request</dt><dd>{message.turnState.request_id}</dd></div>
                      {message.turnState.failure_code && <div><dt>Failure</dt><dd>{message.turnState.failure_code}</dd></div>}
                      {message.turnState.error_class && <div><dt>Class</dt><dd>{message.turnState.error_class}</dd></div>}
                      {message.turnState.error_digest && <div><dt>Error evidence</dt><dd>{message.turnState.error_digest.slice(0, 16)}…</dd></div>}
                      <div><dt>Recorded</dt><dd>{new Date(message.turnState.recorded_at).toLocaleString()}</dd></div>
                    </dl>
                    <button type="button" className={styles.remediationButton} onClick={() => reopenMessage(message)}>
                      Open as a new attempt
                    </button>
                  </section>
                )}
              </article>
            ))}

            {waiting && (
              <div className={styles.waiting} role="status">
                <span className={styles.waitingMark} aria-hidden="true" />
                {ownerCodeMode ? 'Apocrypha is generating, testing and applying within the confirmed scope…' : 'Apocrypha is thinking…'}
              </div>
            )}

            {error && (
              <div className={styles.error} role="alert">
                <span>{error}</span>
                {pendingTurn && (
                  <button
                    type="button"
                    onClick={() => { void send(pendingTurn); }}
                    disabled={waiting || Boolean(rollingBackId)}
                  >
                    Retry same turn
                  </button>
                )}
              </div>
            )}
            <div ref={endRef} />
          </div>
          {restorationNotice && (
            <p className={styles.srOnly} role="status">{restorationNotice}</p>
          )}

          {authenticated ? (
            <form
              className={styles.composer}
              onSubmit={(event) => {
                event.preventDefault();
                void send();
              }}
            >
              <label className={styles.srOnly} htmlFor="public-apocrypha-message">Message Apocrypha</label>
              <div className={styles.composerShell}>
                <textarea
                  id="public-apocrypha-message"
                  ref={composerRef}
                  value={draft}
                  rows={2}
                  maxLength={MAX_TEXT_BYTES}
                  placeholder={selectedMode.placeholder}
                  disabled={waiting || Boolean(rollingBackId) || !conversationId || !sessionReady}
                  aria-describedby="public-apocrypha-disclosure public-apocrypha-count"
                  onChange={(event) => {
                    setDraft(event.target.value);
                    setCodeApproval(null);
                    if (error && !pendingTurn) setError(null);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
                      event.preventDefault();
                      void send();
                    }
                  }}
                />
                <div className={styles.composerBar}>
                  <div className={styles.composerTools}>
                    <button
                      ref={lensButtonRef}
                      type="button"
                      className={styles.lensButton}
                      aria-haspopup="dialog"
                      aria-expanded={surface?.kind === 'intent'}
                      onClick={(event) => {
                        if (surface?.kind === 'intent') closeSurface(true);
                        else openIntentLens(event.currentTarget);
                      }}
                      disabled={waiting || Boolean(rollingBackId)}
                    >
                      <span className={styles.intentGlyph} aria-hidden="true">{selectedMode.icon}</span>
                      <span><small>{selectedMode.verb}</small><strong>{selectedMode.label}</strong></span>
                      <span className={styles.chevron} aria-hidden="true">⌃</span>
                    </button>
                    {ownerCodeMode && (
                      <button
                        type="button"
                        className={styles.scopeCapsule}
                        data-ready={codeApprovalCurrent}
                        aria-haspopup="dialog"
                        aria-expanded={surface?.kind === 'scope'}
                        onClick={(event) => {
                          if (surface?.kind === 'scope') {
                            closeSurface(true);
                            return;
                          }
                          const rect = event.currentTarget.getBoundingClientRect();
                          surfaceTriggerRef.current = event.currentTarget;
                          const anchor = clampAboveSurfaceAnchor(
                            rect.left,
                            rect.top - 10,
                            540,
                            460,
                          );
                          setSurface({ kind: 'scope', ...anchor });
                        }}
                      >
                        <span aria-hidden="true">◎</span> {codeScopeState}
                      </button>
                    )}
                  </div>
                  <div className={styles.sendCluster}>
                    <span
                      id="public-apocrypha-count"
                      className={styles.byteCount}
                      data-visible={currentBytes > MAX_TEXT_BYTES * 0.7}
                    >
                      {currentBytes.toLocaleString()} / {MAX_TEXT_BYTES.toLocaleString()} bytes
                    </span>
                    <button
                      type="submit"
                      className={styles.sendButton}
                      aria-label={waiting ? 'Waiting for Apocrypha' : ownerCodeMode ? 'Run the confirmed governed code effect' : 'Send message'}
                      disabled={
                        waiting
                        || Boolean(rollingBackId)
                        || !conversationId
                        || !draft.trim()
                        || currentBytes > MAX_TEXT_BYTES
                        || (ownerCodeMode && (
                          !codeApprovalCurrent
                          || codePaths.length < 1
                          || codePaths.length > 32
                          || duplicateCodePath
                        ))
                      }
                    >
                      <span>{waiting ? 'In motion' : selectedMode.dispatch}</span>
                      <strong aria-hidden="true">↑</strong>
                    </button>
                  </div>
                </div>
              </div>
              <p id="public-apocrypha-disclosure" className={styles.srOnly}>
                {ownerCodeMode
                  ? 'Owner Code mode uses the governed runtime only after exact path scope and one-run confirmation.'
                  : 'Governed retrieval and read-only context may be used. Workspace and external effects require separate owner confirmation.'}
              </p>
            </form>
          ) : (
            <div className={styles.accessGate} role="status">
              <div>
                <strong>
                  {access === 'unavailable'
                    ? 'Sign-in verification is temporarily unavailable.'
                    : access === 'checking'
                      ? 'Checking your sign-in…'
                      : 'Sign in to begin a restricted member turn.'}
                </strong>
                <span>No message is sent until the session is verified.</span>
              </div>
              {access !== 'checking' && access !== 'unavailable' && (
                <Link href="/login?next=%2Fapocrypha">Sign in</Link>
              )}
            </div>
          )}
        </section>

        {surface?.kind === 'intent' && (
          <div
            ref={surfaceRef}
            className={`${styles.floatingSurface} ${styles.intentPalette}`}
            data-placement={surface.placement}
            style={surfaceStyle(surface)}
            role="dialog"
            aria-modal="false"
            aria-labelledby="public-apocrypha-intent-title"
          >
            <div className={styles.paletteHeader}>
              <div><span aria-hidden="true">✦</span><strong id="public-apocrypha-intent-title">Approach constellation</strong></div>
              <kbd>Ctrl K</kbd>
            </div>
            <div className={styles.intentGrid}>
              {GENERATIVE_MODES.map((candidate) => (
                <button
                  key={candidate.id}
                  type="button"
                  className={candidate.id === mode ? styles.activeIntent : undefined}
                  aria-pressed={candidate.id === mode}
                  onClick={() => selectMode(candidate)}
                  disabled={waiting || Boolean(rollingBackId)}
                >
                  <span className={styles.intentIcon} aria-hidden="true">{candidate.icon}</span>
                  <span>
                    <strong>{candidate.label}</strong>
                    <small>{candidate.description}</small>
                  </span>
                  <span className={styles.intentAxis}>
                    <b>{candidate.verb}</b>
                    {candidate.id === 'code' && access === 'owner' && <em>effect</em>}
                  </span>
                </button>
              ))}
            </div>
            <div className={styles.paletteUtilities}>
              <button
                type="button"
                onClick={() => {
                  closeSurface();
                  requestAnimationFrame(() => composerRef.current?.focus());
                }}
              >
                Focus the composer <span aria-hidden="true">↵</span>
              </button>
              <button
                type="button"
                onClick={newConversation}
                disabled={waiting || Boolean(rollingBackId) || !conversationId}
              >
                New conversation <span aria-hidden="true">＋</span>
              </button>
            </div>
            <p>
              <strong>{selectedMode.label}</strong> is a prompt frame: {selectedMode.frame}.
              {' '}It does not claim a separate faculty route.
              {selectedMode.typedRoute && access === 'owner' ? ` ${selectedMode.typedRoute}` : ''}
            </p>
          </div>
        )}

        {surface?.kind === 'conversation' && (
          <div
            ref={surfaceRef}
            className={`${styles.floatingSurface} ${styles.conversationMenu}`}
            style={surfaceStyle(surface)}
            role="dialog"
            aria-modal="false"
            aria-labelledby="public-apocrypha-conversation-menu-title"
          >
            <div className={styles.menuTitle}>
              <span className={styles.sessionPulse} data-state={authenticated ? 'ready' : 'closed'} aria-hidden="true" />
              <div><strong id="public-apocrypha-conversation-menu-title">{conversationTitle}</strong><small>{sessionLabel}</small></div>
            </div>
            <button
              type="button"
              onClick={newConversation}
              disabled={waiting || Boolean(rollingBackId) || !conversationId}
            >
              <span aria-hidden="true">＋</span><span><strong>New conversation</strong><small>Open a clean canvas</small></span>
            </button>
            {authenticated && sessionSummaries.length > 0 && (
              <div className={styles.recentThreads} aria-label="Recent durable conversations">
                <div className={styles.recentThreadsLabel}>
                  <span>Worldlines</span><small>{sessionSummaries.length} durable</small>
                </div>
                {sessionSummaries.map((session) => (
                  <button
                    type="button"
                    key={session.session_id}
                    className={styles.threadChoice}
                    data-active={session.session_id === conversationId}
                    onClick={() => { void openDurableSession(session.session_id); }}
                    disabled={waiting || Boolean(rollingBackId) || sessionLoading}
                  >
                    <span className={styles.threadOrbit} aria-hidden="true" />
                    <span>
                      <strong>{session.title}</strong>
                      <small>
                        {session.message_count} message{session.message_count === 1 ? '' : 's'}
                        {session.active_job_count > 0 ? ` · ${session.active_job_count} active` : ''}
                      </small>
                    </span>
                  </button>
                ))}
              </div>
            )}
            {worldlineJobs.length > 0 && (
              <div className={styles.worldObjects} aria-label="Background work in this worldline">
                <div className={styles.recentThreadsLabel}>
                  <span>Orbiting work</span><small>Background tasks</small>
                </div>
                {worldlineJobs.slice(-4).map((job, index) => {
                  const jobId = stringValue(job.job_id) ?? `job-${index}`;
                  const actionId = stringValue(job.action_id) ?? stringValue(job.kind);
                  const action = actionId === 'objective.proposal_council.v1'
                    ? 'Proposal council'
                    : actionId ?? 'Background task';
                  const state = stringValue(job.state) ?? 'RECORDED';
                  return (
                    <div className={styles.worldObject} key={jobId}>
                      <span className={styles.threadOrbit} data-state={state.toLowerCase()} aria-hidden="true" />
                      <span><strong>{action}</strong><small>{state}</small></span>
                    </div>
                  );
                })}
              </div>
            )}
            {worldlineArtifacts.length > 0 && (
              <div className={styles.worldObjects} aria-label="Artifacts in this worldline">
                <div className={styles.recentThreadsLabel}>
                  <span>Made here</span><small>Artifacts</small>
                </div>
                {worldlineArtifacts.slice(-4).map((artifact, index) => {
                  const artifactId = stringValue(artifact.artifact_id) ?? `artifact-${index}`;
                  const title = stringValue(artifact.title) ?? stringValue(artifact.kind) ?? 'Conversation artifact';
                  const state = stringValue(artifact.state) ?? stringValue(artifact.status) ?? 'Recorded';
                  return (
                    <div className={styles.worldObject} key={artifactId}>
                      <span aria-hidden="true">◇</span>
                      <span><strong>{title}</strong><small>{state}</small></span>
                    </div>
                  );
                })}
              </div>
            )}
            <button
              type="button"
              onClick={() => {
                const trigger = lensButtonRef.current;
                if (trigger) openIntentLens(trigger);
              }}
            >
              <span aria-hidden="true">✦</span><span><strong>Change approach</strong><small>Open the faculty constellation</small></span>
            </button>
            {currentSessionRecorded && (
              <button
                type="button"
                className={styles.deleteWorldline}
                onClick={() => { void deleteCurrentSession(); }}
                disabled={deletingSession || waiting || Boolean(rollingBackId)}
              >
                <span aria-hidden="true">⌫</span><span><strong>{deletingSession ? 'Archiving worldline…' : 'Archive this worldline…'}</strong><small>Remove from active worldlines; audit-ledger rows remain</small></span>
              </button>
            )}
            <div className={styles.menuFacts} aria-label="Privacy and response details">
              <span><small>History</small><strong>{historyLabel}</strong></span>
              <span><small>Work</small><strong>{worldState.active_job_count > 0 ? `${worldState.active_job_count} active` : 'At rest'}</strong></span>
              <span><small>Training</small><strong>Off</strong></span>
              <span><small>Effects</small><strong>{worldState.effect_count > 0 ? `${worldState.effect_count} evidenced` : ownerCodeMode ? 'One-run airlock' : 'None'}</strong></span>
              <span><small>Faculty</small><strong>{lastModel ?? 'Verified per response'}</strong></span>
            </div>
          </div>
        )}

        {surface?.kind === 'message' && contextualMessage && (
          <div
            ref={surfaceRef}
            className={`${styles.floatingSurface} ${styles.messageContextMenu}`}
            style={surfaceStyle(surface)}
            role="menu"
            aria-label="Message actions"
            onKeyDown={navigateMessageMenu}
          >
            <button type="button" role="menuitem" onClick={() => { void copyMessage(contextualMessage); }}>
              <span aria-hidden="true">⧉</span> Copy message
            </button>
            <button type="button" role="menuitem" onClick={() => reopenMessage(contextualMessage)}>
              <span aria-hidden="true">↗</span>{' '}
              {contextualMessage.turnState ? 'Open as a new attempt' : 'Continue from here'}
            </button>
            {(contextualMessage.receipt || contextualMessage.codeEffect || contextualMessage.turnState) && (
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setInspectedMessageId(contextualMessage.id);
                  closeSurface(true);
                }}
              >
                <span aria-hidden="true">◎</span> Inspect evidence
              </button>
            )}
            {access === 'owner'
              && contextualMessage.codeEffect?.state === 'PROMOTED'
              && contextualMessage.codeEffect.promotionEventDigest && (
              <button
                type="button"
                role="menuitem"
                className={styles.dangerAction}
                onClick={() => {
                  const confirmed = window.confirm('Roll back this promoted change to its recorded prestate?');
                  closeSurface(true);
                  if (confirmed) {
                    void rollbackCodeEffect(
                      contextualMessage.id,
                      contextualMessage.codeEffect!.promotionEventDigest!,
                    );
                  }
                }}
              >
                <span aria-hidden="true">↶</span> Roll back change…
              </button>
            )}
          </div>
        )}

        {surface?.kind === 'scope' && ownerCodeMode && (
          <section
            ref={surfaceRef}
            className={`${styles.floatingSurface} ${styles.scopeSheet}`}
            style={surfaceStyle(surface)}
            role="dialog"
            aria-modal="false"
            aria-labelledby="public-apocrypha-scope-title"
            aria-describedby="public-apocrypha-scope-description"
          >
            <div className={styles.scopeHeader}>
              <div><span aria-hidden="true">◎</span><span><strong id="public-apocrypha-scope-title">Effect airlock</strong><small>Owner-governed code boundary</small></span></div>
              <button type="button" onClick={() => closeSurface(true)} aria-label="Close effect scope">×</button>
            </div>
            <p id="public-apocrypha-scope-description">Only the exact repository paths admitted here may cross into change. Apocrypha isolates and tests the proposal before one apply.</p>
            <label htmlFor="public-apocrypha-code-paths">Allowed repository paths</label>
            <textarea
              id="public-apocrypha-code-paths"
              value={codePathInput}
              rows={4}
              spellCheck={false}
              placeholder={'src/apocv4/example.py\ntests/test_example.py'}
              disabled={waiting || Boolean(rollingBackId)}
              aria-describedby="public-apocrypha-code-path-status"
              aria-invalid={duplicateCodePath || codePaths.length > 32}
              onChange={(event) => {
                setCodePathInput(event.target.value);
                setCodeApproval(null);
              }}
            />
            <div id="public-apocrypha-code-path-status" className={styles.codeScopeMeta} aria-live="polite">
              <span>{codePaths.length}/32 paths admitted</span>
              {duplicateCodePath && <strong>Each path must be unique.</strong>}
            </div>
            <label className={styles.codeConfirm}>
              <input
                type="checkbox"
                checked={codeApprovalCurrent}
                disabled={
                  waiting
                  || Boolean(rollingBackId)
                  || !subjectKey
                  || !conversationId
                  || !draft.trim()
                  || codePaths.length < 1
                  || codePaths.length > 32
                  || duplicateCodePath
                }
                onChange={(event) => {
                  if (!event.target.checked) {
                    setCodeApproval(null);
                    return;
                  }
                  if (!subjectKey || !conversationId || !currentApprovalBinding || !draft.trim()) {
                    setCodeApproval(null);
                    return;
                  }
                  setCodeApproval({
                    binding: currentApprovalBinding,
                    authGeneration: authGenerationRef.current,
                    subjectKey,
                    conversationId,
                    objective: draft.trim(),
                    allowedPaths: [...codePaths],
                  });
                }}
              />
              <span>Authorize one isolated, tested apply on this scope. No automatic retry.</span>
            </label>
            <button type="button" className={styles.scopeDone} onClick={() => closeSurface(true)}>
              {codeApprovalCurrent ? 'Scope ready' : 'Keep scope'}
            </button>
          </section>
        )}
      </main>
    </div>
  );
}
