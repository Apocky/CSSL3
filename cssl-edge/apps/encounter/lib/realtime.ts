import {
  RealtimeEventSchema,
  type RealtimeEvent,
  type TranscriptEvent,
} from "@apocky/contracts";

export const ENCOUNTER_DATA_TOPIC = "apocky.encounter.events.v1";
export const MAX_REALTIME_EVENT_BYTES = 64 * 1024;

const FORBIDDEN_KEY_PARTS = [
  "diagnostic",
  "tool",
  "telemetry",
  "memory",
  "camerametadata",
  "rawaudio",
  "rawvideo",
  "framebytes",
  "contentb64",
] as const;

function normalizedKey(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]/g, "");
}

export function hasForbiddenRealtimeMaterial(value: unknown): boolean {
  if (Array.isArray(value)) {
    return value.some(hasForbiddenRealtimeMaterial);
  }
  if (value === null || typeof value !== "object") {
    return false;
  }
  for (const [key, child] of Object.entries(value)) {
    const normalized = normalizedKey(key);
    if (FORBIDDEN_KEY_PARTS.some((part) => normalized.includes(part))) {
      return true;
    }
    if (hasForbiddenRealtimeMaterial(child)) {
      return true;
    }
  }
  return false;
}

export function encodeRealtimeEvent(event: unknown): Uint8Array {
  if (hasForbiddenRealtimeMaterial(event)) {
    throw new Error("Realtime event contains a forbidden private-data field.");
  }
  const parsed = RealtimeEventSchema.parse(event);
  const bytes = new TextEncoder().encode(JSON.stringify(parsed));
  if (bytes.byteLength > MAX_REALTIME_EVENT_BYTES) {
    throw new Error("Realtime event exceeds the bounded data-channel size.");
  }
  return bytes;
}

export function decodeRealtimeEvent(
  payload: Uint8Array,
  topic: string | undefined,
  authority: {
    expectedSessionId: string;
    authenticatedParticipantIdentity: string;
  },
): RealtimeEvent {
  if (topic !== ENCOUNTER_DATA_TOPIC) {
    throw new Error("Realtime event arrived on an unapproved topic.");
  }
  if (payload.byteLength === 0 || payload.byteLength > MAX_REALTIME_EVENT_BYTES) {
    throw new Error("Realtime event has an invalid byte length.");
  }
  const decoded = new TextDecoder("utf-8", { fatal: true }).decode(payload);
  const value: unknown = JSON.parse(decoded);
  if (hasForbiddenRealtimeMaterial(value)) {
    throw new Error("Realtime event contains a forbidden private-data field.");
  }
  const event = RealtimeEventSchema.parse(value);
  if (event.sessionId !== authority.expectedSessionId) {
    throw new Error("Realtime event belongs to another encounter.");
  }
  if (event.sender !== authority.authenticatedParticipantIdentity) {
    throw new Error(
      "Realtime event sender does not match the authenticated participant.",
    );
  }
  return event;
}

export interface TypedCaptionInput {
  sessionId: string;
  sender: string;
  text: string;
  sequence: number;
  sentAt?: string;
  eventId?: string;
  transcriptId?: string;
}

export function createTypedCaptionEvent(
  input: TypedCaptionInput,
): RealtimeEvent {
  const timestamp = input.sentAt ?? new Date().toISOString();
  const transcript: TranscriptEvent = {
    eventId: input.transcriptId ?? crypto.randomUUID(),
    sessionId: input.sessionId,
    speaker: input.sender,
    sequence: input.sequence,
    status: "final",
    text: input.text,
    startedAtMs: Date.now(),
    endedAtMs: Date.now(),
    confidence: null,
    provenance: {
      source: "participant-text",
      modelDigest: null,
    },
  };
  return RealtimeEventSchema.parse({
    type: "caption",
    eventId: input.eventId ?? crypto.randomUUID(),
    sessionId: input.sessionId,
    sentAt: timestamp,
    sender: input.sender,
    transcript,
  });
}
