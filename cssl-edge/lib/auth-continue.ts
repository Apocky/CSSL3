// What happens immediately after a sign-in succeeds.
//
// Setting up an authenticator used to be something you did AFTER signing in, on an account page you
// had to know existed. That is the wrong moment and the wrong place: the person who just signed in
// by email is exactly the person with no authenticator, they are already thinking about sign-in,
// and they have just proved they are the account — which is the entire gate on enrolment. Asking
// later means asking someone who has moved on.
//
// So every sign-in path ends here instead of calling location.replace itself, and an account with
// no authenticator is offered one as the last step of signing in.
//
// Two rules this must never break:
//   1. A failed or slow status check NEVER blocks the sign-in. The session is already real; the
//      offer is a courtesy. On any doubt, go where the reader asked to go.
//   2. An account that already HAS an authenticator is never asked again — being nagged at every
//      sign-in is how a security feature becomes something people route around.

const STATUS_DEADLINE_MS = 4_000;

export interface EnrolmentStatus {
  readonly enrolled: boolean;
  readonly known: boolean;
}

/** Ask whether this session's account already has a confirmed authenticator. */
export async function fetchEnrolmentStatus(
  fetchImpl: typeof fetch = fetch,
  deadlineMs = STATUS_DEADLINE_MS,
): Promise<EnrolmentStatus> {
  const controller = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  // The deadline RACES the request rather than only aborting its signal. Aborting is the polite
  // path and the one real fetch honours, but a transport that ignores the signal — a wedged proxy,
  // a stubbed fetch, a service worker holding the response — would otherwise hang this await
  // forever, and with it a sign-in that has ALREADY SUCCEEDED. The whole point of this check is
  // that it is optional; it must be structurally incapable of blocking.
  const expired = new Promise<null>((resolve) => {
    timer = setTimeout(() => { controller.abort(); resolve(null); }, deadlineMs);
  });
  try {
    const response = await Promise.race([
      fetchImpl('/api/auth/totp/setup', {
        method: 'GET',
        cache: 'no-store',
        credentials: 'same-origin',
        headers: { accept: 'application/json' },
        signal: controller.signal,
      }),
      expired,
    ]);
    if (!response) return { enrolled: false, known: false };
    const payload = await Promise.race([
      response.json().catch(() => null) as Promise<{ ok?: boolean; enrolled?: boolean } | null>,
      expired,
    ]);
    if (!payload || !response.ok || payload.ok !== true || typeof payload.enrolled !== 'boolean') {
      return { enrolled: false, known: false };
    }
    return { enrolled: payload.enrolled, known: true };
  } catch {
    return { enrolled: false, known: false };
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

/**
 * The URL that offers enrolment as the closing step of signing in.
 *
 * It is a real, navigable URL rather than a piece of in-memory state so that a refresh, a back
 * button, or a provider round-trip all land in the same place instead of silently skipping it.
 */
export function authenticatorSetupHref(destination: string): string {
  return `/login?next=${encodeURIComponent(destination)}&setup=authenticator`;
}

/**
 * Finish a successful sign-in: offer an authenticator if the account has none, otherwise go.
 *
 * `known: false` means the check itself failed, and that must read as "go", not as "enrol" — a
 * reader whose status could not be read must not be pushed at a screen offering to replace an
 * authenticator they may well already have.
 */
export async function continueAfterSignIn(
  destination: string,
  options: {
    readonly fetchImpl?: typeof fetch;
    readonly navigate?: (url: string) => void;
    readonly alreadyOffered?: boolean;
  } = {},
): Promise<string> {
  const navigate = options.navigate ?? ((url: string) => { location.replace(url); });
  // Offered once per sign-in. Without this, declining the offer and being returned through the
  // same completion path would present it again, which is a loop, not a prompt.
  if (options.alreadyOffered) {
    navigate(destination);
    return destination;
  }
  const status = await fetchEnrolmentStatus(options.fetchImpl ?? fetch);
  const target = status.known && !status.enrolled ? authenticatorSetupHref(destination) : destination;
  navigate(target);
  return target;
}
