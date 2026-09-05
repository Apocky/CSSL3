import {
  EncounterConsentReceiptSchema,
  EncounterGrantSchema,
  type EncounterConsentReceipt,
  type EncounterGrant,
} from "@apocky/contracts";
import {
  assertEncounterAuthority,
  computeEncounterConsentDigest,
  digestCanonical,
  type EncounterAuthorityBundle,
  type PublicKeyResolver,
  verifySignedContract,
} from "@apocky/contracts/server";
import type { PrivateContext } from "@apocky/security/server";
import { z } from "zod";

const PublicKeyJwkSchema = z
  .object({
    kty: z.literal("OKP"),
    crv: z.literal("Ed25519"),
    x: z.string().min(43).max(128),
    kid: z.string().optional(),
  })
  .strict();

const JoinReadinessSchema = z
  .object({
    sessionId: z.string().uuid(),
    state: z.enum(["lobby", "active", "understanding"]),
    bilateralConsent: z.boolean(),
    grantUnrevoked: z.boolean(),
    nonceValid: z.boolean(),
    singleSession: z.boolean(),
    ownerReady: z.boolean(),
    apocryphaReady: z.boolean(),
    expiresAt: z.string().datetime({ offset: true }),
  })
  .strict();

export type JoinReadiness = z.infer<typeof JoinReadinessSchema>;

export function createPublicKeyResolver(
  context: PrivateContext,
): PublicKeyResolver {
  const cache = new Map<string, JsonWebKey | null>();
  return async (keyId) => {
    if (cache.has(keyId)) return cache.get(keyId) ?? null;
    const { data, error } = await context.supabase
      .from("participant_keys")
      .select("key_id,public_key_jwk,issued_at,revoked_at")
      .eq("owner_id", context.principal.userId)
      .eq("key_id", keyId)
      .is("revoked_at", null)
      .maybeSingle();
    if (
      error !== null ||
      data === null ||
      data.key_id !== keyId ||
      data.revoked_at !== null ||
      new Date(data.issued_at).getTime() > Date.now() + 5 * 60 * 1_000
    ) {
      cache.set(keyId, null);
      return null;
    }
    const key = PublicKeyJwkSchema.parse(data.public_key_jwk);
    cache.set(keyId, key);
    return key;
  };
}

async function readActiveManifest(
  context: PrivateContext,
  digest: string,
  kind: "presence" | "voice",
): Promise<unknown> {
  const { data, error } = await context.supabase
    .from("authority_manifests")
    .select("manifest,digest,kind,revoked_at")
    .eq("owner_id", context.principal.userId)
    .eq("digest", digest)
    .eq("kind", kind)
    .is("revoked_at", null)
    .maybeSingle();
  if (
    error !== null ||
    data === null ||
    data.digest !== digest ||
    data.kind !== kind ||
    data.revoked_at !== null
  ) {
    throw new Error(`${kind} authority is unavailable or revoked.`);
  }
  return data.manifest;
}

async function assertParticipantBindings(
  context: PrivateContext,
  grant: EncounterGrant,
): Promise<void> {
  for (const participant of grant.participants) {
    const { data, error } = await context.supabase
      .from("participant_keys")
      .select("key_id,principal,role,revoked_at")
      .eq("owner_id", context.principal.userId)
      .eq("key_id", participant.keyId)
      .is("revoked_at", null)
      .maybeSingle();
    if (
      error !== null ||
      data === null ||
      data.revoked_at !== null ||
      data.key_id !== participant.keyId ||
      data.principal !== participant.principal ||
      data.role !== participant.role
    ) {
      throw new Error("Encounter participant identity is not trusted.");
    }
  }
}

export async function readConsentHeads(
  context: PrivateContext,
  sessionId: string,
): Promise<readonly [EncounterConsentReceipt, EncounterConsentReceipt]> {
  const { data, error } = await context.supabase
    .from("encounter_consents")
    .select("id,participant_principal,receipt_digest,receipt,created_at")
    .eq("owner_id", context.principal.userId)
    .eq("session_id", sessionId)
    .order("created_at", { ascending: false })
    .order("id", { ascending: false });
  if (error !== null || data === null) {
    throw new Error("Encounter consent heads are unavailable.");
  }
  const heads = new Map<string, EncounterConsentReceipt>();
  for (const row of data) {
    if (heads.has(row.participant_principal)) continue;
    const receipt = EncounterConsentReceiptSchema.parse(row.receipt);
    if (
      receipt.participant !== row.participant_principal ||
      receipt.canonicalDigest !== row.receipt_digest
    ) {
      throw new Error("Persisted consent provenance is inconsistent.");
    }
    heads.set(receipt.participant, receipt);
  }
  if (heads.size !== 2) {
    throw new Error("Bilateral consent heads are incomplete.");
  }
  return [...heads.values()] as [
    EncounterConsentReceipt,
    EncounterConsentReceipt,
  ];
}

export async function readConsentReceiptsByDigest(
  context: PrivateContext,
  sessionId: string,
  digests: readonly [string, string],
): Promise<readonly [EncounterConsentReceipt, EncounterConsentReceipt]> {
  const { data, error } = await context.supabase
    .from("encounter_consents")
    .select("participant_principal,receipt_digest,receipt")
    .eq("owner_id", context.principal.userId)
    .eq("session_id", sessionId)
    .in("receipt_digest", [...digests]);
  if (error !== null || data === null) {
    throw new Error("Grant-bound consent provenance is unavailable.");
  }
  const receipts = new Map<string, EncounterConsentReceipt>();
  for (const row of data) {
    if (receipts.has(row.receipt_digest)) continue;
    const receipt = EncounterConsentReceiptSchema.parse(row.receipt);
    if (
      receipt.sessionId !== sessionId ||
      receipt.participant !== row.participant_principal ||
      receipt.canonicalDigest !== row.receipt_digest
    ) {
      throw new Error("Grant-bound consent provenance is inconsistent.");
    }
    receipts.set(row.receipt_digest, receipt);
  }
  if (
    receipts.size !== 2 ||
    digests.some((digest) => !receipts.has(digest))
  ) {
    throw new Error("Grant-bound bilateral consent is incomplete.");
  }
  return digests.map((digest) => receipts.get(digest)!) as [
    EncounterConsentReceipt,
    EncounterConsentReceipt,
  ];
}

export async function verifyEncounterGrant(
  context: PrivateContext,
  grantInput: unknown,
  consentReceiptsInput?: unknown,
): Promise<EncounterAuthorityBundle> {
  const grant = EncounterGrantSchema.parse(grantInput);
  const [voiceManifest, presenceManifest, consentReceipts] = await Promise.all([
    readActiveManifest(
      context,
      grant.authorityDigests.voiceManifest,
      "voice",
    ),
    readActiveManifest(
      context,
      grant.authorityDigests.presenceManifest,
      "presence",
    ),
    consentReceiptsInput === undefined
      ? readConsentHeads(context, grant.sessionId)
      : Promise.resolve(
          EncounterConsentReceiptSchema.array()
            .length(2)
            .parse(consentReceiptsInput),
        ),
  ]);
  await assertParticipantBindings(context, grant);
  const authority = await assertEncounterAuthority(
    { grant, voiceManifest, presenceManifest, consentReceipts },
    {
      resolvePublicKey: createPublicKeyResolver(context),
    },
  );
  return authority;
}

export async function readStoredGrant(
  context: PrivateContext,
  sessionId: string,
): Promise<EncounterGrant> {
  const { data, error } = await context.supabase
    .from("encounter_sessions")
    .select("id,grant,state")
    .eq("owner_id", context.principal.userId)
    .eq("id", sessionId)
    .maybeSingle();
  if (
    error !== null ||
    data === null ||
    data.id !== sessionId ||
    !["lobby", "active", "understanding"].includes(data.state)
  ) {
    throw new Error("Encounter session is unavailable.");
  }
  return EncounterGrantSchema.parse(data.grant);
}

export async function verifyStoredEncounterAuthority(
  context: PrivateContext,
  sessionId: string,
): Promise<EncounterAuthorityBundle> {
  const grant = await readStoredGrant(context, sessionId);
  if (grant.sessionId !== sessionId) {
    throw new Error("Stored grant does not bind this encounter.");
  }
  return verifyEncounterGrant(context, grant);
}

export async function readJoinReadiness(
  context: PrivateContext,
  sessionId: string,
  grant: EncounterGrant,
): Promise<JoinReadiness> {
  const [{ data: session, error: sessionError }, openSessions, readyRows] =
    await Promise.all([
      context.supabase
        .from("encounter_sessions")
        .select("id,state,grant_nonce_digest")
        .eq("owner_id", context.principal.userId)
        .eq("id", sessionId)
        .maybeSingle(),
      context.supabase
        .from("encounter_sessions")
        .select("id")
        .eq("owner_id", context.principal.userId)
        .in("state", ["lobby", "active", "understanding"]),
      context.supabase
        .from("encounter_readiness")
        .select("participant_principal,ready,modalities")
        .eq("owner_id", context.principal.userId)
        .eq("session_id", sessionId),
    ]);
  if (
    sessionError !== null ||
    session === null ||
    openSessions.error !== null ||
    openSessions.data === null ||
    readyRows.error !== null ||
    readyRows.data === null
  ) {
    throw new Error("Encounter readiness is unavailable.");
  }
  const byParticipant = new Map(
    readyRows.data.map((row) => [row.participant_principal, row]),
  );
  const owner = grant.participants.find(({ role }) => role === "owner");
  const apocrypha = grant.participants.find(
    ({ role }) => role === "apocrypha",
  );
  if (owner === undefined || apocrypha === undefined) {
    throw new Error("Encounter participant roles are incomplete.");
  }
  const isReady = (principal: string): boolean => {
    const row = byParticipant.get(principal);
    return (
      row?.ready === true &&
      grant.modalities.every((modality) =>
        row.modalities.includes(modality),
      )
    );
  };
  const readiness = JoinReadinessSchema.parse({
    sessionId,
    state: session.state,
    bilateralConsent: true,
    grantUnrevoked: session.state !== "revoked",
    nonceValid:
      session.grant_nonce_digest === digestCanonical(grant.nonce),
    singleSession:
      openSessions.data.length === 1 &&
      openSessions.data[0]?.id === sessionId,
    ownerReady: isReady(owner.principal),
    apocryphaReady: isReady(apocrypha.principal),
    expiresAt: grant.expiresAt,
  });
  return readiness;
}

export function readinessAllowsJoin(readiness: JoinReadiness): boolean {
  return (
    readiness.bilateralConsent &&
    readiness.grantUnrevoked &&
    readiness.nonceValid &&
    readiness.singleSession &&
    readiness.ownerReady &&
    readiness.apocryphaReady &&
    new Date(readiness.expiresAt).getTime() > Date.now()
  );
}

export async function verifyConsentReceipt(
  context: PrivateContext,
  grant: EncounterGrant,
  input: unknown,
): Promise<EncounterConsentReceipt> {
  const receipt = EncounterConsentReceiptSchema.parse(input);
  const participant = grant.participants.find(
    ({ principal }) => principal === receipt.participant,
  );
  const { canonicalDigest, signature: _signature, ...unsigned } = receipt;
  if (
    participant === undefined ||
    participant.keyId !== receipt.signature.keyId ||
    receipt.sessionId !== grant.sessionId ||
    computeEncounterConsentDigest(unsigned) !== canonicalDigest ||
    !(await verifySignedContract(receipt, createPublicKeyResolver(context)))
  ) {
    throw new Error("Consent receipt authority verification failed.");
  }
  if (
    receipt.expiresAt !== null &&
    new Date(receipt.expiresAt).getTime() <= Date.now()
  ) {
    throw new Error("Consent receipt is expired.");
  }
  return receipt;
}
