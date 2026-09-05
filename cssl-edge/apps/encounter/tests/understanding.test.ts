import assert from "node:assert/strict";
import test from "node:test";

import {
  UnderstandingAcknowledgementSchema,
  UnderstandingVersionSchema,
} from "@apocky/contracts";

import {
  assertCorrectionTransition,
  deriveUnderstandingState,
  outcomeOnExit,
} from "../lib/understanding";

const sessionId = "f291a71f-40aa-405b-9670-c35a5fe039d4";
const digest = `sha256:${"a".repeat(64)}`;
const participants = [
  "principal:owner",
  "principal:apocrypha",
] as const;
const signature = {
  algorithm: "Ed25519" as const,
  keyId: "key:participant",
  value: "A".repeat(43),
};

const version = UnderstandingVersionSchema.parse({
  versionId: "28e81e8f-826a-45ae-b0b3-91145ee24d05",
  sessionId,
  version: 1,
  interpretations: [
    { participant: participants[0], interpretation: "I heard you clearly." },
    { participant: participants[1], interpretation: "I understand the request." },
  ],
  transcriptRefs: [],
  unresolvedPoints: [],
  createdAt: "2026-07-25T08:00:00.000Z",
  createdBy: participants[0],
  canonicalDigest: digest,
});

function acknowledgement(
  participant: string,
  status: "understood" | "needs_repair" | "disagree",
  index: number,
) {
  return UnderstandingAcknowledgementSchema.parse({
    acknowledgementId:
      index === 0
        ? "67963f18-0766-491a-bf8f-09c1504a4a92"
        : "c65d21f7-c44c-44a0-8e8b-c1a55a9144c7",
    sessionId,
    participant,
    versionDigest: digest,
    status,
    correction: status === "understood" ? null : "This point needs repair.",
    acknowledgedAt: `2026-07-25T08:00:0${index}.000Z`,
    signature,
  });
}

test("mutual understanding requires both participants on one exact digest", () => {
  const one = deriveUnderstandingState(version, participants, [
    acknowledgement(participants[0], "understood", 0),
  ]);
  assert.equal(one.outcome, "pending");

  const both = deriveUnderstandingState(version, participants, [
    acknowledgement(participants[0], "understood", 0),
    acknowledgement(participants[1], "understood", 1),
  ]);
  assert.equal(both.outcome, "mutually_understood");
  assert.equal(outcomeOnExit(both), "mutually_understood");
});

test("repair and unilateral exit cannot claim mutual understanding", () => {
  const state = deriveUnderstandingState(version, participants, [
    acknowledgement(participants[0], "needs_repair", 0),
    acknowledgement(participants[1], "understood", 1),
  ]);
  assert.equal(state.outcome, "needs_repair");
  assert.equal(outcomeOnExit(state), "ended_unresolved");
  assert.equal(outcomeOnExit(null), "ended_unresolved");
});

test("a correction is a new sequential version and digest", () => {
  const corrected = UnderstandingVersionSchema.parse({
    ...version,
    versionId: "1ce90ba4-10bc-4b15-85c4-b98999dce66b",
    version: 2,
    canonicalDigest: `sha256:${"b".repeat(64)}`,
  });
  assert.doesNotThrow(() => assertCorrectionTransition(version, corrected));
  assert.throws(
    () => assertCorrectionTransition(version, { ...corrected, version: 3 }),
    /next version/,
  );
});
