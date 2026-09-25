// Direct mode: the same site, run on the owner's PC and reached directly -- from the PC itself, or
// from the owner's other devices over Tailscale -- instead of through apocky.com.
//
// Owner decisions 2026-09-25: "a full desktop app with remote direct connection"; direct when the
// PC is reachable, apocky.com as the fallback; Windows only; tray + notifications, Work lane, voice,
// service control panel.
//
// Trust model (every clause is load-bearing):
//   * The server listens on 127.0.0.1 only. Remote devices reach it through `tailscale serve`, which
//     admits only members of the owner's tailnet and adds their identity as Tailscale-User-Login.
//   * A request is the OWNER when direct mode is on, it arrived on loopback, its Host is one this
//     server answers to (blocks DNS rebinding: a hostile page whose name resolves to 127.0.0.1 sends
//     its own Host), and -- if it came through Tailscale -- its tailnet login is allowed.
//   * State-changing routes additionally require same-origin (blocks a page in another tab from
//     POSTing to 127.0.0.1). The site's room routes already do; the direct routes do too.
// Off Vercel only: APOCRYPHA_DIRECT_MODE is never set in the Vercel project, so every direct route
// answers 404 in production.

import type { NextApiRequest } from 'next';

export function directMode(): boolean {
  return process.env.APOCRYPHA_DIRECT_MODE === '1';
}

function list(value: string | undefined): string[] {
  return (value ?? '').split(',').map((item) => item.trim().toLowerCase()).filter(Boolean);
}

function header(req: NextApiRequest, name: string): string {
  const value = req.headers[name];
  return (Array.isArray(value) ? value[0] ?? '' : value ?? '').trim();
}

const LOOPBACK = new Set(['127.0.0.1', '::1', '::ffff:127.0.0.1']);

/** Host names this server answers to: its loopback address plus any tailnet names configured. */
export function allowedHosts(): Set<string> {
  const port = process.env.PORT ?? process.env.APOCRYPHA_DIRECT_PORT ?? '19141';
  return new Set([`127.0.0.1:${port}`, `localhost:${port}`, ...list(process.env.APOCRYPHA_DIRECT_HOSTS)]);
}

export interface DirectViewer {
  readonly owner: boolean;
  /** Set when the request came through Tailscale: the tailnet login of the device's user. */
  readonly tailnetLogin: string | null;
}

/** Whether this request is the owner, reaching this PC directly. Always false off direct mode. */
export function directViewer(req: NextApiRequest): DirectViewer {
  if (!directMode()) return { owner: false, tailnetLogin: null };
  const remote = req.socket?.remoteAddress ?? '';
  if (!LOOPBACK.has(remote)) return { owner: false, tailnetLogin: null };
  // Tailscale Funnel traffic is the public internet: never the owner.
  if (header(req, 'tailscale-funnel-request')) return { owner: false, tailnetLogin: null };
  if (!allowedHosts().has(header(req, 'host').toLowerCase())) return { owner: false, tailnetLogin: null };
  const login = header(req, 'tailscale-user-login').toLowerCase();
  if (login) {
    const allowed = list(process.env.APOCRYPHA_DIRECT_TAILNET_LOGINS);
    return { owner: allowed.includes(login), tailnetLogin: login };
  }
  return { owner: true, tailnetLogin: null };
}

export function directOwnerUserId(): string | null {
  const id = process.env.APOCRYPHA_DIRECT_OWNER_USER_ID?.trim() ?? '';
  return /^[0-9a-f-]{36}$/i.test(id) ? id.toLowerCase() : null;
}

/** Same-origin for state-changing direct routes: the Origin must be one of this server's hosts. */
export function directSameOrigin(req: NextApiRequest): boolean {
  const origin = header(req, 'origin');
  if (!origin) return false;
  try {
    return allowedHosts().has(new URL(origin).host.toLowerCase());
  } catch {
    return false;
  }
}
