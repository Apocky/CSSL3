export const ENCOUNTER_ROUTES = {
  current: { method: "GET", path: "/api/encounters/current" },
  create: { method: "POST", path: "/api/encounters" },
  end: {
    method: "POST",
    path: "/api/encounters/:sessionId/end",
  },
  joinToken: {
    method: "POST",
    path: "/api/encounters/:sessionId/join-token",
  },
  grantConsent: {
    method: "POST",
    path: "/api/encounters/:sessionId/consent/grant",
  },
  revokeConsent: {
    method: "POST",
    path: "/api/encounters/:sessionId/consent/revoke",
  },
  readiness: {
    method: "GET",
    path: "/api/encounters/:sessionId/readiness",
  },
  understandingSubmit: {
    method: "POST",
    path: "/api/encounters/:sessionId/understanding",
  },
  understandingAcknowledge: {
    method: "POST",
    path: "/api/encounters/:sessionId/understanding/acknowledge",
  },
  understandingCorrect: {
    method: "POST",
    path: "/api/encounters/:sessionId/understanding/correct",
  },
  historyRead: {
    method: "GET",
    path: "/api/encounters/:sessionId/history",
  },
  historyDelete: {
    method: "DELETE",
    path: "/api/encounters/:sessionId/history",
  },
} as const;

export function encounterPath(
  template: string,
  sessionId: string,
): string {
  return template.replace(":sessionId", encodeURIComponent(sessionId));
}
