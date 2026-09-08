export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

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
  const entry = JSON.stringify({
    at: new Date().toISOString(),
    level,
    event,
    ...sanitize(fields) as Record<string, unknown>,
  });
  if (level === 'error') process.stderr.write(`${entry}\n`);
  else process.stdout.write(`${entry}\n`);
}
