import fs from 'node:fs';
import path from 'node:path';

import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

import {
  getAkashicRecords,
  getAkashicRecordSummaries,
} from '../../lib/akashic-records';

const VIEWPORTS = [
  { width: 1440, height: 900 },
  { width: 1024, height: 768 },
  { width: 834, height: 1112 },
  { width: 390, height: 844 },
  { width: 320, height: 568 },
] as const;

function longestUnbrokenTokenLength(value: string): number {
  return value.split(/\s+/u).reduce((longest, token) => Math.max(longest, token.length), 0);
}

const records = getAkashicRecords();
const summaries = getAkashicRecordSummaries();
const longestTitle = summaries.reduce((longest, record) => (
  record.title.length > longest.title.length ? record : longest
));
const structuredRecord = records.reduce((richest, record) => (
  new Set(record.blocks.map((block) => block.kind)).size > new Set(richest.blocks.map((block) => block.kind)).size
    ? record
    : richest
));
const unbrokenTokenRecord = records.reduce((longest, record) => {
  const longestToken = longestUnbrokenTokenLength(longest.body);
  const recordToken = longestUnbrokenTokenLength(record.body);
  return recordToken > longestToken ? record : longest;
});
const linkCardRecord = records.find((record) => (
  record.blocks.some((block) => block.kind === 'linkCard')
));
if (linkCardRecord === undefined) throw new Error('Akashic fixture must include an authored link card');
const embedRecord = records.find((record) => record.blocks.some((block) => block.kind === 'embed'));
if (embedRecord === undefined) throw new Error('Akashic fixture must include an omitted embed');
const linkedTextRecord = records.find((record) => record.blocks.some((block) => (
  (block.kind === 'heading' || block.kind === 'blockquote') && (block.links?.length ?? 0) > 0
)));
if (linkedTextRecord === undefined) throw new Error('Akashic fixture must include a linked heading or quotation');
const filterYear = Array.from(new Set(summaries.map((record) => record.year))).find(
  (year) => summaries.some((record) => record.year !== year),
);

function collectBrowserErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });
  return errors;
}

async function disableOptionalSiteData(page: Page): Promise<void> {
  const off = page.getByRole('radio', { name: /^Off\b/i });
  if (await off.isVisible()) {
    await off.click();
    await page.getByRole('button', { name: 'Save choice: Off' }).click();
  }
}

async function expectNoHorizontalOverflow(page: Page, label: string): Promise<void> {
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow, `${label} has horizontal overflow`).toBeLessThanOrEqual(1);
}

test.beforeEach(async ({ page }) => {
  await page.route('**/api/auth/me', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ user: null }),
  }));
});

test('@visual archive index and longest title remain readable across the viewport matrix', async ({ page }) => {
  test.setTimeout(120_000);
  const browserErrors = collectBrowserErrors(page);
  const artifactRoot = path.join(process.cwd(), 'test-results', 'akashic-records');
  fs.mkdirSync(artifactRoot, { recursive: true });

  for (const viewport of VIEWPORTS) {
    await page.setViewportSize(viewport);
    await page.goto('/akashic-records', { waitUntil: 'domcontentloaded' });
    await disableOptionalSiteData(page);
    await expect(page.getByRole('heading', { level: 1, name: 'Akashic Records' })).toBeVisible();
    await expect(page.locator('main').getByRole('status')).toContainText('204 works');
    await expectNoHorizontalOverflow(page, `archive index at ${viewport.width}x${viewport.height}`);
    await page.screenshot({
      path: path.join(artifactRoot, `${viewport.width}x${viewport.height}-index.png`),
      fullPage: false,
    });

    await page.goto(`/akashic-records/${longestTitle.slug}`, { waitUntil: 'domcontentloaded' });
    await expect(page.getByRole('heading', { level: 1, name: longestTitle.title })).toBeVisible();
    await expectNoHorizontalOverflow(page, `longest title at ${viewport.width}x${viewport.height}`);
    await page.screenshot({
      path: path.join(artifactRoot, `${viewport.width}x${viewport.height}-longest-title.png`),
      fullPage: false,
    });
  }

  await page.setViewportSize({ width: 320, height: 568 });
  await page.goto(`/akashic-records/${unbrokenTokenRecord.slug}`, { waitUntil: 'domcontentloaded' });
  await expect(page.getByRole('heading', { level: 1, name: unbrokenTokenRecord.title })).toBeVisible();
  await expectNoHorizontalOverflow(page, 'longest unbroken authored token at 320x568');
  expect(browserErrors, browserErrors.join('\n')).toEqual([]);
});

test('archive controls filter locally and announce their result', async ({ page }) => {
  const browserErrors = collectBrowserErrors(page);
  await page.goto('/akashic-records');
  await disableOptionalSiteData(page);

  const search = page.getByRole('searchbox', { name: 'Search' });
  const resultStatus = page.locator('main').getByRole('status');
  await expect(page.getByRole('combobox', { name: 'Source' })).toHaveCount(0);
  await expect(page.getByRole('combobox', { name: 'Topic' })).toHaveCount(0);
  await expect(page.getByRole('combobox', { name: 'Type' })).toHaveCount(0);
  await search.fill(longestTitle.title);
  await expect(resultStatus).toHaveText('1 of 204 works');
  await expect(page.getByRole('link', { name: longestTitle.title }).first()).toBeVisible();

  await page.getByRole('button', { name: 'Clear' }).click();
  await expect(search).toHaveValue('');
  await expect(resultStatus).toContainText('204 works');

  if (filterYear !== undefined) {
    await page.getByRole('combobox', { name: 'Year' }).selectOption(String(filterYear));
    await expect(resultStatus).toContainText('of 204 works');
    await page.getByRole('button', { name: 'Clear' }).click();
  }

  await search.fill('no-record-can-match-this-exact-phrase-7f6e37');
  await expect(page.getByRole('heading', { name: 'No works match these filters.' })).toBeVisible();
  await page.getByRole('button', { name: 'Show all works' }).click();
  await expect(resultStatus).toContainText('204 works');

  const accessibility = await new AxeBuilder({ page }).include('main').analyze();
  const serious = accessibility.violations.filter((entry) => entry.impact === 'serious' || entry.impact === 'critical');
  expect(serious, JSON.stringify(serious, null, 2)).toEqual([]);
  expect(browserErrors, browserErrors.join('\n')).toEqual([]);
});

test('reader renders the safe semantic block projection without raw HTML', async ({ page }) => {
  const browserErrors = collectBrowserErrors(page);
  await page.goto(`/akashic-records/${structuredRecord.slug}`);
  await disableOptionalSiteData(page);
  const body = page.locator('article [class*="readerBody"]').first();
  await expect(body).toBeVisible();

  const expectedKinds = new Set(structuredRecord.blocks.map((block) => block.kind));
  if (expectedKinds.has('heading')) await expect(body.locator('h2, h3').first()).toBeVisible();
  if (expectedKinds.has('blockquote')) await expect(body.locator('blockquote').first()).toBeVisible();
  if (expectedKinds.has('list')) await expect(body.locator('ol, ul').first()).toBeVisible();
  if (expectedKinds.has('pre')) await expect(body.locator('pre code').first()).toBeVisible();
  if (expectedKinds.has('figure') || expectedKinds.has('embed')) {
    await expect(body.getByText(/omitted from archive copy/i).first()).toBeVisible();
  }

  await expectNoHorizontalOverflow(page, 'structured record reader');

  const linkCardBlock = linkCardRecord.blocks.find((block) => block.kind === 'linkCard');
  if (linkCardBlock?.kind !== 'linkCard') throw new Error('Selected fixture lost its link card');
  await page.goto(`/akashic-records/${linkCardRecord.slug}`);
  const linkCard = page.locator('article [class*="linkCard"] a').first();
  await expect(linkCard).toHaveAttribute('href', linkCardBlock.href);
  await expect(linkCard).toHaveAttribute('target', '_blank');
  await expectNoHorizontalOverflow(page, 'link-card record reader');

  const embedBlock = embedRecord.blocks.find((block) => block.kind === 'embed');
  if (embedBlock?.kind !== 'embed') throw new Error('Selected fixture lost its embed');
  await page.goto(`/akashic-records/${embedRecord.slug}`);
  const embedNotice = page.getByLabel('Omitted embedded media').first();
  await expect(embedNotice).toBeVisible();
  if (embedBlock.href !== undefined) {
    await expect(embedNotice.getByRole('link')).toHaveAttribute('href', embedBlock.href);
  }

  const linkedTextBlock = linkedTextRecord.blocks.find((block) => (
    (block.kind === 'heading' || block.kind === 'blockquote') && (block.links?.length ?? 0) > 0
  ));
  if (linkedTextBlock?.kind !== 'heading' && linkedTextBlock?.kind !== 'blockquote') {
    throw new Error('Selected fixture lost its linked heading or quotation');
  }
  const expectedLinkedHref = linkedTextBlock.links?.[0]?.href;
  if (expectedLinkedHref === undefined) throw new Error('Selected linked text fixture lost its destination');
  await page.goto(`/akashic-records/${linkedTextRecord.slug}`);
  const linkedTextAnchor = page.locator('article [class*="readerBody"] h2 a, article [class*="readerBody"] h3 a, article [class*="readerBody"] blockquote a').first();
  await expect(linkedTextAnchor).toHaveAttribute('href', expectedLinkedHref);

  const accessibility = await new AxeBuilder({ page }).include('main').analyze();
  const serious = accessibility.violations.filter((entry) => entry.impact === 'serious' || entry.impact === 'critical');
  expect(serious, JSON.stringify(serious, null, 2)).toEqual([]);
  expect(browserErrors, browserErrors.join('\n')).toEqual([]);
});
