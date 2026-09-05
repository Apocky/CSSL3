export const OPS_READ_SURFACES = [
  "runtime",
  "sessions",
  "authority",
  "consent",
  "security",
  "deployment",
  "retention",
] as const;

export type OpsReadSurface = (typeof OPS_READ_SURFACES)[number];

export const OPS_ROUTES = {
  snapshot: { method: "GET", path: "/api/snapshot" },
  surface: { method: "GET", path: "/api/surfaces/:surface" },
  action: { method: "POST", path: "/api/actions" },
} as const;

export function isOpsReadSurface(value: string): value is OpsReadSurface {
  return (OPS_READ_SURFACES as readonly string[]).includes(value);
}
