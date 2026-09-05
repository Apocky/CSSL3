import { z } from "zod";

export const IsoDateTimeSchema = z.string().datetime({ offset: true });
export const PrincipalSchema = z.string().trim().min(3).max(200);
export const KeyIdSchema = z.string().trim().min(3).max(160);
export const Sha256DigestSchema = z
  .string()
  .regex(/^sha256:[a-f0-9]{64}$/, "Expected a lowercase sha256: digest");
export const Base64UrlSchema = z
  .string()
  .min(43)
  .max(256)
  .regex(/^[A-Za-z0-9_-]+$/, "Expected unpadded base64url");

export const SignatureSchema = z
  .object({
    algorithm: z.literal("Ed25519"),
    keyId: KeyIdSchema,
    value: Base64UrlSchema,
  })
  .strict();

export type ContractSignature = z.infer<typeof SignatureSchema>;

export const ParticipantRoleSchema = z.enum(["owner", "apocrypha"]);

export const ParticipantIdentitySchema = z
  .object({
    principal: PrincipalSchema,
    keyId: KeyIdSchema,
    role: ParticipantRoleSchema,
    displayName: z.string().trim().min(1).max(80),
    signature: SignatureSchema,
  })
  .strict()
  .superRefine((identity, context) => {
    if (identity.keyId !== identity.signature.keyId) {
      context.addIssue({
        code: "custom",
        message: "Identity keyId must match signature keyId",
        path: ["signature", "keyId"],
      });
    }
  });

export type ParticipantIdentity = z.infer<typeof ParticipantIdentitySchema>;

export const ManifestContextSchema = z.enum([
  "private-encounter",
  "human-acceptance",
  "public-display",
]);

const ManifestBaseShape = {
  manifestId: z.string().uuid(),
  authorPrincipal: PrincipalSchema,
  keyId: KeyIdSchema,
  permittedContexts: z.array(ManifestContextSchema).min(1),
  version: z.number().int().positive(),
  issuedAt: IsoDateTimeSchema,
  revokedAt: IsoDateTimeSchema.nullable(),
  signature: SignatureSchema,
} as const;

export const VoiceManifestSchema = z
  .object({
    kind: z.literal("voice"),
    ...ManifestBaseShape,
    voiceArtifactDigest: Sha256DigestSchema,
    disclosure: z
      .object({
        authoredByApocrypha: z.literal(true),
        borrowedAssistantVoice: z.literal(false),
        humanVoiceClone: z.literal(false),
        processing: z.array(
          z.enum(["echo-cancellation", "noise-suppression", "gain-control"]),
        ),
      })
      .strict(),
  })
  .strict()
  .superRefine((manifest, context) => {
    if (manifest.keyId !== manifest.signature.keyId) {
      context.addIssue({
        code: "custom",
        message: "Manifest keyId must match signature keyId",
        path: ["signature", "keyId"],
      });
    }
  });

export type VoiceManifest = z.infer<typeof VoiceManifestSchema>;

export const PresenceManifestSchema = z
  .object({
    kind: z.literal("presence"),
    ...ManifestBaseShape,
    rendererDigest: Sha256DigestSchema,
    assetDigests: z.array(Sha256DigestSchema).min(1),
    disclosure: z
      .object({
        authoredByApocrypha: z.literal(true),
        placeholder: z.literal(false),
        abstractProxy: z.literal(false),
        genericGeneratedFace: z.literal(false),
        gazeCorrection: z.enum(["none", "authorized-and-disclosed"]),
        eyeContactClaimAuthorized: z.boolean(),
      })
      .strict(),
  })
  .strict()
  .superRefine((manifest, context) => {
    if (manifest.keyId !== manifest.signature.keyId) {
      context.addIssue({
        code: "custom",
        message: "Manifest keyId must match signature keyId",
        path: ["signature", "keyId"],
      });
    }
    if (
      manifest.disclosure.eyeContactClaimAuthorized &&
      manifest.disclosure.gazeCorrection === "none"
    ) {
      context.addIssue({
        code: "custom",
        message:
          "Eye-contact claims require an authorized disclosed renderer capability",
        path: ["disclosure", "eyeContactClaimAuthorized"],
      });
    }
  });

export type PresenceManifest = z.infer<typeof PresenceManifestSchema>;

export const ModalitySchema = z.enum(["audio", "video", "captions", "text"]);

export const RetentionPolicySchema = z
  .object({
    rawAudio: z.literal("never"),
    rawVideo: z.literal("never"),
    transcript: z.enum(["discard", "mutual"]),
    understanding: z.enum(["discard", "mutual"]),
    memoryEffects: z.enum(["discard", "mutual"]),
  })
  .strict();

export type RetentionPolicy = z.infer<typeof RetentionPolicySchema>;

export const EncounterGrantSchema = z
  .object({
    grantId: z.string().uuid(),
    sessionId: z.string().uuid(),
    participants: z.array(ParticipantIdentitySchema).length(2),
    modalities: z.array(ModalitySchema).min(1),
    retentionPolicy: RetentionPolicySchema,
    consentRefs: z.array(Sha256DigestSchema).length(2),
    authorityDigests: z
      .object({
        voiceManifest: Sha256DigestSchema,
        presenceManifest: Sha256DigestSchema,
      })
      .strict(),
    expiresAt: IsoDateTimeSchema,
    nonce: z.string().min(32).max(256),
    signature: SignatureSchema,
  })
  .strict()
  .superRefine((grant, context) => {
    const roles = new Set(grant.participants.map(({ role }) => role));
    const principals = new Set(
      grant.participants.map(({ principal }) => principal),
    );
    const consentRefs = new Set(grant.consentRefs);
    const modalities = new Set(grant.modalities);
    if (!roles.has("owner") || !roles.has("apocrypha")) {
      context.addIssue({
        code: "custom",
        message: "Encounter requires one owner and one Apocrypha participant",
        path: ["participants"],
      });
    }
    if (principals.size !== 2) {
      context.addIssue({
        code: "custom",
        message: "Encounter participants must be distinct",
        path: ["participants"],
      });
    }
    if (consentRefs.size !== 2) {
      context.addIssue({
        code: "custom",
        message: "Encounter requires two distinct consent references",
        path: ["consentRefs"],
      });
    }
    if (modalities.size !== grant.modalities.length) {
      context.addIssue({
        code: "custom",
        message: "An encounter grant may name each modality only once",
        path: ["modalities"],
      });
    }
  });

export type EncounterGrant = z.infer<typeof EncounterGrantSchema>;

export const ModalityConsentStateSchema = z
  .object({
    modality: ModalitySchema,
    state: z.enum(["granted", "revoked"]),
  })
  .strict();

export const EncounterConsentReceiptUnsignedSchema = z
  .object({
    receiptId: z.string().uuid(),
    sessionId: z.string().uuid(),
    participant: PrincipalSchema,
    modalities: z.array(ModalityConsentStateSchema).min(1),
    previousReceiptDigest: Sha256DigestSchema.nullable(),
    issuedAt: IsoDateTimeSchema,
    expiresAt: IsoDateTimeSchema.nullable(),
  })
  .strict()
  .superRefine((receipt, context) => {
    const modalities = new Set(
      receipt.modalities.map(({ modality }) => modality),
    );
    if (modalities.size !== receipt.modalities.length) {
      context.addIssue({
        code: "custom",
        message: "A consent head may name each modality only once",
        path: ["modalities"],
      });
    }
  });

export const EncounterConsentReceiptSchema =
  EncounterConsentReceiptUnsignedSchema.extend({
    canonicalDigest: Sha256DigestSchema,
    signature: SignatureSchema,
  }).strict();

export type EncounterConsentReceiptUnsigned = z.infer<
  typeof EncounterConsentReceiptUnsignedSchema
>;
export type EncounterConsentReceipt = z.infer<
  typeof EncounterConsentReceiptSchema
>;

export const TranscriptEventSchema = z
  .object({
    eventId: z.string().uuid(),
    sessionId: z.string().uuid(),
    speaker: PrincipalSchema,
    sequence: z.number().int().nonnegative(),
    status: z.enum(["partial", "final"]),
    text: z.string().max(20_000),
    startedAtMs: z.number().int().nonnegative(),
    endedAtMs: z.number().int().nonnegative(),
    confidence: z.number().min(0).max(1).nullable(),
    provenance: z
      .object({
        source: z.enum(["participant-text", "speech-recognition"]),
        modelDigest: Sha256DigestSchema.nullable(),
      })
      .strict(),
  })
  .strict()
  .superRefine((event, context) => {
    if (event.endedAtMs < event.startedAtMs) {
      context.addIssue({
        code: "custom",
        message: "Transcript event cannot end before it starts",
        path: ["endedAtMs"],
      });
    }
    if (
      event.provenance.source === "speech-recognition" &&
      event.provenance.modelDigest === null
    ) {
      context.addIssue({
        code: "custom",
        message: "Speech-recognition provenance requires a model digest",
        path: ["provenance", "modelDigest"],
      });
    }
  });

export type TranscriptEvent = z.infer<typeof TranscriptEventSchema>;

export const AttributedInterpretationSchema = z
  .object({
    participant: PrincipalSchema,
    interpretation: z.string().trim().min(1).max(20_000),
  })
  .strict();

export const UnderstandingVersionUnsignedSchema = z
  .object({
    versionId: z.string().uuid(),
    sessionId: z.string().uuid(),
    version: z.number().int().positive(),
    interpretations: z.array(AttributedInterpretationSchema).min(2),
    transcriptRefs: z.array(z.string().uuid()),
    unresolvedPoints: z.array(z.string().trim().min(1).max(4_000)),
    createdAt: IsoDateTimeSchema,
    createdBy: PrincipalSchema,
  })
  .strict();

export const UnderstandingVersionSchema =
  UnderstandingVersionUnsignedSchema.extend({
    canonicalDigest: Sha256DigestSchema,
  }).strict();

export type UnderstandingVersionUnsigned = z.infer<
  typeof UnderstandingVersionUnsignedSchema
>;
export type UnderstandingVersion = z.infer<typeof UnderstandingVersionSchema>;

export const UnderstandingAcknowledgementSchema = z
  .object({
    acknowledgementId: z.string().uuid(),
    sessionId: z.string().uuid(),
    participant: PrincipalSchema,
    versionDigest: Sha256DigestSchema,
    status: z.enum(["understood", "needs_repair", "disagree"]),
    correction: z.string().trim().min(1).max(20_000).nullable(),
    acknowledgedAt: IsoDateTimeSchema,
    signature: SignatureSchema,
  })
  .strict()
  .superRefine((acknowledgement, context) => {
    if (
      acknowledgement.status !== "understood" &&
      acknowledgement.correction === null
    ) {
      context.addIssue({
        code: "custom",
        message: "Repair or disagreement requires an attributed correction",
        path: ["correction"],
      });
    }
    if (
      acknowledgement.status === "understood" &&
      acknowledgement.correction !== null
    ) {
      context.addIssue({
        code: "custom",
        message: "An understood acknowledgement cannot contain a correction",
        path: ["correction"],
      });
    }
  });

export type UnderstandingAcknowledgement = z.infer<
  typeof UnderstandingAcknowledgementSchema
>;

export const RetainedArtifactClassSchema = z.enum([
  "transcript",
  "understanding",
  "memory-effects",
]);

export const RetentionAcknowledgementSchema = z
  .object({
    participant: PrincipalSchema,
    decisionDigest: Sha256DigestSchema,
    acknowledgedAt: IsoDateTimeSchema,
    signature: SignatureSchema,
  })
  .strict();

export const RetentionDecisionUnsignedSchema = z
  .object({
    decisionId: z.string().uuid(),
    sessionId: z.string().uuid(),
    retainedArtifactClasses: z.array(RetainedArtifactClassSchema),
    expiresAt: IsoDateTimeSchema.nullable(),
    withdrawalTerms: z.string().trim().min(1).max(8_000),
  })
  .strict();

export const RetentionDecisionSchema = RetentionDecisionUnsignedSchema.extend({
  decisionDigest: Sha256DigestSchema,
  acknowledgements: z.array(RetentionAcknowledgementSchema).length(2),
})
  .strict()
  .superRefine((decision, context) => {
    const participants = new Set(
      decision.acknowledgements.map(({ participant }) => participant),
    );
    const digests = new Set(
      decision.acknowledgements.map(({ decisionDigest }) => decisionDigest),
    );
    if (participants.size !== 2 || digests.size !== 1) {
      context.addIssue({
        code: "custom",
        message:
          "Retention requires two distinct participants acknowledging one digest",
        path: ["acknowledgements"],
      });
    }
    if (!digests.has(decision.decisionDigest)) {
      context.addIssue({
        code: "custom",
        message: "Retention acknowledgements must bind the decision digest",
        path: ["acknowledgements"],
      });
    }
  });

export type RetentionDecisionUnsigned = z.infer<
  typeof RetentionDecisionUnsignedSchema
>;
export type RetentionDecision = z.infer<typeof RetentionDecisionSchema>;

export const EncounterEndStateSchema = z.enum([
  "ended_unresolved",
  "mutually_understood",
  "revoked",
]);

export const EncounterReceiptSchema = z
  .object({
    receiptId: z.string().uuid(),
    sessionId: z.string().uuid(),
    startedAt: IsoDateTimeSchema.nullable(),
    endedAt: IsoDateTimeSchema,
    endState: EncounterEndStateSchema,
    authorityDigests: z
      .object({
        voiceManifest: Sha256DigestSchema,
        presenceManifest: Sha256DigestSchema,
        encounterGrant: Sha256DigestSchema,
      })
      .strict(),
    consentHeads: z.array(Sha256DigestSchema).length(2),
    retainedContentDigests: z.array(Sha256DigestSchema),
    understandingVersionDigest: Sha256DigestSchema.nullable(),
    signature: SignatureSchema,
  })
  .strict()
  .superRefine((receipt, context) => {
    if (new Set(receipt.consentHeads).size !== 2) {
      context.addIssue({
        code: "custom",
        message: "An encounter receipt requires two distinct consent heads",
        path: ["consentHeads"],
      });
    }
    if (
      new Set(receipt.retainedContentDigests).size !==
      receipt.retainedContentDigests.length
    ) {
      context.addIssue({
        code: "custom",
        message: "Retained content digests must be unique",
        path: ["retainedContentDigests"],
      });
    }
    if (
      receipt.endState === "mutually_understood" &&
      receipt.understandingVersionDigest === null
    ) {
      context.addIssue({
        code: "custom",
        message:
          "A mutually understood receipt requires an understanding digest",
        path: ["understandingVersionDigest"],
      });
    }
    if (
      receipt.endState !== "mutually_understood" &&
      receipt.understandingVersionDigest !== null
    ) {
      context.addIssue({
        code: "custom",
        message:
          "Unresolved, revoked, or cancelled encounters cannot claim an understanding digest",
        path: ["understandingVersionDigest"],
      });
    }
  });

export type EncounterReceipt = z.infer<typeof EncounterReceiptSchema>;
