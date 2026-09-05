import { randomUUID } from "node:crypto";

import {
  EncounterConsentReceiptSchema,
  EncounterGrantSchema,
  EncounterReceiptSchema,
  UnderstandingAcknowledgementSchema,
  UnderstandingVersionSchema,
  evaluateUnderstanding,
  type EncounterConsentReceipt,
  type EncounterGrant,
  type UnderstandingAcknowledgement,
  type UnderstandingVersion,
} from "@apocky/contracts";
import {
  assertEncounterReceiptAuthority,
  assertUnderstandingVersionDigest,
  digestCanonical,
  verifySignedContract,
} from "@apocky/contracts/server";
import type {
  Json,
  PrivateContext,
} from "@apocky/security/server";
import { z } from "zod";

import type {
  EncounterSnapshot,
  ParticipantReadinessView,
} from "./api";
import {
  createPublicKeyResolver,
  readConsentHeads,
  readConsentReceiptsByDigest,
  readJoinReadiness,
  readStoredGrant,
  readinessAllowsJoin,
  verifyConsentReceipt,
  verifyEncounterGrant,
  verifyStoredEncounterAuthority,
} from "./authority";
import { mintOwnerJoinCredential } from "./livekit";
import { assertCorrectionTransition } from "./understanding";

const OPEN_STATES = ["lobby", "active", "understanding"] as const;
const RETAINED_ARTIFACT_CLASSES = [
  "transcript",
  "understanding",
  "memory-effects",
] as const;

const FinalizeEncounterResultSchema = z
  .object({
    encounterReceiptId: z.string().uuid(),
    encounterReceiptDigest: z
      .string()
      .regex(/^sha256:[a-f0-9]{64}$/),
    auditReceiptId: z.string().uuid(),
    auditReceiptDigest: z.string().regex(/^sha256:[a-f0-9]{64}$/),
    endState: z.enum([
      "ended_unresolved",
      "mutually_understood",
      "revoked",
    ]),
    endedAt: z.string().datetime({ offset: true }),
  })
  .strict();

const RetentionWithdrawalResultSchema = z
  .object({
    withdrawalId: z.string().uuid(),
    deletedArtifactCount: z.number().int().nonnegative(),
    deletedContentDigests: z.array(
      z.string().regex(/^sha256:[a-f0-9]{64}$/),
    ),
    workflowState: z.enum(["completed", "pending_upstream"]),
    auditReceiptId: z.string().uuid(),
    auditReceiptDigest: z.string().regex(/^sha256:[a-f0-9]{64}$/),
  })
  .strict();

function jsonValue(value: unknown): Json {
  return JSON.parse(JSON.stringify(value)) as Json;
}

function requireNoError(
  error: { message: string } | null,
  message: string,
): void {
  if (error !== null) {
    throw new Error(`${message}: ${error.message}`);
  }
}

function consentRows(
  ownerId: string,
  receipt: EncounterConsentReceipt,
) {
  return receipt.modalities.map(({ modality, state }) => ({
    owner_id: ownerId,
    session_id: receipt.sessionId,
    participant_principal: receipt.participant,
    modality,
    state,
    receipt_digest: receipt.canonicalDigest,
    receipt: jsonValue(receipt),
    created_at: receipt.issuedAt,
  }));
}

async function insertConsentReceipt(
  context: PrivateContext,
  receipt: EncounterConsentReceipt,
): Promise<void> {
  const { data: existing, error: existingError } = await context.supabase
    .from("encounter_consents")
    .select("id")
    .eq("owner_id", context.principal.userId)
    .eq("session_id", receipt.sessionId)
    .eq("receipt_digest", receipt.canonicalDigest)
    .limit(1);
  requireNoError(existingError, "Consent provenance lookup failed");
  if ((existing?.length ?? 0) > 0) return;
  const { error } = await context.supabase
    .from("encounter_consents")
    .insert(consentRows(context.principal.userId, receipt));
  requireNoError(error, "Consent provenance persistence failed");
}

export async function createEncounter(
  context: PrivateContext,
  grantInput: unknown,
  consentReceiptsInput: unknown,
): Promise<EncounterSnapshot> {
  const grant = EncounterGrantSchema.parse(grantInput);
  const consentReceipts = EncounterConsentReceiptSchema.array()
    .length(2)
    .parse(consentReceiptsInput);
  const authority = await verifyEncounterGrant(
    context,
    grant,
    consentReceipts,
  );

  const { error: sessionError } = await context.supabase
    .from("encounter_sessions")
    .insert({
      id: grant.sessionId,
      owner_id: context.principal.userId,
      state: "lobby",
      grant_digest: authority.grantDigest,
      grant_nonce_digest: digestCanonical(grant.nonce),
      grant: jsonValue(grant),
      voice_manifest_digest: authority.voiceManifestDigest,
      presence_manifest_digest: authority.presenceManifestDigest,
      retention_policy: jsonValue(grant.retentionPolicy),
    });
  requireNoError(sessionError, "Encounter creation failed");

  try {
    for (const receipt of consentReceipts) {
      await insertConsentReceipt(context, receipt);
    }
    const { error: readinessError } = await context.supabase
      .from("encounter_readiness")
      .insert(
        grant.participants.map(({ principal }) => ({
          owner_id: context.principal.userId,
          session_id: grant.sessionId,
          participant_principal: principal,
          ready: false,
          modalities: [],
        })),
      );
    requireNoError(readinessError, "Encounter readiness creation failed");
  } catch (error) {
    const rollback = await context.supabase
      .from("encounter_sessions")
      .delete()
      .eq("owner_id", context.principal.userId)
      .eq("id", grant.sessionId)
      .eq("state", "lobby");
    if (rollback.error !== null) {
      throw new Error(
        "Encounter initialization failed and cleanup could not be verified.",
        { cause: error },
      );
    }
    throw error;
  }
  return readEncounterSnapshot(context, grant.sessionId);
}

export async function persistGrantConsent(
  context: PrivateContext,
  sessionId: string,
  receiptInput: unknown,
): Promise<EncounterSnapshot> {
  const grant = await readStoredGrant(context, sessionId);
  const receipt = await verifyConsentReceipt(context, grant, receiptInput);
  if (
    !grant.consentRefs.includes(receipt.canonicalDigest) ||
    receipt.previousReceiptDigest !== null ||
    grant.modalities.some(
      (modality) =>
        !receipt.modalities.some(
          (entry) =>
            entry.modality === modality && entry.state === "granted",
        ),
    )
  ) {
    throw new Error("Consent receipt is not an initial grant-bound head.");
  }
  await insertConsentReceipt(context, receipt);
  return readEncounterSnapshot(context, sessionId);
}

export async function persistConsentRevocation(
  context: PrivateContext,
  sessionId: string,
  receiptInput: unknown,
): Promise<void> {
  const grant = await readStoredGrant(context, sessionId);
  const receipt = await verifyConsentReceipt(context, grant, receiptInput);
  const heads = await readConsentHeads(context, sessionId);
  const previous = heads.find(
    ({ participant }) => participant === receipt.participant,
  );
  if (
    previous === undefined ||
    receipt.previousReceiptDigest !== previous.canonicalDigest
  ) {
    throw new Error("Consent revocation does not extend the current head.");
  }
  const previousStates = new Map(
    previous.modalities.map(({ modality, state }) => [modality, state]),
  );
  const nextStates = new Map(
    receipt.modalities.map(({ modality, state }) => [modality, state]),
  );
  if (
    grant.modalities.some((modality) => !nextStates.has(modality)) ||
    grant.modalities.some(
      (modality) =>
        previousStates.get(modality) === "revoked" &&
        nextStates.get(modality) === "granted",
    ) ||
    !grant.modalities.some(
      (modality) =>
        previousStates.get(modality) === "granted" &&
        nextStates.get(modality) === "revoked",
    )
  ) {
    throw new Error("Consent revocation must be complete and monotonic.");
  }

  await insertConsentReceipt(context, receipt);
}

export async function setOwnerReadiness(
  context: PrivateContext,
  sessionId: string,
  input: { ready: boolean; modalities: string[] },
): Promise<EncounterSnapshot> {
  const authority = await verifyStoredEncounterAuthority(context, sessionId);
  const selected = new Set(input.modalities);
  const grantedModalities = new Set<string>(authority.grant.modalities);
  if (
    [...selected].some((modality) => !grantedModalities.has(modality)) ||
    (input.ready &&
      authority.grant.modalities.some(
        (modality) => !selected.has(modality),
      ))
  ) {
    throw new Error("Readiness modalities do not match the encounter grant.");
  }
  const now = new Date().toISOString();
  const { error } = await context.supabase
    .from("encounter_readiness")
    .upsert(
      {
        owner_id: context.principal.userId,
        session_id: sessionId,
        participant_principal: authority.ownerIdentity.principal,
        ready: input.ready,
        modalities: [...selected],
        updated_at: now,
      },
      { onConflict: "session_id,participant_principal" },
    );
  requireNoError(error, "Owner readiness persistence failed");
  return readEncounterSnapshot(context, sessionId);
}

async function readUnderstanding(
  context: PrivateContext,
  sessionId: string,
  participants: readonly [string, string],
): Promise<EncounterSnapshot["understanding"]> {
  const { data: versionRow, error: versionError } = await context.supabase
    .from("understanding_versions")
    .select("id,version,canonical_digest,content")
    .eq("owner_id", context.principal.userId)
    .eq("session_id", sessionId)
    .order("version", { ascending: false })
    .limit(1)
    .maybeSingle();
  requireNoError(versionError, "Understanding version lookup failed");
  if (versionRow === null) return null;
  const version = UnderstandingVersionSchema.parse(versionRow.content);
  if (
    version.versionId !== versionRow.id ||
    version.version !== versionRow.version ||
    version.canonicalDigest !== versionRow.canonical_digest
  ) {
    throw new Error("Understanding version provenance is inconsistent.");
  }
  assertUnderstandingVersionDigest(version);

  const { data: acknowledgementRows, error } = await context.supabase
    .from("understanding_acknowledgements")
    .select(
      "id,participant_principal,version_digest,status,correction,signature,acknowledged_at",
    )
    .eq("owner_id", context.principal.userId)
    .eq("session_id", sessionId)
    .eq("version_digest", version.canonicalDigest)
    .order("acknowledged_at", { ascending: true });
  requireNoError(error, "Understanding acknowledgement lookup failed");
  const acknowledgements = (acknowledgementRows ?? []).map((row) =>
    UnderstandingAcknowledgementSchema.parse({
      acknowledgementId: row.id,
      sessionId,
      participant: row.participant_principal,
      versionDigest: row.version_digest,
      status: row.status,
      correction: row.correction,
      acknowledgedAt: row.acknowledged_at,
      signature: row.signature,
    }),
  );
  const grant = await readStoredGrant(context, sessionId);
  const resolvePublicKey = createPublicKeyResolver(context);
  for (const acknowledgement of acknowledgements) {
    const participant = grant.participants.find(
      ({ principal }) => principal === acknowledgement.participant,
    );
    if (
      participant === undefined ||
      participant.keyId !== acknowledgement.signature.keyId ||
      !(await verifySignedContract(acknowledgement, resolvePublicKey))
    ) {
      throw new Error(
        "Persisted understanding acknowledgement authority is invalid.",
      );
    }
  }
  const evaluation = evaluateUnderstanding(
    version,
    participants,
    acknowledgements,
  );
  return {
    version,
    acknowledgements,
    outcome: evaluation.outcome,
  };
}

export async function readEncounterSnapshot(
  context: PrivateContext,
  sessionId: string,
): Promise<EncounterSnapshot> {
  const [{ data: session, error: sessionError }, authority] =
    await Promise.all([
      context.supabase
        .from("encounter_sessions")
        .select(
          "id,state,created_at,started_at,ended_at,grant_digest,voice_manifest_digest,presence_manifest_digest",
        )
        .eq("owner_id", context.principal.userId)
        .eq("id", sessionId)
        .maybeSingle(),
      verifyStoredEncounterAuthority(context, sessionId),
    ]);
  if (sessionError !== null || session === null) {
    throw new Error("Encounter session is unavailable.");
  }
  const participants = authority.grant.participants.map(
    ({ principal, role, displayName }) => ({
      principal,
      role,
      displayName,
    }),
  ) as EncounterSnapshot["participants"];
  const participantPrincipals = participants.map(
    ({ principal }) => principal,
  ) as [string, string];
  const [readinessResult, understanding, retainedCount] = await Promise.all([
    context.supabase
      .from("encounter_readiness")
      .select("participant_principal,ready,modalities,updated_at")
      .eq("owner_id", context.principal.userId)
      .eq("session_id", sessionId),
    readUnderstanding(context, sessionId, participantPrincipals),
    context.supabase
      .from("retained_encounter_artifacts")
      .select("id", { count: "exact", head: true })
      .eq("owner_id", context.principal.userId)
      .eq("session_id", sessionId),
  ]);
  requireNoError(readinessResult.error, "Readiness lookup failed");
  requireNoError(retainedCount.error, "Retained-history count failed");
  const readinessByParticipant = new Map(
    (readinessResult.data ?? []).map((row) => [
      row.participant_principal,
      row,
    ]),
  );
  const readiness = participantPrincipals.map((participant) => {
    const row = readinessByParticipant.get(participant);
    return {
      participant,
      ready: row?.ready ?? false,
      modalities: row?.modalities ?? [],
      updatedAt: row?.updated_at ?? null,
    } satisfies ParticipantReadinessView;
  }) as [ParticipantReadinessView, ParticipantReadinessView];
  const joinReadiness = await readJoinReadiness(
    context,
    sessionId,
    authority.grant,
  );

  return {
    session: {
      id: session.id,
      state: session.state,
      createdAt: session.created_at,
      startedAt: session.started_at,
      endedAt: session.ended_at,
      expiresAt: authority.grant.expiresAt,
      modalities: authority.grant.modalities,
      retentionPolicy: authority.grant.retentionPolicy,
    },
    participants,
    authority: {
      grantDigest: session.grant_digest,
      voiceManifestDigest: session.voice_manifest_digest,
      presenceManifestDigest: session.presence_manifest_digest,
      voiceDisclosure: authority.voiceManifest.disclosure,
      presenceDisclosure: authority.presenceManifest.disclosure,
    },
    consentHeads: authority.consentReceipts.map((receipt) => ({
      participant: receipt.participant,
      digest: receipt.canonicalDigest,
      issuedAt: receipt.issuedAt,
      expiresAt: receipt.expiresAt,
      modalities: receipt.modalities,
    })) as EncounterSnapshot["consentHeads"],
    readiness,
    joinAllowed:
      readinessAllowsJoin(joinReadiness) &&
      understanding?.outcome !== "mutually_understood",
    understanding,
    retainedArtifactCount: retainedCount.count ?? 0,
  };
}

export async function readCurrentEncounter(
  context: PrivateContext,
): Promise<EncounterSnapshot | null> {
  const { data, error } = await context.supabase
    .from("encounter_sessions")
    .select("id")
    .eq("owner_id", context.principal.userId)
    .in("state", OPEN_STATES)
    .order("created_at", { ascending: false })
    .limit(1)
    .maybeSingle();
  requireNoError(error, "Current encounter lookup failed");
  return data === null ? null : readEncounterSnapshot(context, data.id);
}

export async function issueOwnerJoinCredential(
  context: PrivateContext,
  sessionId: string,
) {
  const authority = await verifyStoredEncounterAuthority(context, sessionId);
  const readiness = await readJoinReadiness(
    context,
    sessionId,
    authority.grant,
  );
  if (!readinessAllowsJoin(readiness)) {
    throw new Error(
      "Join requires current bilateral consent, bilateral readiness, and one open encounter.",
    );
  }
  const credential = await mintOwnerJoinCredential(authority);
  const issuedAt = new Date();
  const expiresAt = new Date(
    issuedAt.getTime() + credential.expiresInSeconds * 1_000,
  );
  const tokenReceiptId = randomUUID();
  const { error: revokeError } = await context.supabase
    .from("encounter_join_tokens")
    .update({ revoked_at: issuedAt.toISOString() })
    .eq("owner_id", context.principal.userId)
    .eq("session_id", sessionId)
    .eq("participant_principal", authority.ownerIdentity.principal)
    .is("revoked_at", null);
  requireNoError(revokeError, "Previous join-token revocation failed");
  const { error: receiptError } = await context.supabase
    .from("encounter_join_tokens")
    .insert({
      id: tokenReceiptId,
      owner_id: context.principal.userId,
      session_id: sessionId,
      participant_principal: authority.ownerIdentity.principal,
      token_digest: digestCanonical(credential.token),
      issued_at: issuedAt.toISOString(),
      expires_at: expiresAt.toISOString(),
      revoked_at: null,
    });
  requireNoError(receiptError, "Join-token receipt persistence failed");
  const { data: transitioned, error: stateError } = await context.supabase
    .from("encounter_sessions")
    .update({
      state: "active",
      started_at: issuedAt.toISOString(),
    })
    .eq("owner_id", context.principal.userId)
    .eq("id", sessionId)
    .in("state", ["lobby", "active"])
    .select("id")
    .maybeSingle();
  if (stateError !== null || transitioned === null) {
    await context.supabase
      .from("encounter_join_tokens")
      .update({ revoked_at: new Date().toISOString() })
      .eq("owner_id", context.principal.userId)
      .eq("id", tokenReceiptId);
    throw new Error("Encounter activation failed.");
  }
  return { credential, tokenReceiptId };
}

export async function submitUnderstandingVersion(
  context: PrivateContext,
  sessionId: string,
  versionInput: unknown,
  requireCorrection: boolean,
): Promise<EncounterSnapshot> {
  const version = UnderstandingVersionSchema.parse(versionInput);
  assertUnderstandingVersionDigest(version);
  const grant = await readStoredGrant(context, sessionId);
  if (
    version.sessionId !== sessionId ||
    !grant.participants.some(
      ({ principal }) => principal === version.createdBy,
    )
  ) {
    throw new Error("Understanding author or session binding is invalid.");
  }
  const { data: currentRow, error: currentError } = await context.supabase
    .from("understanding_versions")
    .select("content")
    .eq("owner_id", context.principal.userId)
    .eq("session_id", sessionId)
    .order("version", { ascending: false })
    .limit(1)
    .maybeSingle();
  requireNoError(currentError, "Current understanding lookup failed");
  if (currentRow === null) {
    if (requireCorrection || version.version !== 1) {
      throw new Error("The first understanding must be version 1.");
    }
  } else {
    const current = UnderstandingVersionSchema.parse(currentRow.content);
    assertCorrectionTransition(current, version);
  }

  const { error: insertError } = await context.supabase
    .from("understanding_versions")
    .insert({
      id: version.versionId,
      owner_id: context.principal.userId,
      session_id: sessionId,
      version: version.version,
      canonical_digest: version.canonicalDigest,
      content: jsonValue(version),
      created_by: version.createdBy,
      created_at: version.createdAt,
    });
  requireNoError(insertError, "Understanding persistence failed");
  const { error: stateError } = await context.supabase
    .from("encounter_sessions")
    .update({ state: "understanding" })
    .eq("owner_id", context.principal.userId)
    .eq("id", sessionId)
    .in("state", ["active", "understanding"]);
  requireNoError(stateError, "Understanding state transition failed");
  return readEncounterSnapshot(context, sessionId);
}

export async function acknowledgeUnderstanding(
  context: PrivateContext,
  sessionId: string,
  acknowledgementInput: unknown,
): Promise<EncounterSnapshot> {
  const acknowledgement =
    UnderstandingAcknowledgementSchema.parse(acknowledgementInput);
  const grant = await readStoredGrant(context, sessionId);
  const participant = grant.participants.find(
    ({ principal }) => principal === acknowledgement.participant,
  );
  if (
    acknowledgement.sessionId !== sessionId ||
    participant === undefined ||
    participant.keyId !== acknowledgement.signature.keyId ||
    !(await verifySignedContract(
      acknowledgement,
      createPublicKeyResolver(context),
    ))
  ) {
    throw new Error("Understanding acknowledgement authority is invalid.");
  }
  const { data: version, error: versionError } = await context.supabase
    .from("understanding_versions")
    .select("canonical_digest")
    .eq("owner_id", context.principal.userId)
    .eq("session_id", sessionId)
    .eq("canonical_digest", acknowledgement.versionDigest)
    .maybeSingle();
  if (versionError !== null || version === null) {
    throw new Error("Acknowledged understanding version is unavailable.");
  }
  const { error } = await context.supabase
    .from("understanding_acknowledgements")
    .insert({
      id: acknowledgement.acknowledgementId,
      owner_id: context.principal.userId,
      session_id: sessionId,
      participant_principal: acknowledgement.participant,
      version_digest: acknowledgement.versionDigest,
      status: acknowledgement.status,
      correction: acknowledgement.correction,
      signature: jsonValue(acknowledgement.signature),
      acknowledged_at: acknowledgement.acknowledgedAt,
  });
  requireNoError(error, "Understanding acknowledgement persistence failed");
  return readEncounterSnapshot(context, sessionId);
}

export type RetainedArtifactClass =
  (typeof RETAINED_ARTIFACT_CLASSES)[number];

export function retainedHistoryWithdrawalTarget(
  sessionId: string,
  artifactClasses: readonly RetainedArtifactClass[],
): string {
  return `${sessionId}:${[...artifactClasses].sort().join(",")}`;
}

export async function finalizeEncounter(
  context: PrivateContext,
  sessionId: string,
  receiptInput: unknown,
) {
  const receipt = EncounterReceiptSchema.parse(receiptInput);
  const endState = receipt.endState;
  const grant = await readStoredGrant(context, sessionId);
  const grantConsentHeads = await readConsentReceiptsByDigest(
    context,
    sessionId,
    grant.consentRefs as [string, string],
  );
  const [authority, currentConsentHeads, sessionResult, artifactsResult] =
    await Promise.all([
      verifyEncounterGrant(context, grant, grantConsentHeads),
      readConsentHeads(context, sessionId),
      context.supabase
        .from("encounter_sessions")
        .select(
          "id,state,created_at,started_at,grant_digest,voice_manifest_digest,presence_manifest_digest",
        )
        .eq("owner_id", context.principal.userId)
        .eq("id", sessionId)
        .in("state", OPEN_STATES)
        .maybeSingle(),
      context.supabase
        .from("retained_encounter_artifacts")
        .select("content_digest")
        .eq("owner_id", context.principal.userId)
        .eq("session_id", sessionId)
        .order("content_digest", { ascending: true }),
    ]);
  if (
    sessionResult.error !== null ||
    sessionResult.data === null ||
    artifactsResult.error !== null ||
    artifactsResult.data === null
  ) {
    throw new Error("Terminal encounter evidence is unavailable.");
  }
  const session = sessionResult.data;
  if (
    session.grant_digest !== authority.grantDigest ||
    session.voice_manifest_digest !== authority.voiceManifestDigest ||
    session.presence_manifest_digest !== authority.presenceManifestDigest
  ) {
    throw new Error("Stored encounter authority digests are inconsistent.");
  }
  for (const consentHead of currentConsentHeads) {
    await verifyConsentReceipt(context, grant, consentHead);
  }
  const participantPrincipals = authority.grant.participants.map(
    ({ principal }) => principal,
  ) as [string, string];
  const understanding = await readUnderstanding(
    context,
    sessionId,
    participantPrincipals,
  );
  const mutual =
    understanding?.outcome === "mutually_understood"
      ? understanding.version.canonicalDigest
      : null;
  if (
    (mutual !== null && receipt.endState !== "mutually_understood") ||
    (mutual === null && receipt.endState === "mutually_understood")
  ) {
    throw new Error(
      "Terminal state does not match the bilateral understanding evidence.",
    );
  }
  const endedAt = new Date(receipt.endedAt);
  if (
    endedAt.getTime() < new Date(session.created_at).getTime() ||
    endedAt.getTime() > Date.now() + 5 * 60 * 1_000
  ) {
    throw new Error("Encounter receipt end time is invalid.");
  }
  const verifiedReceipt = await assertEncounterReceiptAuthority(receipt, {
      sessionId,
      authorizedSignerKeyIds: authority.grant.participants.map(
        ({ keyId }) => keyId,
      ),
      authorityDigests: {
        voiceManifest: authority.voiceManifestDigest,
        presenceManifest: authority.presenceManifestDigest,
        encounterGrant: authority.grantDigest,
      },
      consentHeads: currentConsentHeads.map(
        ({ canonicalDigest }) => canonicalDigest,
      ),
      retainedContentDigests: artifactsResult.data.map(
        ({ content_digest }) => content_digest,
      ),
      understandingVersionDigest: mutual,
      startedAt: session.started_at,
      resolvePublicKey: createPublicKeyResolver(context),
    });
  const receiptDigest = digestCanonical(verifiedReceipt);
  const { data, error } = await context.supabase.rpc(
    "finalize_encounter",
    {
      p_session_id: sessionId,
      p_end_state: endState,
      p_receipt_id: verifiedReceipt.receiptId,
      p_receipt_digest: receiptDigest,
      p_receipt: jsonValue(verifiedReceipt),
      p_ended_at: verifiedReceipt.endedAt,
      p_audit_metadata: {
        source: "encounter-app",
        signerKeyId: verifiedReceipt.signature.keyId,
      },
    },
  );
  requireNoError(error, "Atomic encounter finalization failed");
  return FinalizeEncounterResultSchema.parse(data);
}

export async function withdrawRetainedHistory(
  context: PrivateContext,
  sessionId: string,
  artifactClassesInput: readonly RetainedArtifactClass[],
) {
  const artifactClasses = [...new Set(artifactClassesInput)];
  if (
    artifactClasses.length === 0 ||
    artifactClasses.length !== artifactClassesInput.length ||
    artifactClasses.some(
      (candidate) =>
        !(RETAINED_ARTIFACT_CLASSES as readonly string[]).includes(candidate),
    )
  ) {
    throw new Error("Retained artifact withdrawal classes are invalid.");
  }
  const { data, error } = await context.supabase.rpc(
    "withdraw_retained_history",
    {
      p_session_id: sessionId,
      p_artifact_classes: artifactClasses,
      p_audit_metadata: {
        source: "encounter-app",
        requestedClasses: artifactClasses,
      },
    },
  );
  requireNoError(error, "Atomic retained-history withdrawal failed");
  return RetentionWithdrawalResultSchema.parse(data);
}

export interface RetainedHistory {
  decision: {
    digest: string;
    artifactClasses: string[];
    expiresAt: string | null;
    withdrawalTerms: string;
    createdAt: string;
    acknowledgementCount: number;
  } | null;
  artifacts: Array<{
    id: string;
    artifactClass: "transcript" | "understanding" | "memory-effects";
    contentDigest: string;
    content: Json;
    createdAt: string;
  }>;
}

export async function readRetainedHistory(
  context: PrivateContext,
  sessionId: string,
): Promise<RetainedHistory> {
  const [decisionResult, artifactResult] = await Promise.all([
    context.supabase
      .from("retention_decisions")
      .select(
        "id,decision_digest,artifact_classes,expires_at,withdrawal_terms,created_at",
      )
      .eq("owner_id", context.principal.userId)
      .eq("session_id", sessionId)
      .maybeSingle(),
    context.supabase
      .from("retained_encounter_artifacts")
      .select("id,artifact_class,content_digest,content,created_at")
      .eq("owner_id", context.principal.userId)
      .eq("session_id", sessionId)
      .order("created_at", { ascending: true }),
  ]);
  requireNoError(decisionResult.error, "Retention decision lookup failed");
  requireNoError(artifactResult.error, "Retained artifact lookup failed");
  let decision: RetainedHistory["decision"] = null;
  if (decisionResult.data !== null) {
    const acknowledgementResult = await context.supabase
      .from("retention_acknowledgements")
      .select("id", { count: "exact", head: true })
      .eq("owner_id", context.principal.userId)
      .eq("decision_id", decisionResult.data.id)
      .eq("decision_digest", decisionResult.data.decision_digest);
    requireNoError(
      acknowledgementResult.error,
      "Retention acknowledgement lookup failed",
    );
    decision = {
      digest: decisionResult.data.decision_digest,
      artifactClasses: decisionResult.data.artifact_classes,
      expiresAt: decisionResult.data.expires_at,
      withdrawalTerms: decisionResult.data.withdrawal_terms,
      createdAt: decisionResult.data.created_at,
      acknowledgementCount: acknowledgementResult.count ?? 0,
    };
  }
  return {
    decision,
    artifacts: (artifactResult.data ?? []).map((artifact) => ({
      id: artifact.id,
      artifactClass: artifact.artifact_class,
      contentDigest: artifact.content_digest,
      content: artifact.content,
      createdAt: artifact.created_at,
    })),
  };
}
