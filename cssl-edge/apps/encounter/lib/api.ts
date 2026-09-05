import {
  EncounterConsentReceiptSchema,
  EncounterGrantSchema,
  ModalitySchema,
  EncounterReceiptSchema,
  RetainedArtifactClassSchema,
  UnderstandingAcknowledgementSchema,
  UnderstandingVersionSchema,
  type EncounterConsentReceipt,
  type EncounterGrant,
  type RetentionPolicy,
  type UnderstandingAcknowledgement,
  type UnderstandingOutcome,
  type UnderstandingVersion,
} from "@apocky/contracts";
import type { ExplicitConfirmation } from "@apocky/security/client";
import { z } from "zod";

export const CreateEncounterBodySchema = z
  .object({
    grant: EncounterGrantSchema,
    consentReceipts: EncounterConsentReceiptSchema.array().length(2),
    confirmation: z.unknown(),
  })
  .strict();

export const ConsentBodySchema = z
  .object({
    receipt: EncounterConsentReceiptSchema,
    confirmation: z.unknown(),
  })
  .strict();

export const RevokeConsentBodySchema = z
  .object({
    receipt: EncounterConsentReceiptSchema,
    confirmation: z.unknown(),
  })
  .strict();

export const ReadinessBodySchema = z
  .object({
    ready: z.boolean(),
    modalities: z.array(ModalitySchema),
    confirmation: z.unknown(),
  })
  .strict();

export const UnderstandingBodySchema = z
  .object({
    version: UnderstandingVersionSchema,
    confirmation: z.unknown(),
  })
  .strict();

export const UnderstandingAcknowledgementBodySchema = z
  .object({
    acknowledgement: UnderstandingAcknowledgementSchema,
    confirmation: z.unknown(),
  })
  .strict();

export const ConfirmedActionBodySchema = z
  .object({
    confirmation: z.unknown(),
  })
  .strict();

export const JoinBodySchema = ConfirmedActionBodySchema;

export const EndEncounterBodySchema = z
  .object({
    receipt: EncounterReceiptSchema,
    confirmation: z.unknown(),
  })
  .strict();

export const DeleteHistoryBodySchema = z
  .object({
    artifactClasses: z
      .array(RetainedArtifactClassSchema)
      .min(1)
      .max(3)
      .optional(),
    confirmation: z.unknown(),
  })
  .strict();

export interface EncounterParticipantView {
  principal: string;
  role: "owner" | "apocrypha";
  displayName: string;
}

export interface ConsentHeadView {
  participant: string;
  digest: string;
  issuedAt: string;
  expiresAt: string | null;
  modalities: EncounterConsentReceipt["modalities"];
}

export interface ParticipantReadinessView {
  participant: string;
  ready: boolean;
  modalities: string[];
  updatedAt: string | null;
}

export interface EncounterUnderstandingView {
  version: UnderstandingVersion;
  acknowledgements: UnderstandingAcknowledgement[];
  outcome: UnderstandingOutcome;
}

export interface EncounterSnapshot {
  session: {
    id: string;
    state:
      | "lobby"
      | "active"
      | "understanding"
      | "ended_unresolved"
      | "mutually_understood"
      | "revoked";
    createdAt: string;
    startedAt: string | null;
    endedAt: string | null;
    expiresAt: string;
    modalities: EncounterGrant["modalities"];
    retentionPolicy: RetentionPolicy;
  };
  participants: [EncounterParticipantView, EncounterParticipantView];
  authority: {
    grantDigest: string;
    voiceManifestDigest: string;
    presenceManifestDigest: string;
    voiceDisclosure: {
      authoredByApocrypha: true;
      borrowedAssistantVoice: false;
      humanVoiceClone: false;
      processing: string[];
    };
    presenceDisclosure: {
      authoredByApocrypha: true;
      placeholder: false;
      abstractProxy: false;
      genericGeneratedFace: false;
      gazeCorrection: "none" | "authorized-and-disclosed";
      eyeContactClaimAuthorized: boolean;
    };
  };
  consentHeads: [ConsentHeadView, ConsentHeadView];
  readiness: [ParticipantReadinessView, ParticipantReadinessView];
  joinAllowed: boolean;
  understanding: EncounterUnderstandingView | null;
  retainedArtifactCount: number;
}

export interface JoinCredentialView {
  serverUrl: string;
  token: string;
  e2eeKey: string;
  expiresInSeconds: number;
}

export interface ConfirmedActionRequest {
  confirmation: ExplicitConfirmation;
}
