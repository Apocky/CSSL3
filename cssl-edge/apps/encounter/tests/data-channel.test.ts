import assert from "node:assert/strict";
import test from "node:test";

import {
  REALTIME_EVENT_TYPES,
  RealtimeEventSchema,
} from "@apocky/contracts";

import {
  decodeRealtimeEvent,
  ENCOUNTER_DATA_TOPIC,
  encodeRealtimeEvent,
  hasForbiddenRealtimeMaterial,
} from "../lib/realtime";

const validEvent = RealtimeEventSchema.parse({
  type: "speech.state",
  eventId: "67f39c45-dfe3-42d1-96ac-6064d3f1046e",
  sessionId: "56d85b0d-3ddb-4fd5-8685-3da993fd03c6",
  sentAt: "2026-07-25T08:00:00.000Z",
  sender: "principal:owner",
  state: "started",
});

const realtimeAuthority = {
  expectedSessionId: validEvent.sessionId,
  authenticatedParticipantIdentity: validEvent.sender,
};

test("the data channel round-trips only canonical encounter events", () => {
  const encoded = encodeRealtimeEvent(validEvent);
  assert.deepEqual(
    decodeRealtimeEvent(encoded, ENCOUNTER_DATA_TOPIC, realtimeAuthority),
    validEvent,
  );
});

test("an unapproved topic fails closed", () => {
  const encoded = encodeRealtimeEvent(validEvent);
  assert.throws(
    () => decodeRealtimeEvent(encoded, "debug.telemetry", realtimeAuthority),
    /unapproved topic/,
  );
});

test("cross-session and cross-participant envelope spoofing fails closed", () => {
  const encoded = encodeRealtimeEvent(validEvent);
  assert.throws(
    () =>
      decodeRealtimeEvent(encoded, ENCOUNTER_DATA_TOPIC, {
        ...realtimeAuthority,
        expectedSessionId: "67f39c45-dfe3-42d1-96ac-6064d3f1046e",
      }),
    /another encounter/,
  );
  assert.throws(
    () =>
      decodeRealtimeEvent(encoded, ENCOUNTER_DATA_TOPIC, {
        ...realtimeAuthority,
        authenticatedParticipantIdentity: "principal:impostor",
      }),
    /authenticated participant/,
  );
});

test("nested caption identity and session spoofing fails before publication", () => {
  const captionBase = {
    type: "caption",
    eventId: "b878c750-8805-4f90-8816-b760bcad09ab",
    sessionId: validEvent.sessionId,
    sentAt: validEvent.sentAt,
    sender: validEvent.sender,
    transcript: {
      eventId: "41a4e4a9-6bdf-4964-9fbb-f2f074c7b26f",
      sessionId: validEvent.sessionId,
      speaker: validEvent.sender,
      sequence: 1,
      status: "final",
      text: "Bound to the authenticated speaker.",
      startedAtMs: 1,
      endedAtMs: 2,
      confidence: null,
      provenance: {
        source: "participant-text",
        modelDigest: null,
      },
    },
  } as const;
  assert.throws(() =>
    encodeRealtimeEvent({
      ...captionBase,
      transcript: {
        ...captionBase.transcript,
        speaker: "principal:impostor",
      },
    }),
  );
  assert.throws(() =>
    encodeRealtimeEvent({
      ...captionBase,
      transcript: {
        ...captionBase.transcript,
        sessionId: "67f39c45-dfe3-42d1-96ac-6064d3f1046e",
      },
    }),
  );
});

test("private diagnostic, tool, telemetry, memory, and camera material is denied", () => {
  const forbidden = [
    { diagnostics: { trace: "x" } },
    { tool_call: "state.read" },
    { telemetry: { cost: 1 } },
    { memory_internal: "private" },
    { camera_metadata: { device: "secret" } },
    { raw_audio: "bytes" },
    { frame_bytes: "bytes" },
  ];
  for (const value of forbidden) {
    assert.equal(hasForbiddenRealtimeMaterial(value), true);
    assert.throws(() => encodeRealtimeEvent(value), /forbidden/);
  }
});

test("the shared event taxonomy contains no forbidden channel type", () => {
  const joined = REALTIME_EVENT_TYPES.join(" ");
  assert.doesNotMatch(
    joined,
    /diagnostic|tool|telemetry|memory|camera|raw/i,
  );
});
