export type InternalPath =
  | "/"
  | "/apocrypha"
  | "/work"
  | "/learn"
  | "/principles"
  | "/privacy"
  | "/terms";

export type NavigationItem = Readonly<{
  label: "Home" | "Apocrypha" | "Work" | "Learn" | "Principles";
  href: InternalPath;
}>;

export type ExternalDestination = Readonly<{
  label: "CSSL" | "CSLv3" | "Chaos Tarot";
  href: `https://${string}`;
  description: string;
}>;

export const siteIdentity = {
  name: "Apocky",
  url: "https://apocky.com",
  description:
    "The public home of Apocky: a selected body of languages, systems, principles, and creative work."
} as const;

export const primaryNavigation = [
  { label: "Home", href: "/" },
  { label: "Apocrypha", href: "/apocrypha" },
  { label: "Work", href: "/work" },
  { label: "Learn", href: "/learn" },
  { label: "Principles", href: "/principles" }
] as const satisfies readonly NavigationItem[];

export const externalDestinations = [
  {
    label: "CSSL",
    href: "https://cssl.dev",
    description:
      "A compiled programming language, runtime, standard library, and substrate."
  },
  {
    label: "CSLv3",
    href: "https://cssl.dev/CSLv3",
    description:
      "A dense specification notation for human–AI collaboration and compiler input."
  },
  {
    label: "Chaos Tarot",
    href: "https://chaos-tarot.com",
    description:
      "A separate creative work in the selected public ecosystem."
  }
] as const satisfies readonly ExternalDestination[];

export const publicRoutes = [
  "/",
  "/apocrypha",
  "/work",
  "/learn",
  "/principles",
  "/privacy",
  "/terms",
  "/llms.txt",
  "/.well-known/apocky.json",
  "/schemas/site-manifest.v1.json",
  "/robots.txt",
  "/sitemap.xml"
] as const;

export function canonical(path: InternalPath): string {
  return new URL(path, siteIdentity.url).toString();
}
