// Server-only derivation for the durable Apocrypha member principal.

import { createHash } from 'node:crypto';

export const RUNTIME_SESSION_PRINCIPAL_RE = /^principal:apocky-member:[0-9a-f]{64}$/;

declare const RUNTIME_SESSION_PRINCIPAL: unique symbol;
export type RuntimeSessionPrincipal = string & {
  readonly [RUNTIME_SESSION_PRINCIPAL]: 'server-derived-member-principal';
};

export function isRuntimeSessionPrincipal(value: unknown): value is RuntimeSessionPrincipal {
  return typeof value === 'string' && RUNTIME_SESSION_PRINCIPAL_RE.test(value);
}

/**
 * Principal for a signed-out visitor.
 *
 * Domain-separated from the member derivation by its own constant, so the two hash spaces are
 * disjoint: a guest id can never be crafted to collide with a real member's principal, and
 * therefore can never address a member's conversation scope. The branded prefix is kept identical
 * so every downstream validator treats it as an ordinary principal -- a guest is a full principal
 * with a narrower budget, not a special case threaded through the request path.
 */
export function anonymousGuestPrincipalRef(guestId: string): RuntimeSessionPrincipal {
  if (
    typeof guestId !== 'string'
    || guestId !== guestId.trim()
    || guestId.length < 8
    || guestId.length > 128
  ) {
    throw new TypeError('guest_principal_invalid');
  }
  const digest = createHash('sha256')
    .update('APOCRYPHA-V2-ANONYMOUS-GUEST-PRINCIPAL-v1\0', 'utf8')
    .update(guestId, 'utf8')
    .digest('hex');
  return `principal:apocky-member:${digest}` as RuntimeSessionPrincipal;
}

export function publicMemberPrincipalRef(userId: string): RuntimeSessionPrincipal {
  if (
    typeof userId !== 'string'
    || userId !== userId.trim()
    || userId.length < 1
    || userId.length > 512
  ) {
    throw new TypeError('session_principal_invalid');
  }
  const digest = createHash('sha256')
    .update('APOCRYPHA-V2-PUBLIC-MEMBER-PRINCIPAL-v1\0', 'utf8')
    .update(userId, 'utf8')
    .digest('hex');
  return `principal:apocky-member:${digest}` as RuntimeSessionPrincipal;
}
