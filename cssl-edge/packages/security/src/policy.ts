import { digestCanonical } from "@apocky/contracts/server";

import {
  ExplicitConfirmationSchema,
  ExplicitConfirmationUnsignedSchema,
  type ExplicitConfirmation,
} from "./confirmation";

export interface VerifiedAccessClaims {
  sub: string;
  email: string;
  aud: string | readonly string[];
  exp: number;
  iss: string;
}

export interface VerifiedSupabaseClaims {
  sub: string;
  email: string;
}

export interface OwnerAllowlist {
  userIds: ReadonlySet<string>;
  emails: ReadonlySet<string>;
}

export interface AuthorizedOwner {
  userId: string;
  email: string;
  cloudflareSubject: string;
  cloudflareEmail: string;
}

function normalizeEmail(value: string): string {
  return value.trim().toLowerCase();
}

export function parseOwnerAllowlist(input: {
  userIds: string;
  emails: string;
}): OwnerAllowlist {
  const userIds = new Set(
    input.userIds
      .split(",")
      .map((entry) => entry.trim())
      .filter(Boolean),
  );
  const emails = new Set(
    input.emails
      .split(",")
      .map(normalizeEmail)
      .filter(Boolean),
  );
  if (userIds.size === 0 || emails.size === 0) {
    throw new Error(
      "Both APOCKY_OWNER_USER_IDS and APOCKY_OWNER_EMAILS are required",
    );
  }
  return { userIds, emails };
}

export function authorizeOwnerClaims(
  access: VerifiedAccessClaims,
  supabase: VerifiedSupabaseClaims,
  allowlist: OwnerAllowlist,
): AuthorizedOwner | null {
  const accessEmail = normalizeEmail(access.email);
  const supabaseEmail = normalizeEmail(supabase.email);
  if (
    !allowlist.userIds.has(supabase.sub) ||
    !allowlist.emails.has(accessEmail) ||
    !allowlist.emails.has(supabaseEmail) ||
    accessEmail !== supabaseEmail
  ) {
    return null;
  }
  return {
    userId: supabase.sub,
    email: supabaseEmail,
    cloudflareSubject: access.sub,
    cloudflareEmail: accessEmail,
  };
}

export function createConfirmationDigest(
  confirmation: import("./confirmation").ExplicitConfirmationUnsigned,
): `sha256:${string}` {
  return digestCanonical(ExplicitConfirmationUnsignedSchema.parse(confirmation));
}

export {
  ExplicitConfirmationSchema,
  ExplicitConfirmationUnsignedSchema,
};
export type {
  ExplicitConfirmation,
  ExplicitConfirmationUnsigned,
} from "./confirmation";

export function validateExplicitConfirmation(
  input: unknown,
  options: {
    action: string;
    target: string;
    now?: Date;
    maximumAgeMs?: number;
  },
): ExplicitConfirmation {
  const confirmation = ExplicitConfirmationSchema.parse(input);
  if (
    confirmation.action !== options.action ||
    confirmation.target !== options.target
  ) {
    throw new Error("Confirmation does not bind the requested action and target");
  }
  const { digest, ...unsigned } = confirmation;
  if (createConfirmationDigest(unsigned) !== digest) {
    throw new Error("Confirmation digest is invalid");
  }
  const now = options.now ?? new Date();
  const confirmedAt = new Date(confirmation.confirmedAt);
  const maximumAgeMs = options.maximumAgeMs ?? 5 * 60 * 1_000;
  const age = now.getTime() - confirmedAt.getTime();
  if (age < -30_000 || age > maximumAgeMs) {
    throw new Error("Confirmation is stale or issued in the future");
  }
  return confirmation;
}

export function validateMutationRequest(
  request: Request,
  options: {
    allowedOrigins: ReadonlySet<string>;
    action: string;
    target: string;
    confirmation: unknown;
    now?: Date;
  },
): ExplicitConfirmation {
  if (["GET", "HEAD", "OPTIONS"].includes(request.method.toUpperCase())) {
    throw new Error("Consequential actions require a mutation method");
  }
  const origin = request.headers.get("origin");
  if (origin === null || !options.allowedOrigins.has(origin)) {
    throw new Error("Mutation origin is not allowed");
  }
  const fetchSite = request.headers.get("sec-fetch-site");
  if (fetchSite !== null && fetchSite !== "same-origin") {
    throw new Error("Cross-site mutation request denied");
  }
  const confirmation = validateExplicitConfirmation(options.confirmation, {
    action: options.action,
    target: options.target,
    now: options.now,
  });
  if (request.headers.get("x-apocky-confirmation-digest") !== confirmation.digest) {
    throw new Error("Confirmation header does not match the request body");
  }
  return confirmation;
}
