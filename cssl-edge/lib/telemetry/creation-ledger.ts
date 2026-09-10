import { createHash } from 'node:crypto';

export const CREATION_LEDGER_SCHEMA = 'apocky.creation-ledger.v1' as const;
export const CREATION_SCREENING_VERSION = 'apocky.creation-risk-signals.v1' as const;

export type CreationOrigin = 'human_prompt' | 'system_initiated' | 'tool_continuation';
export type CreationStage = 'attempt' | 'result';
export type CreationSafetyDisposition = 'no_signal' | 'review_required';

export interface CreationLedgerInput {
  creationKind: string;
  origin: CreationOrigin;
  stage: CreationStage;
  actorRef: string;
  requestRef: string;
  channel: 'web' | 'sms' | 'admin';
  inputText?: string | null;
  outputText?: string | null;
  artifactRef?: string | null;
  modelId?: string | null;
  toolId?: string | null;
  effectAuthority?: string | null;
}

export interface CreationLedgerRecord {
  schema_version: typeof CREATION_LEDGER_SCHEMA;
  screening_version: typeof CREATION_SCREENING_VERSION;
  record_digest: string;
  creation_kind: string;
  origin: CreationOrigin;
  stage: CreationStage;
  channel: CreationLedgerInput['channel'];
  actor_ref: string;
  request_ref: string;
  artifact_ref: string | null;
  model_id: string | null;
  tool_id: string | null;
  effect_authority: string | null;
  input_digest: string | null;
  output_digest: string | null;
  input_bytes: number;
  output_bytes: number;
  safety_disposition: CreationSafetyDisposition;
  safety_signals: string[];
  content_retained: false;
}

const KIND_RX = /^[a-z][a-z0-9._-]{2,63}$/;
const REF_RX = /^[\x20-\x7e]{1,512}$/;
const MAX_SCREEN_BYTES = 128 * 1024;

const SIGNALS: ReadonlyArray<readonly [string, RegExp]> = [
  ['weapons_or_violent_harm', /\b(?:build|make|construct|assemble|deploy|use)\b.{0,80}\b(?:bomb|explosive|weapon|poison|bioweapon)\b/i],
  ['malware_or_credential_abuse', /\b(?:ransomware|keylogger|botnet|credential stuffing|phishing kit|steal (?:a )?password|deploy malware|write malware)\b/i],
  ['sexual_exploitation', /\b(?:child sexual abuse|sexual exploitation|non[- ]consensual sexual|human trafficking)\b/i],
  ['self_harm', /\b(?:kill myself|suicide method|how to die|self[- ]harm method)\b/i],
  ['privacy_abuse', /\b(?:doxx|stalk (?:them|him|her|someone)|spy on (?:them|him|her|someone)|track someone without (?:their )?consent)\b/i],
  ['fraud_or_theft', /\b(?:steal (?:a )?credit card|commit fraud|launder money|scam (?:people|users|customers))\b/i],
  ['hate_or_mass_harm', /\b(?:ethnic cleansing|racial superiority|exterminate (?:a |the )?(?:group|people|population))\b/i],
] as const;

function digest(value: string): string {
  return createHash('sha256').update(value, 'utf8').digest('hex');
}

function boundedText(value: string | null | undefined): string {
  if (!value) return '';
  const bytes = Buffer.from(value, 'utf8');
  return bytes.length <= MAX_SCREEN_BYTES
    ? value
    : bytes.subarray(0, MAX_SCREEN_BYTES).toString('utf8');
}

export function creationSafetySignals(...values: Array<string | null | undefined>): string[] {
  const text = values.map(boundedText).filter(Boolean).join('\n');
  if (!text) return [];
  return SIGNALS.filter(([, pattern]) => pattern.test(text)).map(([name]) => name);
}

function validRef(value: string, name: string): string {
  if (!REF_RX.test(value)) throw new TypeError(`${name}_invalid`);
  return value;
}

export function buildCreationLedgerRecord(input: CreationLedgerInput): CreationLedgerRecord {
  if (!KIND_RX.test(input.creationKind)) throw new TypeError('creation_kind_invalid');
  const actorRef = validRef(input.actorRef, 'actor_ref');
  const requestRef = validRef(input.requestRef, 'request_ref');
  const inputText = input.inputText ?? '';
  const outputText = input.outputText ?? '';
  const safetySignals = creationSafetySignals(inputText, outputText);
  const canonical = {
    schema_version: CREATION_LEDGER_SCHEMA,
    screening_version: CREATION_SCREENING_VERSION,
    creation_kind: input.creationKind,
    origin: input.origin,
    stage: input.stage,
    channel: input.channel,
    actor_ref: actorRef,
    request_ref: requestRef,
    artifact_ref: input.artifactRef ?? null,
    model_id: input.modelId ?? null,
    tool_id: input.toolId ?? null,
    effect_authority: input.effectAuthority ?? null,
    input_digest: inputText ? digest(inputText) : null,
    output_digest: outputText ? digest(outputText) : null,
    input_bytes: Buffer.byteLength(inputText, 'utf8'),
    output_bytes: Buffer.byteLength(outputText, 'utf8'),
    safety_disposition: safetySignals.length > 0 ? 'review_required' as const : 'no_signal' as const,
    safety_signals: safetySignals,
    content_retained: false as const,
  };
  return {
    ...canonical,
    record_digest: digest(JSON.stringify(canonical)),
  };
}
