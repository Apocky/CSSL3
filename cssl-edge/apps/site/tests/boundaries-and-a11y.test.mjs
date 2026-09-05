import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const sourceRoot = path.join(appRoot, "src");

async function read(relativePath) {
  return readFile(path.join(appRoot, relativePath), "utf8");
}

async function collect(directory, predicate = () => true) {
  const entries = await readdir(directory, { withFileTypes: true });
  const results = [];
  for (const entry of entries) {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      results.push(...(await collect(absolute, predicate)));
    } else if (predicate(absolute)) {
      results.push(absolute);
    }
  }
  return results;
}

test("public application has no auth, database, analytics, mutation, or media client", async () => {
  const packageJson = await read("package.json");
  const files = await collect(sourceRoot, (file) => /\.(?:ts|tsx)$/.test(file));
  const source = (
    await Promise.all(files.map((file) => readFile(file, "utf8")))
  ).join("\n");

  assert.doesNotMatch(packageJson, /supabase|livekit|stripe|analytics/i);
  assert.doesNotMatch(source, /@supabase|service[_-]?role|livekit|stripe/i);
  assert.doesNotMatch(source, /\bfetch\s*\(|XMLHttpRequest|WebSocket|EventSource/);
  assert.doesNotMatch(source, /["']use client["']/);
  assert.doesNotMatch(source, /<form\b|<input\b|<textarea\b|<video\b|<audio\b|<canvas\b/);
});

test("Apocrypha copy is third-person and explicitly bounded", async () => {
  const page = await read("src/app/apocrypha/page.public.tsx");
  const visibleCopy = page
    .replace(/import[\s\S]*?;\n/g, "")
    .replace(/export const metadata[\s\S]*?};\n/, "")
    .replace(/\s+/g, " ");

  assert.doesNotMatch(
    visibleCopy,
    /\b(?:I|me|my|mine|we|us|our|ours)\b/,
    "Apocrypha page must not invent first-person authorship"
  );
  assert.match(visibleCopy, /not a communication surface/);
  assert.match(visibleCopy, /No text on this page is presented as a statement from Apocrypha/);
});

test("visual vocabulary contains no simulated presence or fabricated activity", async () => {
  const files = await collect(sourceRoot, (file) => /\.(?:ts|tsx|css)$/.test(file));
  const source = (
    await Promise.all(files.map((file) => readFile(file, "utf8")))
  ).join("\n");

  assert.doesNotMatch(source, /avatar|mascot|animated[- ]eye|live[- ]indicator/i);
  assert.doesNotMatch(source, /@keyframes|animation\s*:/i);
  assert.doesNotMatch(source, /autoplay|background-music/i);
});

test("relationship map has keyboard links and an explicit text equivalent", async () => {
  const map = await read("src/components/relationship-map.tsx");
  assert.match(map, /aria-labelledby="relationship-title"/);
  assert.match(map, /aria-hidden="true"/);
  assert.match(map, /Relationships, in words/);
  assert.match(map, /<Link href="\/apocrypha">/);
  assert.match(map, /href="https:\/\/cssl\.dev"/);
  assert.match(map, /href="https:\/\/cssl\.dev\/CSLv3"/);
  assert.match(map, /<Link href="\/work">/);
});

test("global accessibility foundations are present", async () => {
  const layout = await read("src/app/layout.public.tsx");
  const header = await read("src/components/site-header.tsx");
  const styles = await read("src/app/globals.css");

  assert.match(layout, /<html lang="en">/);
  assert.match(layout, /<main id="main-content">/);
  assert.match(header, /Skip to content/);
  assert.match(header, /aria-label="Primary navigation"/);
  assert.match(styles, /:focus-visible/);
  assert.match(styles, /prefers-reduced-motion: reduce/);
  assert.match(styles, /prefers-contrast: more/);
  assert.match(styles, /min-height: 2\.75rem/);
});

test("security headers deny public-site sensors and framing", async () => {
  const config = await read("next.config.ts");
  const required = [
    "Content-Security-Policy",
    "Cross-Origin-Opener-Policy",
    "Cross-Origin-Resource-Policy",
    "Permissions-Policy",
    "Referrer-Policy",
    "Strict-Transport-Security",
    "X-Content-Type-Options",
    "X-Frame-Options"
  ];

  for (const header of required) {
    assert.match(config, new RegExp(header));
  }

  assert.match(config, /camera=\(\), microphone=\(\)/);
  assert.match(config, /frame-ancestors 'none'/);
  assert.match(config, /form-action 'none'/);
  assert.match(config, /poweredByHeader: false/);
});

test("framework versions are pinned to the approved stack", async () => {
  const packageJson = JSON.parse(await read("package.json"));
  assert.equal(packageJson.name, "@apocky/site");
  assert.equal(packageJson.dependencies.next, "16.2.11");
  assert.equal(packageJson.dependencies.react, "19.2.8");
  assert.equal(packageJson.dependencies["react-dom"], "19.2.8");
  assert.equal(packageJson.dependencies["@apocky/visual-tokens"], "1.0.0");
  assert.equal(packageJson.devDependencies.typescript, "5.9.3");
});
