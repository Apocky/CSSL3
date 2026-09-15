// Is this page running inside the Apocrypha app, or in a browser?
//
// The native shell appends "ApocryphaShell/<version>" to its user agent. That is the only reliable
// signal: the shell renders the site itself, so everything else about the page is identical to the
// browser.
//
// Worth getting right because the site was inviting people to install an app they were already
// using, which reads as the page not knowing where it is.

const MARKER = /ApocryphaShell\/([0-9A-Za-z.+-]+)/;

export interface ShellContext {
  readonly inApp: boolean;
  readonly version: string | null;
}

export function shellContextFrom(userAgent: string | undefined | null): ShellContext {
  const match = MARKER.exec(userAgent ?? '');
  return { inApp: match !== null, version: match?.[1] ?? null };
}

/**
 * Client-side check.
 *
 * Returns false during server rendering and the first paint, because navigator does not exist
 * there. Callers should treat it as "hide the install prompt once we know", never as a gate on
 * anything that matters — a user agent is a claim by the client, not a fact.
 */
export function inApocryphaApp(): boolean {
  if (typeof navigator === 'undefined') return false;
  return shellContextFrom(navigator.userAgent).inApp;
}
