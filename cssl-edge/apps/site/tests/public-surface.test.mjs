import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const sourceRoot = path.join(appRoot, "src");

const exactRoutes = [
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
];

async function read(relativePath) {
  return readFile(path.join(appRoot, relativePath), "utf8");
}

async function sourceFiles(directory = sourceRoot) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await sourceFiles(absolute)));
    } else if (/\.(?:ts|tsx|css)$/.test(entry.name)) {
      files.push(absolute);
    }
  }
  return files;
}

test("manifest exposes the exact approved public route allowlist", async () => {
  const manifest = JSON.parse(await read("public/.well-known/apocky.json"));
  assert.deepEqual(manifest.publicRoutes, exactRoutes);
  assert.equal(manifest.canonicalOrigin, "https://apocky.com");
  assert.equal(manifest.contentModel, "version-controlled-static");
});

test("the App Router page set matches the approved human-readable pages", async () => {
  const expectedPages = [
    "src/app/page.public.tsx",
    "src/app/apocrypha/page.public.tsx",
    "src/app/work/page.public.tsx",
    "src/app/learn/page.public.tsx",
    "src/app/principles/page.public.tsx",
    "src/app/privacy/page.public.tsx",
    "src/app/terms/page.public.tsx"
  ].sort();

  const files = await sourceFiles(path.join(sourceRoot, "app"));
  const actualPages = files
    .filter((file) => path.basename(file) === "page.public.tsx")
    .map((file) => path.relative(appRoot, file).replaceAll("\\", "/"))
    .sort();

  assert.deepEqual(actualPages, expectedPages);
});

test("primary navigation has the exact approved labels and destinations", async () => {
  const content = await read("src/content/site.ts");
  const navigationBlock = content.match(
    /export const primaryNavigation = \[([\s\S]*?)\] as const/
  )?.[1];
  assert.ok(navigationBlock, "primary navigation declaration is present");

  const entries = [
    ["Home", "/"],
    ["Apocrypha", "/apocrypha"],
    ["Work", "/work"],
    ["Learn", "/learn"],
    ["Principles", "/principles"]
  ];

  for (const [label, href] of entries) {
    assert.match(
      navigationBlock,
      new RegExp(`label: "${label}", href: "${href.replace("/", "\\/")}"`)
    );
  }
  assert.equal((navigationBlock.match(/label:/g) ?? []).length, entries.length);
});

test("public route conventions are isolated from the legacy parent project", async () => {
  const config = await read("next.config.ts");
  assert.match(config, /pageExtensions: \["public\.tsx", "public\.ts"\]/);
});

test("all literal internal links point to approved routes", async () => {
  const files = (await sourceFiles()).filter((file) => file.endsWith(".tsx"));
  const linkedRoutes = new Set();

  for (const file of files) {
    const content = await readFile(file, "utf8");
    for (const match of content.matchAll(/href="(\/[^"]*)"/g)) {
      linkedRoutes.add(match[1]);
    }
  }

  for (const route of linkedRoutes) {
    assert.ok(
      exactRoutes.includes(route),
      `unexpected internal link found: ${route}`
    );
  }
});

test("curated external destinations are exact and labeled", async () => {
  const manifest = JSON.parse(await read("public/.well-known/apocky.json"));
  assert.deepEqual(manifest.externalDestinations, [
    { label: "CSSL", url: "https://cssl.dev" },
    { label: "CSLv3", url: "https://cssl.dev/CSLv3" },
    { label: "Chaos Tarot", url: "https://chaos-tarot.com" }
  ]);

  const externalLink = await read("src/components/external-link.tsx");
  assert.match(externalLink, /external site/);
  assert.match(externalLink, /rel="external"/);
});

test("machine-readable discovery contains public material only", async () => {
  const discoveryFiles = [
    "public/.well-known/apocky.json",
    "public/llms.txt",
    "public/robots.txt",
    "public/sitemap.xml"
  ];

  for (const file of discoveryFiles) {
    const content = (await read(file)).toLowerCase();
    assert.doesNotMatch(content, /encounter\.apocky\.com/);
    assert.doesNotMatch(content, /ops\.apocky\.com/);
  }
});

test("sitemap contains only canonical human-readable pages", async () => {
  const sitemap = await read("public/sitemap.xml");
  const urls = [...sitemap.matchAll(/<loc>(.*?)<\/loc>/g)].map(
    (match) => match[1]
  );
  assert.deepEqual(
    urls,
    exactRoutes
      .slice(0, 7)
      .map((route) => new URL(route, "https://apocky.com").toString())
  );
});
