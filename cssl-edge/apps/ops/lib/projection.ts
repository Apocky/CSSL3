import { digestCanonical } from "@apocky/contracts/server";
import type { PrivateContext } from "@apocky/security/server";

import type { OpsAction } from "./confirmation";
import type { OpsReadSurface } from "./routes";

const OPEN_STATES = ["lobby", "active", "understanding"] as const;
const ROW_LIMIT = 200;

export interface OpsAllowedAction {
  action: OpsAction;
  target: string;
  expectedDigest: string;
}

export interface OpsSnapshot {
  generatedAt: string;
  runtime: {
    cloudflareAccess: "verified";
    applicationSession: "verified";
    ownerAllowlist: "verified";
    ownerUserId: string;
    allowedOriginCount: number;
    liveKitConfigured: boolean;
  };
  sessions: Array<{
    id: string;
    state: string;
    grantDigest: string;
    voiceManifestDigest: string;
    presenceManifestDigest: string;
    createdAt: string;
    startedAt: string | null;
    endedAt: string | null;
    allowedActions: OpsAllowedAction[];
  }>;
  authority: {
    participantKeys: Array<{
      keyId: string;
      principal: string;
      role: "owner" | "apocrypha";
      issuedAt: string;
      revokedAt: string | null;
    }>;
    manifests: Array<{
      id: string;
      kind: "presence" | "voice";
      digest: string;
      authorPrincipal: string;
      issuedAt: string;
      revokedAt: string | null;
    }>;
  };
  consent: Array<{
    sessionId: string;
    participant: string;
    modality: "audio" | "video" | "captions" | "text";
    state: "granted" | "revoked";
    receiptDigest: string;
    createdAt: string;
  }>;
  security: Array<{
    id: string;
    action: string;
    target: string;
    outcome: "allowed" | "denied" | "completed" | "failed";
    receiptDigest: string;
    createdAt: string;
  }>;
  deployment: Array<{
    id: string;
    surface: "site" | "encounter" | "ops" | "media";
    environment: "staging" | "production";
    commitSha: string;
    buildIdentity: string;
    state: "preview" | "canary" | "promoted" | "rolled_back" | "failed";
    rollbackTarget: string | null;
    createdAt: string;
  }>;
  retention: {
    decisions: Array<{
      id: string;
      sessionId: string;
      decisionDigest: string;
      artifactClasses: string[];
      expiresAt: string | null;
      createdAt: string;
      acknowledgementCount: number;
      artifactCount: number;
    }>;
    artifacts: Array<{
      id: string;
      sessionId: string;
      decisionId: string;
      artifactClass: "transcript" | "understanding" | "memory-effects";
      contentDigest: string;
      createdAt: string;
    }>;
    withdrawals: Array<{
      id: string;
      sessionId: string;
      artifactClasses: string[];
      deletedArtifactCount: number;
      workflowState: "completed" | "pending_upstream" | "failed";
      upstreamReceiptDigest: string | null;
      requestedAt: string;
      completedAt: string | null;
      evidenceDigest: string;
      allowedAction: OpsAllowedAction | null;
    }>;
  };
}

function requireNoError(
  error: { message: string } | null,
  label: string,
): void {
  if (error !== null) {
    throw new Error(`${label}: ${error.message}`);
  }
}

export async function readOpsSnapshot(
  context: PrivateContext,
): Promise<OpsSnapshot> {
  const ownerId = context.principal.userId;
  const [
    sessionsResult,
    keysResult,
    manifestsResult,
    consentsResult,
    auditResult,
    deploymentResult,
    decisionsResult,
    acknowledgementsResult,
    artifactsResult,
    withdrawalsResult,
  ] = await Promise.all([
    context.supabase
      .from("encounter_sessions")
      .select(
        "id,state,grant_digest,voice_manifest_digest,presence_manifest_digest,created_at,started_at,ended_at",
      )
      .eq("owner_id", ownerId)
      .order("created_at", { ascending: false })
      .limit(ROW_LIMIT),
    context.supabase
      .from("participant_keys")
      .select("key_id,principal,role,issued_at,revoked_at")
      .eq("owner_id", ownerId)
      .order("issued_at", { ascending: false })
      .limit(ROW_LIMIT),
    context.supabase
      .from("authority_manifests")
      .select(
        "id,kind,digest,author_principal,issued_at,revoked_at",
      )
      .eq("owner_id", ownerId)
      .order("issued_at", { ascending: false })
      .limit(ROW_LIMIT),
    context.supabase
      .from("encounter_consents")
      .select(
        "session_id,participant_principal,modality,state,receipt_digest,created_at",
      )
      .eq("owner_id", ownerId)
      .order("created_at", { ascending: false })
      .limit(ROW_LIMIT),
    context.supabase
      .from("security_audit_receipts")
      .select(
        "id,action,target,outcome,receipt_digest,created_at",
      )
      .eq("owner_id", ownerId)
      .order("created_at", { ascending: false })
      .limit(ROW_LIMIT),
    context.supabase
      .from("deployment_records")
      .select(
        "id,surface,environment,commit_sha,build_identity,state,rollback_target,created_at",
      )
      .eq("owner_id", ownerId)
      .order("created_at", { ascending: false })
      .limit(ROW_LIMIT),
    context.supabase
      .from("retention_decisions")
      .select(
        "id,session_id,decision_digest,artifact_classes,expires_at,created_at",
      )
      .eq("owner_id", ownerId)
      .order("created_at", { ascending: false })
      .limit(ROW_LIMIT),
    context.supabase
      .from("retention_acknowledgements")
      .select("decision_id")
      .eq("owner_id", ownerId)
      .limit(ROW_LIMIT),
    context.supabase
      .from("retained_encounter_artifacts")
      .select(
        "id,session_id,retention_decision_id,artifact_class,content_digest,created_at",
      )
      .eq("owner_id", ownerId)
      .order("created_at", { ascending: false })
      .limit(ROW_LIMIT),
    context.supabase
      .from("retention_withdrawals")
      .select(
        "id,session_id,artifact_classes,artifact_digests,deleted_artifact_count,workflow_state,upstream_receipt_digest,requested_at,completed_at",
      )
      .eq("owner_id", ownerId)
      .order("requested_at", { ascending: false })
      .limit(ROW_LIMIT),
  ]);

  for (const [label, result] of [
    ["sessions", sessionsResult],
    ["participant keys", keysResult],
    ["authority manifests", manifestsResult],
    ["consent", consentsResult],
    ["security receipts", auditResult],
    ["deployment records", deploymentResult],
    ["retention decisions", decisionsResult],
    ["retention acknowledgements", acknowledgementsResult],
    ["retained artifacts", artifactsResult],
    ["retention withdrawals", withdrawalsResult],
  ] as const) {
    requireNoError(result.error, `${label} projection failed`);
  }

  const acknowledgementCount = new Map<string, number>();
  for (const row of acknowledgementsResult.data ?? []) {
    acknowledgementCount.set(
      row.decision_id,
      (acknowledgementCount.get(row.decision_id) ?? 0) + 1,
    );
  }
  const artifactsByDecision = new Map<string, number>();
  const artifactsBySession = new Map<string, number>();
  for (const row of artifactsResult.data ?? []) {
    artifactsByDecision.set(
      row.retention_decision_id,
      (artifactsByDecision.get(row.retention_decision_id) ?? 0) + 1,
    );
    artifactsBySession.set(
      row.session_id,
      (artifactsBySession.get(row.session_id) ?? 0) + 1,
    );
  }
  const decisionBySession = new Map(
    (decisionsResult.data ?? []).map((decision) => [
      decision.session_id,
      decision,
    ]),
  );

  return {
    generatedAt: new Date().toISOString(),
    runtime: {
      cloudflareAccess: "verified",
      applicationSession: "verified",
      ownerAllowlist: "verified",
      ownerUserId: ownerId,
      allowedOriginCount: context.allowedOrigins.size,
      liveKitConfigured: [
        process.env.LIVEKIT_API_KEY,
        process.env.LIVEKIT_API_SECRET,
        process.env.NEXT_PUBLIC_LIVEKIT_URL,
        process.env.LIVEKIT_E2EE_MASTER_KEY,
      ].every((value) => Boolean(value?.trim())),
    },
    sessions: (sessionsResult.data ?? []).map((session) => {
      const allowedActions: OpsAllowedAction[] = [];
      if ((OPEN_STATES as readonly string[]).includes(session.state)) {
        allowedActions.push(
          {
            action: "end_encounter",
            target: session.id,
            expectedDigest: session.grant_digest,
          },
          {
            action: "revoke_encounter_grant",
            target: session.id,
            expectedDigest: session.grant_digest,
          },
        );
      }
      const decision = decisionBySession.get(session.id);
      if (
        (artifactsBySession.get(session.id) ?? 0) > 0 &&
        decision !== undefined
      ) {
        allowedActions.push({
          action: "delete_retained_history",
          target: session.id,
          expectedDigest: decision.decision_digest,
        });
      }
      return {
        id: session.id,
        state: session.state,
        grantDigest: session.grant_digest,
        voiceManifestDigest: session.voice_manifest_digest,
        presenceManifestDigest: session.presence_manifest_digest,
        createdAt: session.created_at,
        startedAt: session.started_at,
        endedAt: session.ended_at,
        allowedActions,
      };
    }),
    authority: {
      participantKeys: (keysResult.data ?? []).map((row) => ({
        keyId: row.key_id,
        principal: row.principal,
        role: row.role,
        issuedAt: row.issued_at,
        revokedAt: row.revoked_at,
      })),
      manifests: (manifestsResult.data ?? []).map((row) => ({
        id: row.id,
        kind: row.kind,
        digest: row.digest,
        authorPrincipal: row.author_principal,
        issuedAt: row.issued_at,
        revokedAt: row.revoked_at,
      })),
    },
    consent: (consentsResult.data ?? []).map((row) => ({
      sessionId: row.session_id,
      participant: row.participant_principal,
      modality: row.modality,
      state: row.state,
      receiptDigest: row.receipt_digest,
      createdAt: row.created_at,
    })),
    security: (auditResult.data ?? []).map((row) => ({
      id: row.id,
      action: row.action,
      target: row.target,
      outcome: row.outcome,
      receiptDigest: row.receipt_digest,
      createdAt: row.created_at,
    })),
    deployment: (deploymentResult.data ?? []).map((row) => ({
      id: row.id,
      surface: row.surface,
      environment: row.environment,
      commitSha: row.commit_sha,
      buildIdentity: row.build_identity,
      state: row.state,
      rollbackTarget: row.rollback_target,
      createdAt: row.created_at,
    })),
    retention: {
      decisions: (decisionsResult.data ?? []).map((row) => ({
        id: row.id,
        sessionId: row.session_id,
        decisionDigest: row.decision_digest,
        artifactClasses: row.artifact_classes,
        expiresAt: row.expires_at,
        createdAt: row.created_at,
        acknowledgementCount: acknowledgementCount.get(row.id) ?? 0,
        artifactCount: artifactsByDecision.get(row.id) ?? 0,
      })),
      artifacts: (artifactsResult.data ?? []).map((row) => ({
        id: row.id,
        sessionId: row.session_id,
        decisionId: row.retention_decision_id,
        artifactClass: row.artifact_class,
        contentDigest: row.content_digest,
        createdAt: row.created_at,
      })),
      withdrawals: (withdrawalsResult.data ?? []).map((row) => {
        const evidenceDigest = digestCanonical({
          withdrawalId: row.id,
          sessionId: row.session_id,
          artifactClasses: row.artifact_classes,
          artifactDigests: row.artifact_digests,
          deletedArtifactCount: row.deleted_artifact_count,
          workflowState: row.workflow_state,
          requestedAt: row.requested_at,
        });
        return {
          id: row.id,
          sessionId: row.session_id,
          artifactClasses: row.artifact_classes,
          deletedArtifactCount: row.deleted_artifact_count,
          workflowState: row.workflow_state,
          upstreamReceiptDigest: row.upstream_receipt_digest,
          requestedAt: row.requested_at,
          completedAt: row.completed_at,
          evidenceDigest,
          allowedAction:
            row.workflow_state === "pending_upstream"
              ? {
                  action: "complete_retention_withdrawal",
                  target: row.id,
                  expectedDigest: evidenceDigest,
                }
              : null,
        };
      }),
    },
  };
}

export async function readOpsSurface(
  context: PrivateContext,
  surface: OpsReadSurface,
): Promise<unknown> {
  const snapshot = await readOpsSnapshot(context);
  return snapshot[surface];
}
