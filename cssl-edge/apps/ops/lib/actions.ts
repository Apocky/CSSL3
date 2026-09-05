import {
  EncounterConsentReceiptSchema,
  EncounterGrantSchema,
  EncounterReceiptSchema,
  UnderstandingAcknowledgementSchema,
} from "@apocky/contracts";
import {
  assertEncounterReceiptAuthority,
  computeEncounterConsentDigest,
  digestCanonical,
  verifySignedContract,
  type PublicKeyResolver,
} from "@apocky/contracts/server";
import type {
  Json,
  PrivateContext,
} from "@apocky/security/server";
import { z } from "zod";

import type { OpsCommand } from "./confirmation";

const PublicKeyJwkSchema = z
  .object({
    kty: z.literal("OKP"),
    crv: z.literal("Ed25519"),
    x: z.string().min(43).max(128),
    kid: z.string().optional(),
  })
  .strict();

const RpcResultSchema = z.record(z.string(), z.unknown());

function jsonValue(value: unknown): Json {
  return JSON.parse(JSON.stringify(value)) as Json;
}

function resolver(context: PrivateContext): PublicKeyResolver {
  const cache = new Map<string, JsonWebKey | null>();
  return async (keyId) => {
    if (cache.has(keyId)) return cache.get(keyId) ?? null;
    const { data, error } = await context.supabase
      .from("participant_keys")
      .select("key_id,public_key_jwk,revoked_at")
      .eq("owner_id", context.principal.userId)
      .eq("key_id", keyId)
      .is("revoked_at", null)
      .maybeSingle();
    if (error !== null || data === null || data.revoked_at !== null) {
      cache.set(keyId, null);
      return null;
    }
    const key = PublicKeyJwkSchema.parse(data.public_key_jwk);
    cache.set(keyId, key);
    return key;
  };
}

async function finalizeEncounter(
  context: PrivateContext,
  command: OpsCommand,
) {
  const receipt = EncounterReceiptSchema.parse(command.encounterReceipt);
  const endState = receipt.endState;
  const resolvePublicKey = resolver(context);
  const [sessionResult, consentResult, artifactResult] = await Promise.all([
    context.supabase
      .from("encounter_sessions")
      .select(
        "id,state,grant,grant_digest,voice_manifest_digest,presence_manifest_digest,created_at,started_at",
      )
      .eq("owner_id", context.principal.userId)
      .eq("id", command.target)
      .in("state", ["lobby", "active", "understanding"])
      .maybeSingle(),
    context.supabase
      .from("encounter_consents")
      .select(
        "id,participant_principal,receipt_digest,receipt,created_at",
      )
      .eq("owner_id", context.principal.userId)
      .eq("session_id", command.target)
      .order("created_at", { ascending: false })
      .order("id", { ascending: false }),
    context.supabase
      .from("retained_encounter_artifacts")
      .select("content_digest")
      .eq("owner_id", context.principal.userId)
      .eq("session_id", command.target)
      .order("content_digest", { ascending: true }),
  ]);
  if (
    sessionResult.error !== null ||
    sessionResult.data === null ||
    consentResult.error !== null ||
    consentResult.data === null ||
    artifactResult.error !== null ||
    artifactResult.data === null
  ) {
    throw new Error("Live encounter finalization evidence is unavailable.");
  }
  const session = sessionResult.data;
  const grant = EncounterGrantSchema.parse(session.grant);
  if (
    grant.sessionId !== command.target ||
    digestCanonical(grant) !== session.grant_digest ||
    grant.authorityDigests.voiceManifest !==
      session.voice_manifest_digest ||
    grant.authorityDigests.presenceManifest !==
      session.presence_manifest_digest ||
    command.expectedDigest !== session.grant_digest
  ) {
    throw new Error("Encounter grant evidence changed.");
  }
  for (const identity of grant.participants) {
    if (!(await verifySignedContract(identity, resolvePublicKey))) {
      throw new Error("Encounter participant signature is invalid.");
    }
  }
  if (
    !grant.participants.some(
      ({ keyId }) => keyId === grant.signature.keyId,
    ) ||
    !(await verifySignedContract(grant, resolvePublicKey))
  ) {
    throw new Error("Encounter grant signature is invalid.");
  }

  const consentHeads = new Map<
    string,
    ReturnType<typeof EncounterConsentReceiptSchema.parse>
  >();
  for (const row of consentResult.data) {
    if (consentHeads.has(row.participant_principal)) continue;
    const consent = EncounterConsentReceiptSchema.parse(row.receipt);
    const participant = grant.participants.find(
      ({ principal }) => principal === consent.participant,
    );
    const { canonicalDigest, signature: _signature, ...unsigned } = consent;
    if (
      participant === undefined ||
      participant.keyId !== consent.signature.keyId ||
      consent.canonicalDigest !== row.receipt_digest ||
      computeEncounterConsentDigest(unsigned) !== canonicalDigest ||
      !(await verifySignedContract(consent, resolvePublicKey))
    ) {
      throw new Error("Current consent head authority is invalid.");
    }
    consentHeads.set(consent.participant, consent);
  }
  if (consentHeads.size !== 2) {
    throw new Error("Bilateral consent heads are incomplete.");
  }

  let understandingDigest: string | null = null;
  if (endState === "mutually_understood") {
    if (receipt.understandingVersionDigest === null) {
      throw new Error("Mutual closure lacks an understanding digest.");
    }
    const { data, error } = await context.supabase
      .from("understanding_acknowledgements")
      .select(
        "id,participant_principal,version_digest,status,correction,signature,acknowledged_at",
      )
      .eq("owner_id", context.principal.userId)
      .eq("session_id", command.target)
      .eq("version_digest", receipt.understandingVersionDigest)
      .eq("status", "understood");
    if (error !== null || data === null) {
      throw new Error("Understanding acknowledgements are unavailable.");
    }
    const acknowledged = new Set<string>();
    for (const row of data) {
      if (acknowledged.has(row.participant_principal)) continue;
      const acknowledgement = UnderstandingAcknowledgementSchema.parse({
        acknowledgementId: row.id,
        sessionId: command.target,
        participant: row.participant_principal,
        versionDigest: row.version_digest,
        status: row.status,
        correction: row.correction,
        acknowledgedAt: row.acknowledged_at,
        signature: row.signature,
      });
      const participant = grant.participants.find(
        ({ principal }) => principal === acknowledgement.participant,
      );
      if (
        participant === undefined ||
        participant.keyId !== acknowledgement.signature.keyId ||
        !(await verifySignedContract(acknowledgement, resolvePublicKey))
      ) {
        throw new Error("Understanding acknowledgement signature is invalid.");
      }
      acknowledged.add(acknowledgement.participant);
    }
    if (
      acknowledged.size !== 2 ||
      !grant.participants.every(({ principal }) =>
        acknowledged.has(principal),
      )
    ) {
      throw new Error("Mutual closure lacks bilateral acknowledgements.");
    }
    understandingDigest = receipt.understandingVersionDigest;
  }

  const verified = await assertEncounterReceiptAuthority(receipt, {
    sessionId: command.target,
    authorizedSignerKeyIds: grant.participants.map(({ keyId }) => keyId),
    authorityDigests: {
      voiceManifest: session.voice_manifest_digest,
      presenceManifest: session.presence_manifest_digest,
      encounterGrant: session.grant_digest,
    },
    consentHeads: [...consentHeads.values()].map(
      ({ canonicalDigest }) => canonicalDigest,
    ),
    retainedContentDigests: artifactResult.data.map(
      ({ content_digest }) => content_digest,
    ),
    understandingVersionDigest: understandingDigest,
    startedAt: session.started_at,
    resolvePublicKey,
  });
  const { data, error } = await context.supabase.rpc(
    "finalize_encounter",
    {
      p_session_id: command.target,
      p_end_state: endState,
      p_receipt_id: verified.receiptId,
      p_receipt_digest: digestCanonical(verified),
      p_receipt: jsonValue(verified),
      p_ended_at: verified.endedAt,
      p_audit_metadata: {
        source: "ops-app",
        requestedAction: command.action,
        signerKeyId: verified.signature.keyId,
      },
    },
  );
  if (error !== null) {
    throw new Error(`Atomic encounter finalization failed: ${error.message}`);
  }
  return RpcResultSchema.parse(data);
}

export async function executeOpsAction(
  context: PrivateContext,
  command: OpsCommand,
) {
  if (
    command.action === "end_encounter" ||
    command.action === "revoke_encounter_grant"
  ) {
    return finalizeEncounter(context, command);
  }
  if (command.action === "delete_retained_history") {
    const { data, error } = await context.supabase.rpc(
      "withdraw_retained_history",
      {
        p_session_id: command.target,
        p_artifact_classes: [
          "transcript",
          "understanding",
          "memory-effects",
        ],
        p_audit_metadata: {
          source: "ops-app",
          confirmedDecisionDigest: command.expectedDigest,
        },
      },
    );
    if (error !== null) {
      throw new Error(`Atomic history withdrawal failed: ${error.message}`);
    }
    return RpcResultSchema.parse(data);
  }
  if (command.upstreamReceiptDigest === undefined) {
    throw new Error("Upstream withdrawal receipt digest is required.");
  }
  const { data, error } = await context.supabase.rpc(
    "complete_retention_withdrawal",
    {
      p_withdrawal_id: command.target,
      p_upstream_receipt_digest: command.upstreamReceiptDigest,
      p_audit_metadata: {
        source: "ops-app",
        confirmedWithdrawalDigest: command.expectedDigest,
      },
    },
  );
  if (error !== null) {
    throw new Error(`Retention completion failed: ${error.message}`);
  }
  return RpcResultSchema.parse(data);
}
