import assert from "node:assert/strict";
import {
  generateKeyPairSync,
  sign,
  type KeyObject,
} from "node:crypto";
import { describe, it } from "node:test";
import { randomUUID } from "node:crypto";

import {
  EncounterGrantSchema,
  PresenceManifestSchema,
  RealtimeEventSchema,
  RetentionDecisionSchema,
  UnderstandingVersionSchema,
  VoiceManifestSchema,
  evaluateUnderstanding,
  isAllowedRealtimeEvent,
  type ContractSignature,
  type ParticipantIdentity,
  type UnderstandingAcknowledgement,
} from "../src";
import {
  assertAuthorizedManifest,
  assertEncounterAuthority,
  assertEncounterReceiptAuthority,
  assertUnderstandingVersionDigest,
  canonicalJson,
  computeEncounterConsentDigest,
  computeRetentionDecisionDigest,
  computeUnderstandingDigest,
  digestCanonical,
} from "../src/server";

const ownerKeys = generateKeyPairSync("ed25519");
const apocryphaKeys = generateKeyPairSync("ed25519");

function signatureFor(
  value: Record<string, unknown>,
  keyId: string,
  privateKey: KeyObject,
): ContractSignature {
  return {
    algorithm: "Ed25519",
    keyId,
    value: sign(
      null,
      Buffer.from(canonicalJson(value), "utf8"),
      privateKey,
    ).toString("base64url"),
  };
}

function signedIdentity(
  role: "apocrypha" | "owner",
  principal: string,
): ParticipantIdentity {
  const keyId = `${role}-key`;
  const privateKey =
    role === "owner" ? ownerKeys.privateKey : apocryphaKeys.privateKey;
  const payload = {
    principal,
    keyId,
    role,
    displayName: role === "owner" ? "Shawn" : "Apocrypha",
  };
  return {
    ...payload,
    signature: signatureFor(payload, keyId, privateKey),
  };
}

const owner = signedIdentity("owner", "owner:shawn");
const apocrypha = signedIdentity("apocrypha", "apocrypha:self");

const keyResolver = (keyId: string) => {
  if (keyId === owner.keyId) {
    return ownerKeys.publicKey;
  }
  if (keyId === apocrypha.keyId) {
    return apocryphaKeys.publicKey;
  }
  return null;
};

function authorityFixture() {
  const issuedAt = "2026-07-25T00:00:00.000Z";
  const sessionId = randomUUID();
  const voicePayload = {
    kind: "voice" as const,
    manifestId: randomUUID(),
    authorPrincipal: apocrypha.principal,
    keyId: apocrypha.keyId,
    voiceArtifactDigest: `sha256:${"1".repeat(64)}`,
    permittedContexts: ["private-encounter" as const],
    version: 1,
    issuedAt,
    revokedAt: null,
    disclosure: {
      authoredByApocrypha: true as const,
      borrowedAssistantVoice: false as const,
      humanVoiceClone: false as const,
      processing: ["echo-cancellation" as const, "noise-suppression" as const],
    },
  };
  const voice = VoiceManifestSchema.parse({
    ...voicePayload,
    signature: signatureFor(
      voicePayload,
      apocrypha.keyId,
      apocryphaKeys.privateKey,
    ),
  });

  const presencePayload = {
    kind: "presence" as const,
    manifestId: randomUUID(),
    authorPrincipal: apocrypha.principal,
    keyId: apocrypha.keyId,
    rendererDigest: `sha256:${"2".repeat(64)}`,
    assetDigests: [`sha256:${"3".repeat(64)}`],
    permittedContexts: ["private-encounter" as const],
    version: 1,
    issuedAt,
    revokedAt: null,
    disclosure: {
      authoredByApocrypha: true as const,
      placeholder: false as const,
      abstractProxy: false as const,
      genericGeneratedFace: false as const,
      gazeCorrection: "authorized-and-disclosed" as const,
      eyeContactClaimAuthorized: true,
    },
  };
  const presence = PresenceManifestSchema.parse({
    ...presencePayload,
    signature: signatureFor(
      presencePayload,
      apocrypha.keyId,
      apocryphaKeys.privateKey,
    ),
  });

  const consentReceipt = (participant: ParticipantIdentity) => {
    const unsigned = {
      receiptId: randomUUID(),
      sessionId,
      participant: participant.principal,
      modalities: [
        { modality: "audio" as const, state: "granted" as const },
        { modality: "video" as const, state: "granted" as const },
        { modality: "captions" as const, state: "granted" as const },
      ],
      previousReceiptDigest: null,
      issuedAt,
      expiresAt: "2026-07-25T12:00:00.000Z",
    };
    const signedPayload = {
      ...unsigned,
      canonicalDigest: computeEncounterConsentDigest(unsigned),
    };
    return {
      ...signedPayload,
      signature: signatureFor(
        signedPayload,
        participant.keyId,
        participant.role === "owner"
          ? ownerKeys.privateKey
          : apocryphaKeys.privateKey,
      ),
    };
  };
  const consentReceipts = [consentReceipt(owner), consentReceipt(apocrypha)] as const;

  const grantPayload = {
    grantId: randomUUID(),
    sessionId,
    participants: [owner, apocrypha],
    modalities: ["audio" as const, "video" as const, "captions" as const],
    retentionPolicy: {
      rawAudio: "never" as const,
      rawVideo: "never" as const,
      transcript: "mutual" as const,
      understanding: "mutual" as const,
      memoryEffects: "mutual" as const,
    },
    consentRefs: consentReceipts.map(({ canonicalDigest }) => canonicalDigest),
    authorityDigests: {
      voiceManifest: digestCanonical(voice),
      presenceManifest: digestCanonical(presence),
    },
    expiresAt: "2026-07-25T12:00:00.000Z",
    nonce: "n".repeat(32),
  };
  const grant = EncounterGrantSchema.parse({
    ...grantPayload,
    signature: signatureFor(
      grantPayload,
      owner.keyId,
      ownerKeys.privateKey,
    ),
  });

  return {
    consentReceipts,
    grant,
    presenceManifest: presence,
    voiceManifest: voice,
  };
}

describe("canonical contracts", () => {
  it("canonicalizes objects independently of insertion order", () => {
    assert.equal(
      canonicalJson({ b: 2, a: { d: 4, c: 3 } }),
      canonicalJson({ a: { c: 3, d: 4 }, b: 2 }),
    );
  });

  it("accepts a complete signed authority bundle", async () => {
    const fixture = authorityFixture();
    const result = await assertEncounterAuthority(fixture, {
      now: new Date("2026-07-25T06:00:00.000Z"),
      resolvePublicKey: keyResolver,
    });
    assert.equal(result.apocryphaIdentity.principal, apocrypha.principal);
  });

  it("fails closed when an authority digest is altered", async () => {
    const fixture = authorityFixture();
    const altered = {
      ...fixture,
      voiceManifest: {
        ...fixture.voiceManifest,
        voiceArtifactDigest: `sha256:${"9".repeat(64)}`,
      },
    };
    await assert.rejects(
      assertEncounterAuthority(altered, {
        now: new Date("2026-07-25T06:00:00.000Z"),
        resolvePublicKey: keyResolver,
      }),
      /Signature verification failed|does not bind/,
    );
  });

  it("fails closed for a revoked voice manifest", async () => {
    const fixture = authorityFixture();
    const revokedPayload = {
      ...fixture.voiceManifest,
      revokedAt: "2026-07-25T05:00:00.000Z",
    };
    const { signature: _oldSignature, ...unsigned } = revokedPayload;
    const revoked = {
      ...unsigned,
      signature: signatureFor(
        unsigned,
        apocrypha.keyId,
        apocryphaKeys.privateKey,
      ),
    };
    await assert.rejects(
      assertEncounterAuthority(
        { ...fixture, voiceManifest: revoked },
        {
          now: new Date("2026-07-25T06:00:00.000Z"),
          resolvePublicKey: keyResolver,
        },
      ),
      /revoked/,
    );
  });

  it("rejects a manifest that claims Apocrypha authorship under another trusted key", async () => {
    const fixture = authorityFixture();
    const {
      signature: _oldSignature,
      keyId: _oldKeyId,
      ...manifestFields
    } = fixture.voiceManifest;
    const forgedPayload = {
      ...manifestFields,
      keyId: owner.keyId,
    };
    const forged = {
      ...forgedPayload,
      signature: signatureFor(
        forgedPayload,
        owner.keyId,
        ownerKeys.privateKey,
      ),
    };
    await assert.rejects(
      assertAuthorizedManifest(forged, {
        context: "private-encounter",
        expectedKind: "voice",
        expectedAuthorKeyId: apocrypha.keyId,
        expectedAuthorPrincipal: apocrypha.principal,
        now: new Date("2026-07-25T06:00:00.000Z"),
        resolvePublicKey: keyResolver,
      }),
      /signing key does not match Apocrypha/,
    );
  });

  it("accepts a participant-signed encounter receipt bound to current evidence", async () => {
    const fixture = authorityFixture();
    const receiptPayload = {
      receiptId: randomUUID(),
      sessionId: fixture.grant.sessionId,
      startedAt: "2026-07-25T06:00:00.000Z",
      endedAt: "2026-07-25T07:00:00.000Z",
      endState: "ended_unresolved" as const,
      authorityDigests: {
        voiceManifest: digestCanonical(fixture.voiceManifest),
        presenceManifest: digestCanonical(fixture.presenceManifest),
        encounterGrant: digestCanonical(fixture.grant),
      },
      consentHeads: fixture.consentReceipts.map(
        ({ canonicalDigest }) => canonicalDigest,
      ),
      retainedContentDigests: [],
      understandingVersionDigest: null,
    };
    const receipt = {
      ...receiptPayload,
      signature: signatureFor(
        receiptPayload,
        owner.keyId,
        ownerKeys.privateKey,
      ),
    };
    await assert.doesNotReject(
      assertEncounterReceiptAuthority(receipt, {
        sessionId: fixture.grant.sessionId,
        authorizedSignerKeyIds: fixture.grant.participants.map(
          ({ keyId }) => keyId,
        ),
        authorityDigests: receiptPayload.authorityDigests,
        consentHeads: receiptPayload.consentHeads,
        retainedContentDigests: [],
        understandingVersionDigest: null,
        startedAt: receiptPayload.startedAt,
        resolvePublicKey: keyResolver,
      }),
    );
  });

  it("rejects an encounter receipt with a stale consent head", async () => {
    const fixture = authorityFixture();
    const receiptPayload = {
      receiptId: randomUUID(),
      sessionId: fixture.grant.sessionId,
      startedAt: null,
      endedAt: "2026-07-25T07:00:00.000Z",
      endState: "revoked" as const,
      authorityDigests: {
        voiceManifest: digestCanonical(fixture.voiceManifest),
        presenceManifest: digestCanonical(fixture.presenceManifest),
        encounterGrant: digestCanonical(fixture.grant),
      },
      consentHeads: [
        fixture.consentReceipts[0].canonicalDigest,
        `sha256:${"8".repeat(64)}`,
      ],
      retainedContentDigests: [],
      understandingVersionDigest: null,
    };
    const receipt = {
      ...receiptPayload,
      signature: signatureFor(
        receiptPayload,
        owner.keyId,
        ownerKeys.privateKey,
      ),
    };
    await assert.rejects(
      assertEncounterReceiptAuthority(receipt, {
        sessionId: fixture.grant.sessionId,
        authorizedSignerKeyIds: fixture.grant.participants.map(
          ({ keyId }) => keyId,
        ),
        authorityDigests: receiptPayload.authorityDigests,
        consentHeads: fixture.consentReceipts.map(
          ({ canonicalDigest }) => canonicalDigest,
        ),
        retainedContentDigests: [],
        understandingVersionDigest: null,
        startedAt: null,
        resolvePublicKey: keyResolver,
      }),
      /current consent heads/,
    );
  });
});

describe("understanding and retention", () => {
  it("requires two understood acknowledgements on one digest", () => {
    const unsigned = {
      versionId: randomUUID(),
      sessionId: randomUUID(),
      version: 1,
      interpretations: [
        { participant: owner.principal, interpretation: "One interpretation" },
        {
          participant: apocrypha.principal,
          interpretation: "Another interpretation",
        },
      ],
      transcriptRefs: [],
      unresolvedPoints: [],
      createdAt: "2026-07-25T06:00:00.000Z",
      createdBy: owner.principal,
    };
    const version = UnderstandingVersionSchema.parse({
      ...unsigned,
      canonicalDigest: computeUnderstandingDigest(unsigned),
    });
    assert.doesNotThrow(() => {
      assertUnderstandingVersionDigest(version);
    });

    const makeAcknowledgement = (
      participant: ParticipantIdentity,
      acknowledgedAt: string,
    ): UnderstandingAcknowledgement => {
      const payload = {
        acknowledgementId: randomUUID(),
        sessionId: version.sessionId,
        participant: participant.principal,
        versionDigest: version.canonicalDigest,
        status: "understood" as const,
        correction: null,
        acknowledgedAt,
      };
      const privateKey =
        participant.role === "owner"
          ? ownerKeys.privateKey
          : apocryphaKeys.privateKey;
      return {
        ...payload,
        signature: signatureFor(payload, participant.keyId, privateKey),
      };
    };

    const one = [
      makeAcknowledgement(owner, "2026-07-25T06:01:00.000Z"),
    ];
    assert.equal(
      evaluateUnderstanding(
        version,
        [owner.principal, apocrypha.principal],
        one,
      ).outcome,
      "pending",
    );
    const two = [
      ...one,
      makeAcknowledgement(apocrypha, "2026-07-25T06:02:00.000Z"),
    ];
    assert.equal(
      evaluateUnderstanding(
        version,
        [owner.principal, apocrypha.principal],
        two,
      ).outcome,
      "mutually_understood",
    );
  });

  it("binds both retention acknowledgements to one decision digest", () => {
    const unsigned = {
      decisionId: randomUUID(),
      sessionId: randomUUID(),
      retainedArtifactClasses: ["transcript" as const],
      expiresAt: null,
      withdrawalTerms: "Either participant may withdraw retained material.",
    };
    const decisionDigest = computeRetentionDecisionDigest(unsigned);
    const acknowledgement = (participant: ParticipantIdentity) => {
      const payload = {
        participant: participant.principal,
        decisionDigest,
        acknowledgedAt: "2026-07-25T06:00:00.000Z",
      };
      return {
        ...payload,
        signature: signatureFor(
          payload,
          participant.keyId,
          participant.role === "owner"
            ? ownerKeys.privateKey
            : apocryphaKeys.privateKey,
        ),
      };
    };
    assert.doesNotThrow(() =>
      RetentionDecisionSchema.parse({
        ...unsigned,
        decisionDigest,
        acknowledgements: [acknowledgement(owner), acknowledgement(apocrypha)],
      }),
    );
  });
});

describe("realtime data-channel allowlist", () => {
  it("rejects tool and telemetry events", () => {
    assert.equal(
      isAllowedRealtimeEvent({
        type: "tool.trace",
        eventId: randomUUID(),
        sessionId: randomUUID(),
        sentAt: "2026-07-25T06:00:00.000Z",
        sender: owner.principal,
        tool: "private",
      }),
      false,
    );
    assert.equal(
      RealtimeEventSchema.safeParse({
        type: "telemetry",
        payload: {},
      }).success,
      false,
    );
  });

  it("rejects captions whose nested speaker or session diverges from the envelope", () => {
    const sessionId = randomUUID();
    assert.equal(
      RealtimeEventSchema.safeParse({
        type: "caption",
        eventId: randomUUID(),
        sessionId,
        sentAt: "2026-07-25T06:00:00.000Z",
        sender: owner.principal,
        transcript: {
          eventId: randomUUID(),
          sessionId: randomUUID(),
          speaker: apocrypha.principal,
          sequence: 1,
          status: "final",
          text: "spoofed",
          startedAtMs: 1,
          endedAtMs: 2,
          confidence: null,
          provenance: {
            source: "participant-text",
            modelDigest: null,
          },
        },
      }).success,
      false,
    );
  });
});
