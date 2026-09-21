/**
 * Cloudflare Access verification, done AT THE ORIGIN.
 *
 * WHY THIS EXISTS AT ALL, when cloudflared already enforces Access at the edge:
 *
 * The edge check lives in a YAML file. One typo in `access.required`, one hostname added without
 * an Access policy, one `cloudflared` started with a different config, and the tunnel becomes an
 * open pipe from the public internet to a process that can read and write this filesystem. The
 * edge is a door; this is the lock on the room. The same pattern is already used for
 * apocrypha.apocky.com, whose config comments say the backend double-checks the JWT per I-16.
 *
 * WHAT IT CHECKS. Cloudflare puts a signed JWT in `Cf-Access-Jwt-Assertion` on every request that
 * passed an Access policy. This verifies:
 *
 *   1. the signature, against the team's published RSA keys      <- the whole basis of trust
 *   2. `aud` contains this application's audience tag            <- a JWT for ANOTHER app of the
 *                                                                   same team must not work here
 *   3. `iss` is exactly this team's domain
 *   4. `exp` and `nbf`, with a small skew allowance
 *   5. `email` is on the allow list
 *
 * Every one of those is load-bearing. Skipping (2) is the classic Access mistake: any application
 * in the same Cloudflare team could mint a token that this origin would accept.
 *
 * ZERO DEPENDENCIES. Node's crypto verifies RS256 against a JWK directly.
 */
import { createPublicKey, verify as cryptoVerify, timingSafeEqual } from 'node:crypto';

export interface AccessConfig {
  /** e.g. "winter-snowflake-e14a" -> https://winter-snowflake-e14a.cloudflareaccess.com */
  teamName: string;
  /** The Access application's AUD tag. Wrong tag = a token for another app. */
  audTag: string;
  /** Lower-cased emails allowed through. Empty means "nobody", never "everybody". */
  allowedEmails: ReadonlySet<string>;
  /** Seconds of clock skew tolerated on exp/nbf. */
  skewSeconds?: number;
  fetchImpl?: typeof fetch;
  /** Cache lifetime for the team's signing keys. */
  keyTtlMs?: number;
}

export interface AccessIdentity {
  email: string;
  subject: string;
  expiresAt: number;
}

export type AccessResult =
  | { ok: true; identity: AccessIdentity }
  | { ok: false; reason: string };

interface Jwk {
  kid?: string;
  kty?: string;
  alg?: string;
  n?: string;
  e?: string;
}

function b64urlToBuffer(part: string): Buffer {
  return Buffer.from(part.replace(/-/g, '+').replace(/_/g, '/'), 'base64');
}

function decodeJson(part: string): Record<string, unknown> | null {
  try {
    return JSON.parse(b64urlToBuffer(part).toString('utf8')) as Record<string, unknown>;
  } catch {
    return null;
  }
}

/** Constant-time string compare, so the aud tag cannot be probed by timing. */
function sameSecret(a: string, b: string): boolean {
  const ab = Buffer.from(a);
  const bb = Buffer.from(b);
  if (ab.length !== bb.length) return false;
  return timingSafeEqual(ab, bb);
}

export class AccessVerifier {
  private keys: Map<string, Jwk> = new Map();
  private fetchedAt = 0;
  private readonly fetchImpl: typeof fetch;

  constructor(private readonly config: AccessConfig) {
    this.fetchImpl = config.fetchImpl ?? fetch;
  }

  get issuer(): string {
    return 'https://' + this.config.teamName + '.cloudflareaccess.com';
  }

  get certsUrl(): string {
    return this.issuer + '/cdn-cgi/access/certs';
  }

  private async keyFor(kid: string): Promise<Jwk | null> {
    const ttl = this.config.keyTtlMs ?? 60 * 60 * 1000;
    const stale = Date.now() - this.fetchedAt > ttl;
    if (!stale && this.keys.has(kid)) return this.keys.get(kid) ?? null;

    try {
      const res = await this.fetchImpl(this.certsUrl, { signal: AbortSignal.timeout(5_000) });
      if (!res.ok) return this.keys.get(kid) ?? null;
      const body = (await res.json()) as { keys?: Jwk[] };
      const next = new Map<string, Jwk>();
      for (const key of body.keys ?? []) {
        if (key.kid) next.set(key.kid, key);
      }
      // Only replace the cache if the fetch produced something. An empty response during a
      // Cloudflare blip must not lock out a valid token.
      if (next.size > 0) {
        this.keys = next;
        this.fetchedAt = Date.now();
      }
    } catch {
      /* keep whatever is cached; a network failure is not a reason to accept an unverified token */
    }
    return this.keys.get(kid) ?? null;
  }

  /**
   * Verify one assertion. Returns a reason on failure, never throws, and the reason is safe to
   * log: it names the CHECK that failed, never any part of the token.
   */
  async verify(assertion: string | undefined | null): Promise<AccessResult> {
    if (!assertion) return { ok: false, reason: 'missing_assertion' };

    const parts = assertion.split('.');
    if (parts.length !== 3) return { ok: false, reason: 'malformed_jwt' };
    // Destructured once so the rest of this function works on values the compiler knows exist,
    // rather than re-indexing an array it has to keep proving is long enough.
    const [rawHeader, rawPayload, rawSignature] = parts as [string, string, string];

    const header = decodeJson(rawHeader);
    const payload = decodeJson(rawPayload);
    if (!header || !payload) return { ok: false, reason: 'undecodable_jwt' };

    // alg is taken from OUR expectation, not from the token. Trusting header.alg is how "alg:none"
    // and RS256->HS256 confusion attacks work.
    if (header.alg !== 'RS256') return { ok: false, reason: 'unexpected_alg' };
    const kid = typeof header.kid === 'string' ? header.kid : '';
    if (!kid) return { ok: false, reason: 'missing_kid' };

    const jwk = await this.keyFor(kid);
    if (!jwk || jwk.kty !== 'RSA' || !jwk.n || !jwk.e) return { ok: false, reason: 'unknown_kid' };

    let verified = false;
    try {
      const key = createPublicKey({ key: jwk as never, format: 'jwk' });
      verified = cryptoVerify(
        'RSA-SHA256',
        Buffer.from(rawHeader + '.' + rawPayload, 'utf8'),
        key,
        b64urlToBuffer(rawSignature),
      );
    } catch {
      return { ok: false, reason: 'signature_error' };
    }
    if (!verified) return { ok: false, reason: 'bad_signature' };

    if (payload.iss !== this.issuer) return { ok: false, reason: 'wrong_issuer' };

    // aud may be a string or an array. A token minted for a DIFFERENT Access application in the
    // same team is otherwise perfectly valid, which is exactly why this check is not optional.
    const audList = Array.isArray(payload.aud)
      ? (payload.aud as unknown[]).map(String)
      : typeof payload.aud === 'string' ? [payload.aud] : [];
    if (!audList.some((a) => sameSecret(a, this.config.audTag))) {
      return { ok: false, reason: 'wrong_audience' };
    }

    const skew = this.config.skewSeconds ?? 60;
    const now = Math.floor(Date.now() / 1000);
    const exp = Number(payload.exp);
    if (!Number.isFinite(exp) || exp + skew < now) return { ok: false, reason: 'expired' };
    const nbf = Number(payload.nbf);
    if (Number.isFinite(nbf) && nbf - skew > now) return { ok: false, reason: 'not_yet_valid' };

    const email = String(payload.email ?? '').trim().toLowerCase();
    if (!email) return { ok: false, reason: 'no_email' };
    // An empty allow list denies everyone. "No list configured" must never mean "allow all".
    if (!this.config.allowedEmails.has(email)) return { ok: false, reason: 'email_not_allowed' };

    return {
      ok: true,
      identity: { email, subject: String(payload.sub ?? ''), expiresAt: exp },
    };
  }
}

/** Parse the allow list. Comma separated, lower-cased, empties dropped. */
export function parseAllowedEmails(raw: string | undefined): Set<string> {
  return new Set(
    (raw ?? '')
      .split(',')
      .map((s) => s.trim().toLowerCase())
      .filter(Boolean),
  );
}

export interface AccessGate {
  required: boolean;
  verifier: AccessVerifier | null;
  /** Why the gate is off, for the startup banner. */
  disabledReason?: string;
}

/**
 * Build the gate from the environment.
 *
 * It is REQUIRED whenever the server is reachable from anywhere but loopback, or whenever
 * APOCRYPHA_WORK_REQUIRE_ACCESS=1. Misconfiguration fails CLOSED: if Access is required but the
 * team, tag or allow list is missing, the caller must refuse to serve rather than serve openly.
 */
export function buildAccessGate(env: NodeJS.ProcessEnv, host: string): AccessGate {
  const loopback = host === '127.0.0.1' || host === '::1' || host === 'localhost';
  const explicit = env.APOCRYPHA_WORK_REQUIRE_ACCESS === '1';
  const required = explicit || !loopback;
  if (!required) return { required: false, verifier: null, disabledReason: 'loopback only' };

  const teamName = (env.APOCRYPHA_WORK_ACCESS_TEAM ?? '').trim();
  const audTag = (env.APOCRYPHA_WORK_ACCESS_AUD ?? '').trim();
  const allowedEmails = parseAllowedEmails(env.APOCRYPHA_WORK_ACCESS_EMAILS);
  if (!teamName || !audTag || allowedEmails.size === 0) {
    // Deliberately returns required:true with a null verifier. The server must treat this as
    // fatal. Returning required:false here would turn a missing setting into an open door.
    return {
      required: true,
      verifier: null,
      disabledReason:
        'APOCRYPHA_WORK_ACCESS_TEAM, _AUD and _EMAILS are all required when Access is enforced',
    };
  }
  return {
    required: true,
    verifier: new AccessVerifier({ teamName, audTag, allowedEmails }),
  };
}
