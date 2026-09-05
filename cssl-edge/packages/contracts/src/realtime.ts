import { z } from "zod";

import {
  IsoDateTimeSchema,
  ModalitySchema,
  PrincipalSchema,
  TranscriptEventSchema,
  UnderstandingAcknowledgementSchema,
  UnderstandingVersionSchema,
} from "./types";

const EventBaseShape = {
  eventId: z.string().uuid(),
  sessionId: z.string().uuid(),
  sentAt: IsoDateTimeSchema,
  sender: PrincipalSchema,
} as const;

export const ParticipantReadinessEventSchema = z
  .object({
    type: z.literal("participant.readiness"),
    ...EventBaseShape,
    ready: z.boolean(),
    modalities: z.array(ModalitySchema),
  })
  .strict();

export const SpeechStateEventSchema = z
  .object({
    type: z.literal("speech.state"),
    ...EventBaseShape,
    state: z.enum(["started", "stopped", "interrupted"]),
  })
  .strict();

export const CaptionEventSchema = z
  .object({
    type: z.literal("caption"),
    ...EventBaseShape,
    transcript: TranscriptEventSchema,
  })
  .strict()
  .superRefine((event, context) => {
    if (event.transcript.sessionId !== event.sessionId) {
      context.addIssue({
        code: "custom",
        message: "Caption transcript must belong to the envelope session",
        path: ["transcript", "sessionId"],
      });
    }
    if (event.transcript.speaker !== event.sender) {
      context.addIssue({
        code: "custom",
        message: "Caption speaker must match the envelope sender",
        path: ["transcript", "speaker"],
      });
    }
  });

export const ConsentChangeEventSchema = z
  .object({
    type: z.literal("consent.change"),
    ...EventBaseShape,
    modality: ModalitySchema,
    state: z.enum(["granted", "revoked"]),
    consentDigest: z.string().regex(/^sha256:[a-f0-9]{64}$/),
  })
  .strict();

export const UnderstandingProposalEventSchema = z
  .object({
    type: z.literal("understanding.proposal"),
    ...EventBaseShape,
    version: UnderstandingVersionSchema,
  })
  .strict()
  .superRefine((event, context) => {
    if (event.version.sessionId !== event.sessionId) {
      context.addIssue({
        code: "custom",
        message: "Understanding proposal must belong to the envelope session",
        path: ["version", "sessionId"],
      });
    }
    if (event.version.createdBy !== event.sender) {
      context.addIssue({
        code: "custom",
        message: "Understanding author must match the envelope sender",
        path: ["version", "createdBy"],
      });
    }
  });

export const UnderstandingAcknowledgementEventSchema = z
  .object({
    type: z.literal("understanding.acknowledgement"),
    ...EventBaseShape,
    acknowledgement: UnderstandingAcknowledgementSchema,
  })
  .strict()
  .superRefine((event, context) => {
    if (event.acknowledgement.sessionId !== event.sessionId) {
      context.addIssue({
        code: "custom",
        message:
          "Understanding acknowledgement must belong to the envelope session",
        path: ["acknowledgement", "sessionId"],
      });
    }
    if (event.acknowledgement.participant !== event.sender) {
      context.addIssue({
        code: "custom",
        message:
          "Understanding acknowledgement participant must match the envelope sender",
        path: ["acknowledgement", "participant"],
      });
    }
  });

export const ConnectionStateEventSchema = z
  .object({
    type: z.literal("connection.state"),
    ...EventBaseShape,
    state: z.enum([
      "connecting",
      "connected",
      "reconnecting",
      "disconnected",
    ]),
  })
  .strict();

export const SessionEndEventSchema = z
  .object({
    type: z.literal("session.end"),
    ...EventBaseShape,
    outcome: z.enum(["ended_unresolved", "mutually_understood", "revoked"]),
  })
  .strict();

export const RealtimeEventSchema = z.discriminatedUnion("type", [
  ParticipantReadinessEventSchema,
  SpeechStateEventSchema,
  CaptionEventSchema,
  ConsentChangeEventSchema,
  UnderstandingProposalEventSchema,
  UnderstandingAcknowledgementEventSchema,
  ConnectionStateEventSchema,
  SessionEndEventSchema,
]);

export type RealtimeEvent = z.infer<typeof RealtimeEventSchema>;

export const REALTIME_EVENT_TYPES = [
  "participant.readiness",
  "speech.state",
  "caption",
  "consent.change",
  "understanding.proposal",
  "understanding.acknowledgement",
  "connection.state",
  "session.end",
] as const;

export function isAllowedRealtimeEvent(value: unknown): value is RealtimeEvent {
  return RealtimeEventSchema.safeParse(value).success;
}
