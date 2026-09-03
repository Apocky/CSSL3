import fs from 'node:fs';
import path from 'node:path';

import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

import { PUBLIC_SURFACE_NODES } from '../../lib/public-surface-graph';

const VIEWPORTS = [
  { width: 320, height: 568 },
  { width: 390, height: 844 },
  { width: 568, height: 320 },
  { width: 768, height: 1024 },
  { width: 960, height: 720 },
  { width: 1280, height: 720 },
  { width: 1440, height: 900 },
] as const;

function collectBrowserErrors(page: import('@playwright/test').Page) {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });
  return errors;
}

async function expectNoSeriousAccessibilityFindings(page: import('@playwright/test').Page) {
  const result = await new AxeBuilder({ page }).analyze();
  const findings = result.violations.filter((entry) => entry.impact === 'serious' || entry.impact === 'critical');
  expect(findings, JSON.stringify(findings, null, 2)).toEqual([]);
}

test('@visual native hub stays horizontally bounded and keeps primary routes reachable', async ({ page }) => {
  test.setTimeout(120_000);
  const errors = collectBrowserErrors(page);
  const unexpectedSupportRequests: string[] = [];
  page.on('request', (request) => {
    const hostname = new URL(request.url()).hostname;
    if (
      hostname === 'chaos-tarot.com'
      || hostname.endsWith('.chaos-tarot.com')
      || hostname === 'ko-fi.com'
      || hostname.endsWith('.ko-fi.com')
      || hostname === 'patreon.com'
      || hostname.endsWith('.patreon.com')
    ) {
      unexpectedSupportRequests.push(request.url());
    }
  });
  const artifactRoot = path.join(process.cwd(), 'test-results', 'public-route-matrix');
  fs.mkdirSync(artifactRoot, { recursive: true });

  for (const viewport of VIEWPORTS) {
    await page.setViewportSize(viewport);
    await page.goto('/', { waitUntil: 'domcontentloaded' });
    await expect(page).toHaveTitle(/Interconnected worlds, tools, and living ideas/i);
    await expect(page.getByRole('heading', { level: 1, name: /Follow the signal.*Enter the system/i })).toBeVisible();
    await expect(page.getByRole('link', { name: /Explore the Atlas/i }).first()).toBeVisible();
    await expect(page.locator('main').getByRole('link', { name: /CSSL and CSLv3.*Read the language guide/i })).toHaveAttribute('href', '/docs/cssl-language');
    const supportSection = page.locator('main').getByRole('region', { name: /If this deserves to exist, help it compound/i });
    await expect(supportSection).toBeVisible();
    await expect(supportSection).toContainText(/Patreon, Ko-fi, and paid Chaos Tarot access/i);
    const chaosTarot = supportSection.getByRole('link', { name: /Unlock Chaos Tarot.*See plans/i });
    const koFi = supportSection.getByRole('link', { name: /Fuel the next release.*Open Ko-fi/i });
    await expect(chaosTarot).toHaveAttribute('href', 'https://chaos-tarot.com/pricing?source=apocky-home');
    await expect(koFi).toHaveAttribute('href', 'https://ko-fi.com/oneinfinity');
    for (const supportLink of [chaosTarot, koFi]) {
      await expect(supportLink).toBeVisible();
      await expect(supportLink).toHaveAttribute('target', '_blank');
      await expect(supportLink).toHaveAttribute('rel', 'noopener noreferrer');
      const hitArea = await supportLink.boundingBox();
      expect(hitArea?.width ?? 0).toBeGreaterThanOrEqual(44);
      expect(hitArea?.height ?? 0).toBeGreaterThanOrEqual(44);
    }

    if (viewport.width <= 1080) {
      const mobileMenu = page.locator('details.apx-mobile-menu');
      await expect(mobileMenu.locator('summary', { hasText: 'Explore' })).toBeVisible();
      await mobileMenu.locator('summary').click();
      await expect(mobileMenu.getByRole('link', { name: 'Chaos Tarot' })).toBeVisible();
      const openMenuOverflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
      expect(openMenuOverflow, `open mobile menu overflow at ${viewport.width}x${viewport.height}`).toBeLessThanOrEqual(1);
      await mobileMenu.locator('summary').click();
    } else {
      await expect(page.getByRole('navigation', { name: 'Primary navigation' }).getByRole('link', { name: 'Chaos Tarot' })).toBeVisible();
    }

    const horizontalOverflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    expect(horizontalOverflow, `horizontal overflow at ${viewport.width}x${viewport.height}`).toBeLessThanOrEqual(1);

    await page.screenshot({
      path: path.join(artifactRoot, `${viewport.width}x${viewport.height}-hub.png`),
      fullPage: true,
    });
  }

  const supportSection = page.locator('main').getByRole('region', { name: /If this deserves to exist, help it compound/i });
  const supportDetails = supportSection.getByRole('link', { name: /Compare the live paths/i });
  const chaosTarot = supportSection.getByRole('link', { name: /Unlock Chaos Tarot.*See plans/i });
  const koFi = supportSection.getByRole('link', { name: /Fuel the next release.*Open Ko-fi/i });
  await supportDetails.focus();
  await expect(supportDetails).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(chaosTarot).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(koFi).toBeFocused();

  await expectNoSeriousAccessibilityFindings(page);
  expect(errors, errors.join('\n')).toEqual([]);
  expect(unexpectedSupportRequests, 'support providers must not load before a visitor intentionally follows a link').toEqual([]);
});

test('Constellation Atlas stays explicit, keyboard-readable, stateful, and quiet before external handoff', async ({ page }) => {
  const errors = collectBrowserErrors(page);
  const unexpectedRelayRequests: string[] = [];
  page.on('request', (request) => {
    const hostname = new URL(request.url()).hostname;
    if (
      hostname === 'chaos-tarot.com'
      || hostname === 'cssl.dev'
      || hostname === 'ko-fi.com'
      || hostname.endsWith('.ko-fi.com')
      || hostname === 'patreon.com'
      || hostname.endsWith('.patreon.com')
    ) {
      unexpectedRelayRequests.push(request.url());
    }
  });
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/atlas');

  await expect(page).toHaveTitle(/Constellation Atlas/i);
  await expect(page.getByRole('heading', { level: 1, name: /Constellation Atlas/i })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Map', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByTestId('constellation-map')).toBeVisible();
  const minimumNodeSeparation = await page.locator('[data-testid^="atlas-map-node-"]').evaluateAll((elements) => {
    const points = elements.map((element) => {
      const matrix = (element as SVGGElement).transform.baseVal.consolidate()?.matrix;
      return { x: matrix?.e ?? 0, y: matrix?.f ?? 0 };
    });
    let minimum = Number.POSITIVE_INFINITY;
    for (let left = 0; left < points.length; left += 1) {
      for (let right = left + 1; right < points.length; right += 1) {
        minimum = Math.min(minimum, Math.hypot(
          points[left]!.x - points[right]!.x,
          points[left]!.y - points[right]!.y,
        ));
      }
    }
    return minimum;
  });
  expect(minimumNodeSeparation, 'map node hit halos must not overlap').toBeGreaterThanOrEqual(80);
  const viewport = page.viewportSize();
  if ((viewport?.width ?? 0) <= 480) {
    await expect.poll(async () => {
      const [stageBox, atlasBox] = await Promise.all([
        page.getByTestId('constellation-map').boundingBox(),
        page.getByTestId('atlas-map-node-atlas').boundingBox(),
      ]);
      if (!stageBox || !atlasBox) return false;
      const atlasCenter = atlasBox.x + (atlasBox.width / 2);
      return atlasCenter >= stageBox.x && atlasCenter <= stageBox.x + stageBox.width;
    }, { message: 'mobile map must initially center the Atlas root node' }).toBe(true);
    await expect(page.getByText(/Swipe or drag the starfield/i)).toBeVisible();
  }
  const artifactRoot = path.join(process.cwd(), 'test-results', 'public-route-matrix');
  fs.mkdirSync(artifactRoot, { recursive: true });
  await page.screenshot({
    path: path.join(artifactRoot, `${viewport?.width ?? 'unknown'}x${viewport?.height ?? 'unknown'}-atlas-map.png`),
    fullPage: true,
  });

  const akashicNode = page.getByTestId('atlas-node-akashic-records');
  await akashicNode.focus();
  await expect(akashicNode).toBeFocused();
  await page.keyboard.press('Enter');
  const selection = page.getByTestId('atlas-selection');
  await expect(selection.getByRole('heading', { level: 2, name: 'Akashic Records' })).toBeVisible();
  for (const label of ['People', 'Meaning', 'Visibility', 'Time']) {
    await expect(selection.getByText(label, { exact: true })).toBeVisible();
  }

  await page.getByTestId('atlas-node-chaos-tarot').click();
  await expect(page).toHaveURL(/node=chaos-tarot/);
  const chaosLink = selection.getByRole('link', { name: /Enter Chaos Tarot/i });
  await expect(chaosLink).toHaveAttribute('href', 'https://chaos-tarot.com');
  await expect(chaosLink).toHaveAttribute('target', '_blank');
  await expect(chaosLink).toHaveAttribute('rel', 'noopener noreferrer');
  await expect(selection).toContainText(/Nothing is sent there unless you choose the handoff/i);

  await page.getByRole('button', { name: 'Index', exact: true }).click();
  await expect(page).toHaveURL(/view=index/);
  await page.getByRole('searchbox', { name: 'Search public destinations' }).fill('atmosphere');
  await expect(page.getByText('1 destination match', { exact: true })).toBeVisible();
  await expect(page.getByTestId('atlas-index-node-chaos-tarot')).toBeVisible();

  await page.getByRole('button', { name: 'Dictionary', exact: true }).click();
  await expect(page).toHaveURL(/view=dictionary/);
  await page.getByRole('searchbox', { name: 'Search the dictionary' }).fill('consent');
  await expect(page.locator('dt').filter({ hasText: /^Consent$/ })).toBeVisible();

  const horizontalOverflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(horizontalOverflow).toBeLessThanOrEqual(1);
  await page.screenshot({
    path: path.join(artifactRoot, `${viewport?.width ?? 'unknown'}x${viewport?.height ?? 'unknown'}-atlas-dictionary.png`),
    fullPage: true,
  });
  await expectNoSeriousAccessibilityFindings(page);
  expect(errors, errors.join('\n')).toEqual([]);
  expect(unexpectedRelayRequests, 'external destinations must not load before a visitor intentionally follows a relay').toEqual([]);
});

test('Now, Labs, and memory tools expose usable labeled routes without silent external requests', async ({ page }) => {
  const errors = collectBrowserErrors(page);
  const unexpectedExternalRequests: string[] = [];
  page.on('request', (request) => {
    const hostname = new URL(request.url()).hostname;
    if (hostname !== '127.0.0.1') unexpectedExternalRequests.push(request.url());
  });

  for (const route of [
    { href: '/now', title: /What is alive now/i, heading: /What is alive.*What is becoming/i },
    { href: '/labs', title: /Public labs and experiments/i, heading: /Touch the machinery.*Keep the labels attached/i },
    { href: '/memory-tools', title: /Memory banks and tools/i, heading: /Memory banks.*Tools.*One routed mind/i },
  ]) {
    await page.goto(route.href);
    await expect(page).toHaveTitle(route.title);
    await expect(page.getByRole('heading', { level: 1, name: route.heading })).toBeVisible();
    await expect(page.locator('link[rel="canonical"]')).toHaveAttribute('href', `https://www.apocky.com${route.href}`);
    const horizontalOverflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    expect(horizontalOverflow, `${route.href} horizontal overflow`).toBeLessThanOrEqual(1);
    await expectNoSeriousAccessibilityFindings(page);
  }

  expect(errors, errors.join('\n')).toEqual([]);
  expect(unexpectedExternalRequests, 'linked external systems must remain quiet until a visitor chooses a handoff').toEqual([]);
});

test('Clearing resolves as the live React social room without route aliasing', async ({ page }) => {
  const errors = collectBrowserErrors(page);
  await page.route('**/fake-supabase/**', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: '[]',
  }));
  await page.goto('/clearing?room=north-clearing');
  await expect.poll(() => new URL(page.url()).pathname).toBe('/clearing');
  await expect(page.locator('main[aria-label="The Clearing public room"]')).toBeVisible();
  await expect(page.getByRole('heading', { level: 1, name: /North Clearing|The Clearing/ })).toBeVisible();
  await expect(page.getByRole('button', { name: /Sign in to join the room|Checking your account/ })).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Room actions' })).toBeVisible();
  await expect(page.locator('link[rel="canonical"]')).toHaveAttribute('href', 'https://www.apocky.com/clearing');
  await expect(page.getByText('SAMPLE', { exact: true })).toHaveCount(0);
  await expectNoSeriousAccessibilityFindings(page);
  expect(errors, errors.join('\n')).toEqual([]);
});

test('new public routes and retained application routes resolve together', async ({ request }) => {
  const graphRoutes = PUBLIC_SURFACE_NODES
    .filter((node) => !node.external)
    .map((node) => node.href.split('#', 1)[0] ?? node.href);
  for (const route of [...new Set([...graphRoutes, '/account', '/auth/callback'])]) {
    await expect.poll(async () => (await request.get(route)).status(), {
      message: route,
      timeout: 10_000,
    }).toBeLessThan(400);
  }
});

test('legacy Commons hub forwards to the native React homepage', async ({ page }) => {
  await page.goto('/commons');
  await expect.poll(() => new URL(page.url()).pathname).toBe('/');
  await expect(page.getByRole('heading', { level: 1, name: /Follow the signal.*Enter the system/i })).toBeVisible();
});
