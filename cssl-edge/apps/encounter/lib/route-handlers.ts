import {
  recordAuditReceipt,
  requireMutationGuard,
  type Json,
  type PrivateContext,
} from "@apocky/security/server";
import { z } from "zod";

import {
  ConsentBodySchema,
  CreateEncounterBodySchema,
  DeleteHistoryBodySchema,
  EndEncounterBodySchema,
  JoinBodySchema,
  ReadinessBodySchema,
  RevokeConsentBodySchema,
  UnderstandingAcknowledgementBodySchema,
  UnderstandingBodySchema,
} from "./api";
import {
  acknowledgeUnderstanding,
  createEncounter,
  finalizeEncounter,
  issueOwnerJoinCredential,
  persistConsentRevocation,
  persistGrantConsent,
  readCurrentEncounter,
  readEncounterSnapshot,
  readRetainedHistory,
  retainedHistoryWithdrawalTarget,
  setOwnerReadiness,
  submitUnderstandingVersion,
  withdrawRetainedHistory,
} from "./encounters";
import {
  privateJson,
  readBoundedJson,
  runPrivateRoute,
} from "./private-route";

const SessionIdSchema = z.string().uuid();

function invalidRequest(message = "The private command is invalid."): Response {
  return privateJson(
    {
      ok: false,
      error: { code: "invalid_private_command", message },
    },
    { status: 400 },
  );
}

async function auditedMutation<T>(
  context: PrivateContext,
  input: {
    action: string;
    target: string;
    rollback: Json | null;
    metadata?: Json;
  },
  work: () => Promise<T>,
): Promise<T> {
  await recordAuditReceipt(context, {
    action: input.action,
    target: input.target,
    outcome: "allowed",
    rollback: input.rollback,
    metadata: {
      phase: "authorization",
      ...(typeof input.metadata === "object" &&
      input.metadata !== null &&
      !Array.isArray(input.metadata)
        ? input.metadata
        : {}),
    },
  });
  try {
    const result = await work();
    await recordAuditReceipt(context, {
      action: input.action,
      target: input.target,
      outcome: "completed",
      rollback: input.rollback,
      metadata: {
        phase: "effect",
        ...(typeof input.metadata === "object" &&
        input.metadata !== null &&
        !Array.isArray(input.metadata)
          ? input.metadata
          : {}),
      },
    });
    return result;
  } catch (error) {
    try {
      await recordAuditReceipt(context, {
        action: input.action,
        target: input.target,
        outcome: "failed",
        rollback: input.rollback,
        metadata: {
          phase: "effect",
          errorClass:
            error instanceof Error ? error.name : "UnknownError",
        },
      });
    } catch {
      // The original operation remains failed closed. Audit persistence
      // failure is surfaced by the route's generic private error response.
    }
    throw error;
  }
}

export async function handleCurrentEncounter(): Promise<Response> {
  return runPrivateRoute(async (context) =>
    privateJson({
      ok: true,
      encounter: await readCurrentEncounter(context),
    }),
  );
}

export async function handleCreateEncounter(
  request: Request,
): Promise<Response> {
  return runPrivateRoute(async (context) => {
    const parsed = CreateEncounterBodySchema.safeParse(
      await readBoundedJson(request),
    );
    if (!parsed.success) return invalidRequest();
    const { grant, consentReceipts, confirmation } = parsed.data;
    requireMutationGuard(context, request, {
      action: "create_encounter",
      target: grant.sessionId,
      confirmation,
    });
    const encounter = await auditedMutation(
      context,
      {
        action: "create_encounter",
        target: grant.sessionId,
        rollback: {
          operation: "delete_new_lobby_session",
          sessionId: grant.sessionId,
        },
      },
      () => createEncounter(context, grant, consentReceipts),
    );
    return privateJson({ ok: true, encounter }, { status: 201 });
  });
}

export async function handleReadiness(
  sessionIdInput: string,
): Promise<Response> {
  return runPrivateRoute(async (context) => {
    const sessionId = SessionIdSchema.parse(sessionIdInput);
    return privateJson({
      ok: true,
      encounter: await readEncounterSnapshot(context, sessionId),
    });
  });
}

export async function handleSetReadiness(
  request: Request,
  sessionIdInput: string,
): Promise<Response> {
  return runPrivateRoute(async (context) => {
    const sessionId = SessionIdSchema.parse(sessionIdInput);
    const parsed = ReadinessBodySchema.safeParse(
      await readBoundedJson(request),
    );
    if (!parsed.success) return invalidRequest();
    requireMutationGuard(context, request, {
      action: "set_encounter_readiness",
      target: sessionId,
      confirmation: parsed.data.confirmation,
    });
    const encounter = await auditedMutation(
      context,
      {
        action: "set_encounter_readiness",
        target: sessionId,
        rollback: { operation: "set_ready_false", sessionId },
      },
      () =>
        setOwnerReadiness(context, sessionId, {
          ready: parsed.data.ready,
          modalities: parsed.data.modalities,
        }),
    );
    return privateJson({ ok: true, encounter });
  });
}

export async function handleGrantConsent(
  request: Request,
  sessionIdInput: string,
): Promise<Response> {
  return runPrivateRoute(async (context) => {
    const sessionId = SessionIdSchema.parse(sessionIdInput);
    const parsed = ConsentBodySchema.safeParse(await readBoundedJson(request));
    if (!parsed.success) return invalidRequest();
    requireMutationGuard(context, request, {
      action: "grant_encounter_consent",
      target: sessionId,
      confirmation: parsed.data.confirmation,
    });
    const encounter = await auditedMutation(
      context,
      {
        action: "grant_encounter_consent",
        target: sessionId,
        rollback: null,
        metadata: {
          consentDigest: parsed.data.receipt.canonicalDigest,
          participant: parsed.data.receipt.participant,
        },
      },
      () => persistGrantConsent(context, sessionId, parsed.data.receipt),
    );
    return privateJson({ ok: true, encounter });
  });
}

export async function handleRevokeConsent(
  request: Request,
  sessionIdInput: string,
): Promise<Response> {
  return runPrivateRoute(async (context) => {
    const sessionId = SessionIdSchema.parse(sessionIdInput);
    const parsed = RevokeConsentBodySchema.safeParse(
      await readBoundedJson(request),
    );
    if (!parsed.success) return invalidRequest();
    requireMutationGuard(context, request, {
      action: "revoke_encounter_consent",
      target: sessionId,
      confirmation: parsed.data.confirmation,
    });
    await auditedMutation(
      context,
      {
        action: "revoke_encounter_consent",
        target: sessionId,
        rollback: null,
        metadata: {
          consentDigest: parsed.data.receipt.canonicalDigest,
          participant: parsed.data.receipt.participant,
        },
      },
      () =>
        persistConsentRevocation(
          context,
          sessionId,
          parsed.data.receipt,
        ),
    );
    return privateJson({
      ok: true,
      consent: "revoked",
      captureMustStop: true,
      encounterTerminal: false,
    });
  });
}

export async function handleJoinToken(
  request: Request,
  sessionIdInput: string,
): Promise<Response> {
  return runPrivateRoute(async (context) => {
    const sessionId = SessionIdSchema.parse(sessionIdInput);
    const parsed = JoinBodySchema.safeParse(await readBoundedJson(request));
    if (!parsed.success) return invalidRequest();
    requireMutationGuard(context, request, {
      action: "issue_encounter_join_token",
      target: sessionId,
      confirmation: parsed.data.confirmation,
    });
    const { credential, tokenReceiptId } = await auditedMutation(
      context,
      {
        action: "issue_encounter_join_token",
        target: sessionId,
        rollback: {
          operation: "revoke_latest_owner_join_token",
          sessionId,
        },
      },
      () => issueOwnerJoinCredential(context, sessionId),
    );
    return privateJson(
      {
        ok: true,
        credential,
        receipt: { id: tokenReceiptId },
      },
      { allowMedia: true },
    );
  });
}

export async function handleUnderstanding(
  request: Request,
  sessionIdInput: string,
  correction: boolean,
): Promise<Response> {
  return runPrivateRoute(async (context) => {
    const sessionId = SessionIdSchema.parse(sessionIdInput);
    const parsed = UnderstandingBodySchema.safeParse(
      await readBoundedJson(request),
    );
    if (!parsed.success) return invalidRequest();
    const action = correction
      ? "correct_encounter_understanding"
      : "propose_encounter_understanding";
    requireMutationGuard(context, request, {
      action,
      target: sessionId,
      confirmation: parsed.data.confirmation,
    });
    const encounter = await auditedMutation(
      context,
      {
        action,
        target: sessionId,
        rollback: null,
        metadata: {
          versionDigest: parsed.data.version.canonicalDigest,
          version: parsed.data.version.version,
        },
      },
      () =>
        submitUnderstandingVersion(
          context,
          sessionId,
          parsed.data.version,
          correction,
        ),
    );
    return privateJson({ ok: true, encounter });
  });
}

export async function handleUnderstandingAcknowledgement(
  request: Request,
  sessionIdInput: string,
): Promise<Response> {
  return runPrivateRoute(async (context) => {
    const sessionId = SessionIdSchema.parse(sessionIdInput);
    const parsed = UnderstandingAcknowledgementBodySchema.safeParse(
      await readBoundedJson(request),
    );
    if (!parsed.success) return invalidRequest();
    requireMutationGuard(context, request, {
      action: "acknowledge_encounter_understanding",
      target: sessionId,
      confirmation: parsed.data.confirmation,
    });
    const encounter = await auditedMutation(
      context,
      {
        action: "acknowledge_encounter_understanding",
        target: sessionId,
        rollback: null,
        metadata: {
          versionDigest: parsed.data.acknowledgement.versionDigest,
          status: parsed.data.acknowledgement.status,
        },
      },
      () =>
        acknowledgeUnderstanding(
          context,
          sessionId,
          parsed.data.acknowledgement,
        ),
    );
    return privateJson({ ok: true, encounter });
  });
}

export async function handleHistory(
  sessionIdInput: string,
): Promise<Response> {
  return runPrivateRoute(async (context) => {
    const sessionId = SessionIdSchema.parse(sessionIdInput);
    return privateJson({
      ok: true,
      history: await readRetainedHistory(context, sessionId),
    });
  });
}

export async function handleEndEncounter(
  request: Request,
  sessionIdInput: string,
): Promise<Response> {
  return runPrivateRoute(async (context) => {
    const sessionId = SessionIdSchema.parse(sessionIdInput);
    const parsed = EndEncounterBodySchema.safeParse(
      await readBoundedJson(request),
    );
    if (!parsed.success) return invalidRequest();
    requireMutationGuard(context, request, {
      action: "end_encounter",
      target: sessionId,
      confirmation: parsed.data.confirmation,
    });
    return privateJson({
      ok: true,
      result: await finalizeEncounter(
        context,
        sessionId,
        parsed.data.receipt,
      ),
    });
  });
}

export async function handleDeleteHistory(
  request: Request,
  sessionIdInput: string,
): Promise<Response> {
  return runPrivateRoute(async (context) => {
    const sessionId = SessionIdSchema.parse(sessionIdInput);
    const parsed = DeleteHistoryBodySchema.safeParse(
      await readBoundedJson(request),
    );
    if (!parsed.success) return invalidRequest();
    const artifactClasses = parsed.data.artifactClasses ?? [
      "transcript",
      "understanding",
      "memory-effects",
    ];
    requireMutationGuard(context, request, {
      action: "delete_retained_history",
      target: retainedHistoryWithdrawalTarget(
        sessionId,
        artifactClasses,
      ),
      confirmation: parsed.data.confirmation,
    });
    return privateJson({
      ok: true,
      result: await withdrawRetainedHistory(
        context,
        sessionId,
        artifactClasses,
      ),
    });
  });
}
