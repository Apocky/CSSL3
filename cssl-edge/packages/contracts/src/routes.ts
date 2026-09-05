export const PUBLIC_NAVIGATION = [
  { label: "Home", href: "/" },
  { label: "Apocrypha", href: "/apocrypha" },
  { label: "Work", href: "/work" },
  { label: "Learn", href: "/learn" },
  { label: "Principles", href: "/principles" },
] as const;

export const PUBLIC_SUPPORT_ROUTES = [
  "/privacy",
  "/terms",
  "/llms.txt",
  "/.well-known/apocky.json",
  "/schemas/site-manifest.v1.json",
  "/robots.txt",
  "/sitemap.xml",
] as const;

export const PUBLIC_PAGE_ROUTES = [
  "/",
  "/apocrypha",
  "/work",
  "/learn",
  "/principles",
  "/privacy",
  "/terms",
] as const;

export const PUBLIC_ROUTE_ALLOWLIST = [
  ...PUBLIC_PAGE_ROUTES,
  ...PUBLIC_SUPPORT_ROUTES,
] as const;

export const EXTERNAL_DESTINATIONS = [
  { label: "CSSL", href: "https://cssl.dev" },
  { label: "CSLv3", href: "https://cssl.dev/CSLv3" },
  { label: "Chaos Tarot", href: "https://chaos-tarot.com" },
] as const;

export const EXCLUDED_PUBLIC_CAPABILITIES = [
  "chat",
  "registration",
  "account",
  "waitlist",
  "access-request",
  "support-funnel",
  "private-doorway",
  "commerce",
  "membership",
  "community-room",
  "dashboard",
] as const;

export type PublicRoute = (typeof PUBLIC_ROUTE_ALLOWLIST)[number];
