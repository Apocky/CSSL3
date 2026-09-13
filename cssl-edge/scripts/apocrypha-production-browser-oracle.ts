import { randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, statSync, writeFileSync } from 'node:fs';
import { isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { chromium, type Page, type Request } from '@playwright/test';

export const PRODUCTION_ORIGIN = 'https://www.apocky.com';
export const ORACLE_ENDPOINT = '/api/admin/apocrypha/oracles';
export const EXPECTED_ANSWER = 'BROWSER ORACLE PASSED';
const PROFILE_MARKER = '.apocrypha-production-oracle-profile-v1';
const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const MAX_ERROR_CHARS = 600;

export interface OracleConfig {
  readonly origin: typeof PRODUCTION_ORIGIN;
  readonly profilePath: string;
  readonly profileState: 'ready' | 'missing';
  readonly bootstrap: boolean;
  readonly outputRoot: string;
}

interface StatLike { isDirectory(): boolean }

export function resolveOracleConfig(
  env: Readonly<Record<string, string | undefined>>,
  inspect: { exists(path: string): boolean; stat(path: string): StatLike } = { exists: existsSync, stat: statSync },
): OracleConfig {
  const configuredOrigin = env.APOCKY_E2E_BASE_URL?.replace(/\/$/, '') ?? PRODUCTION_ORIGIN;
  if (configuredOrigin !== PRODUCTION_ORIGIN) throw new Error('ORACLE_PRODUCTION_ORIGIN_REQUIRED');

  const rawProfile = env.APOCKY_ORACLE_PROFILE_PATH;
  if (!rawProfile || !isAbsolute(rawProfile)) throw new Error('ORACLE_PROFILE_PATH_REQUIRED');
  const profilePath = resolve(rawProfile);
  const bootstrap = env.APOCKY_ORACLE_BOOTSTRAP === '1';
  const profileExists = inspect.exists(profilePath);
  if (profileExists) {
    try {
      if (!inspect.stat(profilePath).isDirectory() || !inspect.exists(join(profilePath, PROFILE_MARKER))) {
        throw new Error('not-dedicated');
      }
    } catch {
      throw new Error('ORACLE_DEDICATED_PROFILE_MISSING');
    }
  } else if (!bootstrap) {
    throw new Error('ORACLE_DEDICATED_PROFILE_MISSING');
  }

  const outputRoot = resolve(env.APOCKY_ORACLE_OUTPUT_PATH ?? join(process.cwd(), 'test-results', 'apocrypha-production-oracle'));
  if (outputRoot === profilePath || outputRoot.startsWith(profilePath + '\\') || outputRoot.startsWith(profilePath + '/')) {
    throw new Error('ORACLE_OUTPUT_MUST_NOT_ENTER_PROFILE');
  }
  return { origin: PRODUCTION_ORIGIN, profilePath, profileState: profileExists ? 'ready' : 'missing', bootstrap, outputRoot };
}

export function sanitizeEvidence(value: unknown): string {
  let text = value instanceof Error ? value.message : String(value ?? '');
  text = text
    .replace(/https?:\/\/[^\s?#]+(?:\?[^\s#]*)?(?:#[^\s]*)?/gi, (url) => {
      try { return new URL(url).origin + new URL(url).pathname; } catch { return '[url]'; }
    })
    .replace(/\bBearer\s+[A-Za-z0-9._~+\/-]+=*/gi, 'Bearer [redacted]')
    .replace(/\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b/g, '[jwt-redacted]')
    .replace(/\b(?:access_token|refresh_token|authorization|cookie|set-cookie)\s*[:=]\s*[^\s,;]+/gi, '$1=[redacted]')
    .replace(/\b[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\b/gi, '[id-redacted]')
    .replace(/\b[A-Za-z0-9_+\/-]{80,}={0,2}\b/g, '[opaque-redacted]');
  return text.slice(0, MAX_ERROR_CHARS);
}

function record(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null;
}

function requiredUuid(value: unknown, code: string): string {
  if (typeof value !== 'string' || !UUID_V4.test(value)) throw new Error(code);
  return value.toLowerCase();
}

export function activeConversationRows(body: unknown): unknown[] | null {
  const data = record(record(body)?.data);
  return Array.isArray(data?.conversations) ? data.conversations : null;
}

async function requestJson(
  page: Page,
  method: 'GET' | 'POST' | 'DELETE',
  path: string,
  body?: Record<string, unknown>,
): Promise<{ status: number; body: unknown }> {
  return page.evaluate(async ({ requestMethod, requestPath, requestBody }) => {
    const response = await fetch(requestPath, {
      method: requestMethod,
      credentials: 'same-origin',
      cache: 'no-store',
      headers: requestBody ? { 'content-type': 'application/json' } : undefined,
      body: requestBody ? JSON.stringify(requestBody) : undefined,
    });
    let responseBody: unknown = null;
    try { responseBody = await response.json(); } catch { responseBody = null; }
    return { status: response.status, body: responseBody };
  }, { requestMethod: method, requestPath: path, requestBody: body });
}

async function requireOwner(page: Page): Promise<void> {
  const response = await requestJson(page, 'GET', '/api/auth/me');
  const body = record(response.body);
  if (response.status !== 200 || !record(body?.user)) throw new Error('ORACLE_SIGNED_IN_SESSION_REQUIRED');
  if (body?.authorized !== true || body?.owner_conversation !== true) throw new Error('ORACLE_OWNER_SESSION_REQUIRED');
}

export function redactedPathForEvidence(rawUrl: string): string | null {
  try {
    const url = new URL(rawUrl);
    if (url.origin !== PRODUCTION_ORIGIN) return null;
    if (url.pathname === '/api/auth/me' || url.pathname.startsWith('/api/admin/apocrypha/')) {
      return url.pathname.replace(
        /\b[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\b/gi,
        ':id',
      );
    }
  } catch { /* ignored */ }
  return null;
}

export async function runProductionBrowserOracle(config: OracleConfig): Promise<string> {
  if (config.bootstrap) throw new Error('ORACLE_BOOTSTRAP_MUST_NOT_RUN_LIVE_ORACLE');
  if (config.profileState !== 'ready') throw new Error('ORACLE_DEDICATED_PROFILE_MISSING');
  const nonce = randomUUID();
  const artifactDir = join(config.outputRoot, nonce);
  mkdirSync(artifactDir, { recursive: true });

  const network: Array<{ method: string; path: string; status: number; duration_ms: number }> = [];
  const errors: string[] = [];
  const screenshots: string[] = [];
  const starts = new WeakMap<Request, number>();
  let runId: string | null = null;
  let conversationId: string | null = null;
  let cleanupVerified = false;
  let context: Awaited<ReturnType<typeof chromium.launchPersistentContext>> | null = null;
  let primaryFailure: unknown = null;

  try {
    try {
      context = await chromium.launchPersistentContext(config.profilePath, {
        channel: 'chrome',
        headless: true,
        viewport: { width: 1440, height: 900 },
        colorScheme: 'dark',
        locale: 'en-US',
        reducedMotion: 'reduce',
      });
    } catch {
      throw new Error('ORACLE_PROFILE_UNAVAILABLE_OR_LOCKED');
    }
    const pages = context.pages();
    const page = pages[0] ?? await context.newPage();
    page.on('request', (request) => { if (redactedPathForEvidence(request.url())) starts.set(request, Date.now()); });
    page.on('response', (response) => {
      const request = response.request();
      const path = redactedPathForEvidence(response.url());
      if (!path) return;
      network.push({
        method: request.method(), path, status: response.status(),
        duration_ms: Math.max(0, Date.now() - (starts.get(request) ?? Date.now())),
      });
    });
    page.on('requestfailed', (request) => {
      const path = redactedPathForEvidence(request.url());
      if (path) errors.push(sanitizeEvidence(`requestfailed ${request.method()} ${path}: ${request.failure()?.errorText ?? 'unknown'}`));
    });
    page.on('pageerror', (error) => errors.push(sanitizeEvidence(`pageerror: ${error.message}`)));
    page.on('console', (message) => {
      if (message.type() === 'error' || message.type() === 'warning') {
        errors.push(sanitizeEvidence(`console-${message.type()}: ${message.text()}`));
      }
    });

    const navigation = await page.goto(`${config.origin}/apocrypha`, { waitUntil: 'domcontentloaded', timeout: 30_000 });
    if (!navigation || navigation.status() !== 200 || new URL(page.url()).origin !== config.origin) {
      throw new Error('ORACLE_PRODUCTION_NAVIGATION_FAILED');
    }
    await requireOwner(page);

    const created = await requestJson(page, 'POST', ORACLE_ENDPOINT, { nonce });
    const createdData = record(record(created.body)?.data);
    if (created.status !== 201 || record(created.body)?.ok !== true || !createdData) throw new Error('ORACLE_CREATE_FAILED');
    runId = requiredUuid(createdData.run_id, 'ORACLE_CREATE_RUN_ID_INVALID');
    conversationId = requiredUuid(createdData.conversation_id, 'ORACLE_CREATE_CONVERSATION_ID_INVALID');
    requiredUuid(createdData.job_id, 'ORACLE_CREATE_JOB_ID_INVALID');
    const prompt = createdData.prompt;
    if (typeof prompt !== 'string' || prompt.length < 1 || prompt.length > 500) throw new Error('ORACLE_CREATE_PROMPT_INVALID');

    await page.reload({ waitUntil: 'domcontentloaded', timeout: 30_000 });
    await page.getByText(prompt, { exact: true }).waitFor({ state: 'visible', timeout: 30_000 });
    const retry = page.getByRole('button', { name: 'Retry failed attempt' });
    await retry.waitFor({ state: 'visible', timeout: 30_000 });
    const shell = page.locator('.chat-main');
    await shell.screenshot({ path: join(artifactDir, 'failed-retry-visible.png') });
    screenshots.push('failed-retry-visible.png');
    await retry.click();
    await page.getByText(EXPECTED_ANSWER, { exact: true }).waitFor({ state: 'visible', timeout: 120_000 });
    await shell.screenshot({ path: join(artifactDir, 'retry-succeeded.png') });
    screenshots.push('retry-succeeded.png');
    if (errors.length > 0) throw new Error('ORACLE_BROWSER_OR_NETWORK_ERRORS');
  } catch (error) {
    primaryFailure = error;
  } finally {
    if (context && runId) {
      try {
        const page = context.pages()[0] ?? await context.newPage();
        const removed = await requestJson(page, 'DELETE', ORACLE_ENDPOINT, { run_id: runId });
        const removedData = record(record(removed.body)?.data);
        if (removed.status !== 200 || record(removed.body)?.ok !== true
          || removedData?.run_id !== runId || removedData?.cleaned !== true) throw new Error('ORACLE_CLEANUP_FAILED');
        if (!conversationId) throw new Error('ORACLE_CLEANUP_CONVERSATION_MISSING');
        const absent = await requestJson(page, 'GET', `/api/admin/apocrypha/conversations?id=${encodeURIComponent(conversationId)}`);
        const active = await requestJson(page, 'GET', '/api/admin/apocrypha/conversations?scope=active');
        const conversations = activeConversationRows(active.body);
        if (absent.status !== 404 || active.status !== 200 || !conversations
          || conversations.some((item) => record(item)?.id === conversationId)) throw new Error('ORACLE_CLEANUP_NOT_VERIFIED');
        cleanupVerified = true;
      } catch (cleanupError) {
        errors.push(sanitizeEvidence(cleanupError));
        if (!primaryFailure) primaryFailure = cleanupError;
      }
    }
    await context?.close().catch(() => undefined);

    const receipt = {
      schema: 'apocky.production-browser-oracle.v1',
      origin: config.origin,
      run_id: runId,
      passed: primaryFailure === null && cleanupVerified && errors.length === 0,
      cleanup_verified: cleanupVerified,
      screenshots,
      network,
      errors,
      observed_at: new Date().toISOString(),
    };
    writeFileSync(join(artifactDir, 'receipt.json'), JSON.stringify(receipt, null, 2) + '\n', { encoding: 'utf8', mode: 0o600 });
  }

  if (primaryFailure) throw new Error(sanitizeEvidence(primaryFailure));
  if (!runId || !cleanupVerified) throw new Error('ORACLE_INCOMPLETE');
  return join(artifactDir, 'receipt.json');
}

export async function bootstrapProductionBrowserOracle(config: OracleConfig): Promise<void> {
  if (!config.bootstrap) throw new Error('ORACLE_BOOTSTRAP_NOT_REQUESTED');
  if (config.profileState === 'missing') {
    try {
      // Deliberately non-recursive: bootstrap cannot manufacture or traverse an
      // unexpected parent tree, and cannot adopt an existing browser profile.
      mkdirSync(config.profilePath, { recursive: false });
      writeFileSync(join(config.profilePath, PROFILE_MARKER), 'apocky.production-browser-oracle-profile.v1\n', {
        encoding: 'utf8', flag: 'wx', mode: 0o600,
      });
    } catch {
      throw new Error('ORACLE_BOOTSTRAP_PROFILE_CREATE_FAILED');
    }
  }

  let context: Awaited<ReturnType<typeof chromium.launchPersistentContext>> | null = null;
  try {
    try {
      context = await chromium.launchPersistentContext(config.profilePath, {
        channel: 'chrome', headless: false, viewport: { width: 1440, height: 900 },
        colorScheme: 'dark', locale: 'en-US', reducedMotion: 'reduce',
      });
    } catch {
      throw new Error('ORACLE_PROFILE_UNAVAILABLE_OR_LOCKED');
    }
    const page = context.pages()[0] ?? await context.newPage();
    const navigation = await page.goto(`${config.origin}/login?next=%2Fapocrypha`, {
      waitUntil: 'domcontentloaded', timeout: 30_000,
    });
    if (!navigation || new URL(page.url()).origin !== config.origin) throw new Error('ORACLE_BOOTSTRAP_NAVIGATION_FAILED');

    const deadline = Date.now() + 10 * 60_000;
    while (Date.now() < deadline) {
      try {
        await requireOwner(page);
        return;
      } catch (error) {
        if (error instanceof Error && error.message === 'ORACLE_OWNER_SESSION_REQUIRED') {
          throw error;
        }
      }
      await page.waitForTimeout(1_000);
    }
    throw new Error('ORACLE_BOOTSTRAP_OWNER_TIMEOUT');
  } finally {
    await context?.close().catch(() => undefined);
  }
}

async function main(): Promise<void> {
  const config = resolveOracleConfig(process.env);
  if (config.bootstrap) {
    await bootstrapProductionBrowserOracle(config);
    process.stdout.write('apocrypha production browser oracle: BOOTSTRAP PASS · dedicated owner profile ready\n');
    return;
  }
  const receipt = await runProductionBrowserOracle(config);
  process.stdout.write(`apocrypha production browser oracle: PASS · receipt=${receipt}\n`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  void main().catch((error: unknown) => {
    process.stderr.write(`apocrypha production browser oracle: FAIL · ${sanitizeEvidence(error)}\n`);
    process.exitCode = 1;
  });
}
