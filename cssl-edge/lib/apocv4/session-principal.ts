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
