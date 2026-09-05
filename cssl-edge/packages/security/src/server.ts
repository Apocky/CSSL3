import { randomUUID } from "node:crypto";

import { digestCanonical } from "@apocky/contracts/server";
import { createServerClient } from "@supabase/ssr";
import type { SupabaseClient } from "@supabase/supabase-js";
import { createRemoteJWKSet, jwtVerify, type JWTPayload } from "jose";
import { cookies, headers } from "next/headers";

import type { Database, Json } from "./database";
import {
  authorizeOwnerClaims,
  parseOwnerAllowlist,
  validateMutationRequest,
  type AuthorizedOwner,
  type ExplicitConfirmation,
  type VerifiedAccessClaims,
} from "./policy";
import {
  mergeHeaders,
  privateNoStoreHeaders,
  securityHeaders,
} from "./headers";

export class PrivateAccessError extends Error {
  readonly code:
    | "configuration_missing"
    | "access_token_missing"
    | "access_token_invalid"
    | "session_missing"
    | "session_invalid"
    | "owner_denied"
    | "mutation_denied"
    | "audit_failed";
  readonly status: number;

  constructor(
    code: PrivateAccessError["code"],
    message: string,
    status: number,
  ) {
    super(message);
    this.name = "PrivateAccessError";
    this.code = code;
    this.status = status;
  }
}

export interface PrivateContext {
  principal: AuthorizedOwner;
  supabase: SupabaseClient<Database>;
  requestHeaders: Headers;
  allowedOrigins: ReadonlySet<string>;
}

interface PrivateEnvironment {
  supabaseUrl: string;
  supabasePublishableKey: string;
  cloudflareTeamDomain: string;
  cloudflareAudience: string;
  ownerUserIds: string;
  ownerEmails: string;
  allowedOrigins: ReadonlySet<string>;
}

const jwksByDomain = new Map<
  string,
  ReturnType<typeof createRemoteJWKSet>
>();

function requiredEnvironment(
  environment: NodeJS.ProcessEnv = process.env,
): PrivateEnvironment {
  const required = {
    supabaseUrl: environment.NEXT_PUBLIC_SUPABASE_URL,
    supabasePublishableKey:
      environment.NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY,
    cloudflareTeamDomain: environment.CLOUDFLARE_ACCESS_TEAM_DOMAIN,
    cloudflareAudience: environment.CLOUDFLARE_ACCESS_AUD,
    ownerUserIds: environment.APOCKY_OWNER_USER_IDS,
    ownerEmails: environment.APOCKY_OWNER_EMAILS,
    allowedOrigins: environment.APOCKY_ALLOWED_ORIGINS,
  };
  const missing = Object.entries(required)
    .filter(([, value]) => value === undefined || value.trim() === "")
    .map(([key]) => key);
  if (missing.length > 0) {
    throw new PrivateAccessError(
      "configuration_missing",
      `Private access configuration is incomplete: ${missing.join(", ")}`,
      503,
    );
  }

  const teamUrl = new URL(required.cloudflareTeamDomain!);
  if (teamUrl.protocol !== "https:") {
    throw new PrivateAccessError(
      "configuration_missing",
      "Cloudflare Access team domain must use HTTPS",
      503,
    );
  }

  const allowedOrigins = new Set(
    required.allowedOrigins!
      .split(",")
      .map((entry) => entry.trim())
      .filter(Boolean)
      .map((entry) => new URL(entry).origin),
  );
  if (allowedOrigins.size === 0) {
    throw new PrivateAccessError(
      "configuration_missing",
      "At least one allowed private origin is required",
      503,
    );
  }

  return {
    supabaseUrl: required.supabaseUrl!,
    supabasePublishableKey: required.supabasePublishableKey!,
    cloudflareTeamDomain: teamUrl.origin,
    cloudflareAudience: required.cloudflareAudience!,
    ownerUserIds: required.ownerUserIds!,
    ownerEmails: required.ownerEmails!,
    allowedOrigins,
  };
}

function remoteKeys(teamDomain: string) {
  const existing = jwksByDomain.get(teamDomain);
  if (existing !== undefined) {
    return existing;
  }
  const keys = createRemoteJWKSet(
    new URL("/cdn-cgi/access/certs", teamDomain),
  );
  jwksByDomain.set(teamDomain, keys);
  return keys;
}

function accessClaims(payload: JWTPayload): VerifiedAccessClaims {
  if (
    typeof payload.sub !== "string" ||
    typeof payload.email !== "string" ||
    (typeof payload.aud !== "string" && !Array.isArray(payload.aud)) ||
    typeof payload.exp !== "number" ||
    typeof payload.iss !== "string"
  ) {
    throw new PrivateAccessError(
      "access_token_invalid",
      "Cloudflare Access token lacks required verified claims",
      403,
    );
  }
  return {
    sub: payload.sub,
    email: payload.email,
    aud: payload.aud,
    exp: payload.exp,
    iss: payload.iss,
  };
}

export async function requirePrivateContext(): Promise<PrivateContext> {
  const environment = requiredEnvironment();
  const requestHeaders = new Headers(await headers());
  const accessToken = requestHeaders.get("cf-access-jwt-assertion");
  if (accessToken === null || accessToken === "") {
    throw new PrivateAccessError(
      "access_token_missing",
      "Cloudflare Access verification is required",
      401,
    );
  }

  let access: VerifiedAccessClaims;
  try {
    const verified = await jwtVerify(
      accessToken,
      remoteKeys(environment.cloudflareTeamDomain),
      {
        audience: environment.cloudflareAudience,
        issuer: environment.cloudflareTeamDomain,
      },
    );
    access = accessClaims(verified.payload);
  } catch (error) {
    if (error instanceof PrivateAccessError) {
      throw error;
    }
    throw new PrivateAccessError(
      "access_token_invalid",
      "Cloudflare Access verification failed",
      403,
    );
  }

  const cookieStore = await cookies();
  const supabase = createServerClient<Database>(
    environment.supabaseUrl,
    environment.supabasePublishableKey,
    {
      cookies: {
        getAll() {
          return cookieStore.getAll();
        },
        setAll(cookiesToSet) {
          try {
            for (const { name, options, value } of cookiesToSet) {
              cookieStore.set(name, value, {
                ...options,
                httpOnly: true,
                sameSite: "lax",
                secure: true,
              });
            }
          } catch {
            // Read-only Server Component contexts cannot refresh cookies.
            // The current request remains fail-closed if claims are invalid.
          }
        },
      },
    },
  );

  const { data, error } = await supabase.auth.getClaims();
  if (error !== null || data?.claims === undefined) {
    throw new PrivateAccessError(
      "session_invalid",
      "Application session verification failed",
      401,
    );
  }
  const subject = data.claims.sub;
  const email = data.claims.email;
  if (typeof subject !== "string" || typeof email !== "string") {
    throw new PrivateAccessError(
      "session_invalid",
      "Application session lacks required identity claims",
      401,
    );
  }

  const allowlist = parseOwnerAllowlist({
    userIds: environment.ownerUserIds,
    emails: environment.ownerEmails,
  });
  const principal = authorizeOwnerClaims(
    access,
    { sub: subject, email },
    allowlist,
  );
  if (principal === null) {
    throw new PrivateAccessError(
      "owner_denied",
      "Verified identities are not authorized for this private surface",
      403,
    );
  }

  return {
    principal,
    supabase,
    requestHeaders,
    allowedOrigins: environment.allowedOrigins,
  };
}

export function privateErrorResponse(error: unknown): Response {
  const accessError =
    error instanceof PrivateAccessError
      ? error
      : new PrivateAccessError(
          "owner_denied",
          "Private access denied",
          403,
        );
  return Response.json(
    {
      ok: false,
      error: {
        code: accessError.code,
        message: accessError.message,
      },
    },
    {
      status: accessError.status,
      headers: mergeHeaders(
        securityHeaders({ allowMedia: false }),
        privateNoStoreHeaders,
      ),
    },
  );
}

export function requireMutationGuard(
  context: PrivateContext,
  request: Request,
  options: {
    action: string;
    target: string;
    confirmation: unknown;
    now?: Date;
  },
): ExplicitConfirmation {
  try {
    return validateMutationRequest(request, {
      allowedOrigins: context.allowedOrigins,
      action: options.action,
      target: options.target,
      confirmation: options.confirmation,
      now: options.now,
    });
  } catch {
    throw new PrivateAccessError(
      "mutation_denied",
      "Mutation confirmation or origin validation failed",
      403,
    );
  }
}

export async function recordAuditReceipt(
  context: PrivateContext,
  input: {
    action: string;
    target: string;
    outcome: "allowed" | "completed" | "denied" | "failed";
    rollback: Json | null;
    metadata: Json;
  },
): Promise<{
  id: string;
  receiptDigest: `sha256:${string}`;
  createdAt: string;
}> {
  const id = randomUUID();
  const createdAt = new Date().toISOString();
  const receiptPayload = {
    id,
    ownerId: context.principal.userId,
    actorEmail: context.principal.email,
    action: input.action,
    target: input.target,
    outcome: input.outcome,
    rollback: input.rollback,
    metadata: input.metadata,
    createdAt,
  };
  const receiptDigest = digestCanonical(receiptPayload);
  const auditRow: Database["public"]["Tables"]["security_audit_receipts"]["Insert"] = {
    id,
    owner_id: context.principal.userId,
    action: input.action,
    target: input.target,
    outcome: input.outcome,
    rollback: input.rollback,
    metadata: input.metadata,
    receipt_digest: receiptDigest,
    created_at: createdAt,
  };
  const { error } = await context.supabase
    .from("security_audit_receipts")
    .insert(auditRow);
  if (error !== null) {
    throw new PrivateAccessError(
      "audit_failed",
      "Audit receipt persistence failed",
      503,
    );
  }
  return { id, receiptDigest, createdAt };
}

export { privateNoStoreHeaders, securityHeaders } from "./headers";
export type { Database, Json } from "./database";
