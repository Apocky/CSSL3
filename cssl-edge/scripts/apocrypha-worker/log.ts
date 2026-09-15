import { appendFileSync, existsSync, renameSync, statSync } from 'node:fs';

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface LogEntry {
  at: string;
  level: LogLevel;
  event: string;
  fields: Record<string, unknown>;
}

// The last events, in memory, so /health can show what just happened without
// anyone having to find the right log file - or any log file: a worker started
// by hand from a terminal used to log nowhere durable (2026-09-12).
const RING_MAX = 60;
const ring: LogEntry[] = [];

let logFile: string | null = null;
let writesSinceSizeCheck = 0;
const ROTATE_BYTES = 20 * 1024 * 1024;

/** Every event is also appended to this file, whatever launched the process. */
export function configureLogFile(path: string): void {
  logFile = path;
}

export function currentLogFile(): string | null {
  return logFile;
}

export function recentEvents(limit = 20): LogEntry[] {
  return ring.slice(-limit);
}

function appendToFile(line: string): void {
  if (!logFile) return;
  try {
    writesSinceSizeCheck += 1;
    if (writesSinceSizeCheck >= 200) {
      writesSinceSizeCheck = 0;
      if (existsSync(logFile) && statSync(logFile).size > ROTATE_BYTES) renameSync(logFile, `${logFile}.1`);
    }
    appendFileSync(logFile, `${line}\n`);
  } catch {
    // A failing sink must never take the worker down; stdout still has the line.
  }
}

const secretKey = /^(authorization|.*(?:secret|password|credential|api_key|access_key|private_key)|(?:node|lease|worker|read|bearer)_token)$/i;

function sanitize(value: unknown, depth = 0): unknown {
  if (depth > 4) return '[bounded]';
  if (Array.isArray(value)) return value.slice(0, 30).map((item) => sanitize(item, depth + 1));
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value as Record<string, unknown>).map(([key, item]) => [
      key,
      secretKey.test(key) ? '[redacted]' : sanitize(item, depth + 1),
    ]));
  }
  if (typeof value === 'string') return value.slice(0, 4_000);
  return value;
}

export function log(level: LogLevel, event: string, fields: Record<string, unknown> = {}): void {
  const at = new Date().toISOString();
  const clean = sanitize(fields) as Record<string, unknown>;
  const entry = JSON.stringify({ at, level, event, ...clean });
  if (level === 'error') process.stderr.write(`${entry}\n`);
  else process.stdout.write(`${entry}\n`);
  appendToFile(entry);
  ring.push({ at, level, event, fields: clean });
  if (ring.length > RING_MAX) ring.splice(0, ring.length - RING_MAX);
}
