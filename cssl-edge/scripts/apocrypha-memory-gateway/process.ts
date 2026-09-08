import { spawn } from 'node:child_process';

export class NativeProcessError extends Error {
  constructor(readonly code: string) {
    super(code);
  }
}

function closedEnvironment(): NodeJS.ProcessEnv {
  const result = { NODE_ENV: process.env.NODE_ENV ?? 'production' } as NodeJS.ProcessEnv;
  for (const name of ['SystemRoot', 'WINDIR', 'TEMP', 'TMP', 'USERPROFILE', 'LOCALAPPDATA', 'APPDATA']) {
    if (process.env[name]) result[name] = process.env[name];
  }
  return result;
}

export async function runBoundedJsonl(
  executable: string,
  args: string[],
  input: unknown[],
  signal: AbortSignal,
  outputBytes: number,
): Promise<unknown[]> {
  if (signal.aborted) throw new NativeProcessError('NATIVE_ABORTED');
  return new Promise((resolve, reject) => {
    const child = spawn(executable, args, {
      stdio: ['pipe', 'pipe', 'pipe'],
      windowsHide: true,
      shell: false,
      env: closedEnvironment(),
    });
    const stdout: Buffer[] = [];
    let stdoutBytes = 0;
    let stderrBytes = 0;
    let settled = false;
    const finish = (error?: Error, value?: unknown[]) => {
      if (settled) return;
      settled = true;
      signal.removeEventListener('abort', abort);
      if (error) reject(error); else resolve(value ?? []);
    };
    const abort = () => {
      child.kill();
      finish(new NativeProcessError('NATIVE_TIMEOUT'));
    };
    signal.addEventListener('abort', abort, { once: true });
    child.once('error', () => finish(new NativeProcessError('NATIVE_UNAVAILABLE')));
    child.stdout.on('data', (chunk: Buffer) => {
      stdoutBytes += chunk.length;
      if (stdoutBytes > outputBytes) {
        child.kill();
        finish(new NativeProcessError('NATIVE_OUTPUT_LIMIT'));
        return;
      }
      stdout.push(Buffer.from(chunk));
    });
    child.stderr.on('data', (chunk: Buffer) => {
      stderrBytes += chunk.length;
      if (stderrBytes > 16_384) child.kill();
    });
    child.once('close', (code) => {
      if (settled) return;
      if (code !== 0) return finish(new NativeProcessError('NATIVE_FAILED'));
      try {
        const lines = Buffer.concat(stdout).toString('utf8').split(/\r?\n/u).filter(Boolean);
        finish(undefined, lines.map((line) => JSON.parse(line) as unknown));
      } catch {
        finish(new NativeProcessError('NATIVE_RESPONSE_INVALID'));
      }
    });
    try {
      for (const frame of input) child.stdin.write(`${JSON.stringify(frame)}\n`);
      child.stdin.end();
    } catch {
      child.kill();
      finish(new NativeProcessError('NATIVE_INPUT_FAILED'));
    }
  });
}

export function utf8Prefix(value: string, maximumBytes: number): string {
  const bytes = Buffer.from(value, 'utf8');
  if (bytes.length <= maximumBytes) return value;
  let end = maximumBytes;
  while (end > 0 && (bytes[end]! & 0xc0) === 0x80) end -= 1;
  return bytes.subarray(0, end).toString('utf8');
}
