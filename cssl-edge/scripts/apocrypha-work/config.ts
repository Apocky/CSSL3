import { randomBytes } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import type { RiskTier, WorkConfig } from './types';

type Env = Record<string, string | undefined>;

function required(env: Env, key: string): string {
  const value = env[key]?.trim();
  if (!value) throw new Error(`${key} is required`);
  return value;
}

function integer(env: Env, key: string, fallback: number, min: number, max: number): number {
  const raw = env[key]?.trim();
  if (!raw) return fallback;
  const value = Number(raw);
  if (!Number.isInteger(value) || value < min || value > max) {
    throw new Error(`${key} must be an integer in [${min}, ${max}]`);
  }
  return value;
}

function decimal(env: Env, key: string, fallback: number, min: number, max: number): number {
  const raw = env[key]?.trim();
  if (!raw) return fallback;
  const value = Number(raw);
  if (!Number.isFinite(value) || value < min || value > max) throw new Error(`${key} must be in [${min}, ${max}]`);
  return value;
}

/**
 * Read the shared token, minting one on first run.
 *
 * The token is the only thing standing between a local process and a tool that can write files
 * and run commands, so it is 32 bytes of CSPRNG output and the file is created 0600. It is never
 * logged and never returned by any route.
 */
function loadToken(stateDir: string, env: Env): string {
  const supplied = env.APOCRYPHA_WORK_TOKEN?.trim();
  if (supplied) {
    if (supplied.length < 24) throw new Error('APOCRYPHA_WORK_TOKEN must be at least 24 characters');
    return supplied;
  }
  const path = join(stateDir, 'work.token');
  if (existsSync(path)) {
    const existing = readFileSync(path, 'utf8').trim();
    if (existing.length >= 24) return existing;
  }
  const minted = randomBytes(32).toString('base64url');
  writeFileSync(path, minted, { encoding: 'utf8', mode: 0o600 });
  try {
    chmodSync(path, 0o600);
  } catch {
    // Windows ignores POSIX modes; the ACL inherited from the state directory governs there.
  }
  return minted;
}

function parseRoots(env: Env): { label: string; path: string; writable: boolean }[] {
  // Format: "label=path[:ro]" entries separated by ";". A root is writable unless marked :ro.
  const raw = required(env, 'APOCRYPHA_WORK_ROOTS');
  const roots = raw.split(';').map((entry) => entry.trim()).filter(Boolean).map((entry) => {
    const readOnly = entry.endsWith(':ro');
    const body = readOnly ? entry.slice(0, -3) : entry;
    const split = body.indexOf('=');
    if (split <= 0) throw new Error(`APOCRYPHA_WORK_ROOTS entry must be "label=path": ${entry}`);
    return {
      label: body.slice(0, split).trim(),
      path: resolve(body.slice(split + 1).trim()),
      writable: !readOnly,
    };
  });
  if (roots.length === 0) throw new Error('APOCRYPHA_WORK_ROOTS resolved to no roots');
  const labels = new Set(roots.map((root) => root.label));
  if (labels.size !== roots.length) throw new Error('APOCRYPHA_WORK_ROOTS labels must be unique');
  return roots;
}

// Commands that are never worth the blast radius even with consent. A deny here is final and is
// not offered for approval.
//
// Every rule anchors on COMMAND POSITION (start of string, or just after a newline, ; && || |).
// A bare \bformat\b looks right and is not: it also blocks `python scripts/format.py`, `npm run
// format` and `cargo fmt --check`, which is how a safety rule earns a reputation for being in the
// way and gets switched off. The test suite carries the near-misses that pin this down.
const CMD = String.raw`(?:^|[\n;|&]\s*)`;
const deny = (body: string): RegExp => new RegExp(CMD + body, 'i');

const SHELL_DENY = [
  // Disk destruction. `format` only counts when aimed at a drive letter.
  deny(String.raw`(?:format(?:\.com|\.exe)?\s+[a-z]:|diskpart\b|mkfs(?:\.\w+)?\s)`),
  // Recursive AND forced removal, in either flag order, combined or separate.
  deny(String.raw`rm\s+(?:-[a-z]*\s+)*-[a-z]*(?:rf|fr)\b`),
  deny(String.raw`rm\s+(?:-[a-z]*\s+)*(?=-[a-z]*r\b)(?:-[a-z]*\s+)*-[a-z]*f\b`),
  deny(String.raw`rm\s+(?:-[a-z]*\s+)*(?=-[a-z]*f\b)(?:-[a-z]*\s+)*-[a-z]*r\b`),
  /Remove-Item[^|;]*-Recurse[^|;]*-Force|Remove-Item[^|;]*-Force[^|;]*-Recurse/i,
  // Host power state.
  deny(String.raw`(?:shutdown|Reset-Computer|Restart-Computer)\b`),
  // System and boot configuration.
  deny(String.raw`(?:netsh|bcdedit)\b`),
  deny(String.raw`reg(?:\.exe)?\s+(?:add|delete)\b`),
  /Set-ItemProperty\s+-Path\s+HK/i,
  // History rewrite on a remote. --force-with-lease is the careful form and stays allowed.
  /\bgit\s+push\b(?![^\n;|&]*--force-with-lease)[^\n;|&]*(?:--force\b|\s-f\b)/i,
  // Pipe-to-interpreter, in both shells.
  /\bcurl\b[^|;]*\|\s*(?:ba|z|k)?sh\b|\biwr\b[^|;]*\|\s*iex\b|\bInvoke-Expression\b/i,
];

export function loadWorkConfig(env: Env = process.env): WorkConfig {
  const stateDir = resolve(env.APOCRYPHA_WORK_STATE_DIR?.trim() || 'D:/Apocrypha/work');
  mkdirSync(stateDir, { recursive: true });

  const autoApprove = (env.APOCRYPHA_WORK_AUTO_APPROVE?.trim() || 'read')
    .split(',').map((entry) => entry.trim()).filter(Boolean) as RiskTier[];
  for (const tier of autoApprove) {
    if (!['read', 'write', 'execute'].includes(tier)) throw new Error(`unknown risk tier in APOCRYPHA_WORK_AUTO_APPROVE: ${tier}`);
  }

  return {
    host: env.APOCRYPHA_WORK_HOST?.trim() || '127.0.0.1',
    port: integer(env, 'APOCRYPHA_WORK_PORT', 19130, 1024, 65535),
    token: loadToken(stateDir, env),
    stateDir,
    roots: parseRoots(env),
    engine: {
      alias: env.APOCRYPHA_WORK_MODEL_ALIAS?.trim() || 'work-coder',
      baseUrl: (env.APOCRYPHA_WORK_ENGINE_URL?.trim() || 'http://127.0.0.1:19131').replace(/\/+$/, ''),
      contextWindow: integer(env, 'APOCRYPHA_WORK_CONTEXT', 32768, 4096, 262144),
      maxOutputTokens: integer(env, 'APOCRYPHA_WORK_MAX_OUTPUT', 4096, 256, 32768),
      temperature: decimal(env, 'APOCRYPHA_WORK_TEMPERATURE', 0.2, 0, 2),
      topP: decimal(env, 'APOCRYPHA_WORK_TOP_P', 0.95, 0, 1),
      topK: integer(env, 'APOCRYPHA_WORK_TOP_K', 40, 0, 200),
    },
    maxToolIterations: integer(env, 'APOCRYPHA_WORK_MAX_ITERATIONS', 40, 1, 200),
    toolTimeoutMs: integer(env, 'APOCRYPHA_WORK_TOOL_TIMEOUT_MS', 120_000, 1_000, 900_000),
    turnTimeoutMs: integer(env, 'APOCRYPHA_WORK_TURN_TIMEOUT_MS', 1_800_000, 10_000, 7_200_000),
    shellAllowed: env.APOCRYPHA_WORK_SHELL?.trim().toLowerCase() !== 'off',
    mcpConfigPath: resolve(env.APOCRYPHA_WORK_MCP_CONFIG?.trim() || join(stateDir, 'mcp.json')),
    shellDenyPatterns: SHELL_DENY,
    autoApprove,
    arbiter: {
      // Default is 'manual': the arbiter will never take the GPU from the public Chat lane on its
      // own initiative. 'auto' lets a Work turn trigger the handover once Chat reports idle.
      mode: (['off', 'manual', 'auto'].includes(env.APOCRYPHA_WORK_ARBITER?.trim() ?? '')
        ? env.APOCRYPHA_WORK_ARBITER?.trim() : 'manual') as 'off' | 'manual' | 'auto',
      // One port for both lanes. Defaults to the port the Chat worker already talks to, so the
      // worker needs no reconfiguration and never notices which model is answering.
      enginePort: integer(env, 'APOCRYPHA_WORK_ENGINE_PORT', 19128, 1024, 65535),
      chatModelPath: env.APOCRYPHA_WORK_CHAT_MODEL?.trim()
        || 'D:\Apocrypha\models\Qwen3.5-35B-A3B-Q4\Qwen3.5-35B-A3B-Q4_K_S.gguf',
      workModelPath: env.APOCRYPHA_WORK_MODEL?.trim()
        || 'C:\Apocrypha\models\work-lane\Qwen3-Coder-Next-UD-Q2_K_XL.gguf',
      chatLauncher: env.APOCRYPHA_WORK_CHAT_LAUNCHER?.trim()
        || 'C:\\Users\\Apocky\\source\\repos\\apocrypha-core\\tools\\run-qwen35-vulkan.ps1',
      workLauncher: env.APOCRYPHA_WORK_ENGINE_LAUNCHER?.trim()
        || 'C:\\Users\\Apocky\\source\\repos\\apocrypha-core\\tools\\run-work-engine.ps1',
      workProfile: env.APOCRYPHA_WORK_ENGINE_PROFILE?.trim() || 'exclusive',
      chatWorkerHealthUrl: env.APOCRYPHA_WORK_CHAT_WORKER_HEALTH?.trim() || 'http://127.0.0.1:19126/health',
      drainTimeoutMs: integer(env, 'APOCRYPHA_WORK_DRAIN_TIMEOUT_MS', 120_000, 5_000, 900_000),
      startTimeoutMs: integer(env, 'APOCRYPHA_WORK_START_TIMEOUT_MS', 420_000, 30_000, 1_800_000),
      idleYieldMs: integer(env, 'APOCRYPHA_WORK_IDLE_YIELD_MS', 900_000, 0, 86_400_000),
      launcherLogDir: env.APOCRYPHA_WORK_LAUNCHER_LOG_DIR?.trim() || stateDir,
    },
  };
}
