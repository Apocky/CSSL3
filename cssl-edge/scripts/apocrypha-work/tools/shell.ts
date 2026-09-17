import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { join } from 'node:path';
import type { ToolDefinition } from '../types';
import type { ToolContext, ToolResult } from './files';

const MAX_OUTPUT_BYTES = 256 * 1024;

export class ShellDenied extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ShellDenied';
  }
}

export const SHELL_TOOLS: ToolDefinition[] = [
  {
    name: 'run_command',
    risk: 'execute',
    description:
      'Run a shell command inside the workspace and return its combined output. Use for builds, tests, linters, '
      + 'and git. The operator sees and approves the exact command before it runs.',
    parameters: {
      type: 'object',
      properties: {
        command: { type: 'string', description: 'The command line to run.' },
        cwd: { type: 'string', description: 'Working directory inside the workspace. Defaults to the primary root.' },
        shell: { type: 'string', enum: ['pwsh', 'powershell', 'bash'], description: 'Explicit interpreter. Omit to use the verified local PowerShell installation.' },
      },
      required: ['command'],
    },
  },
];

export interface ShellPolicy {
  readonly allowed: boolean;
  readonly denyPatterns: readonly RegExp[];
  readonly timeoutMs: number;
}

const POWERSHELL_ARGS = ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command'];
const shellChecks = new Map<string, Promise<string>>();

function verifiedPowerShell(requested: string): Promise<string> {
  const existing = shellChecks.get(requested);
  if (existing) return existing;
  const modern = process.platform === 'win32'
    ? join(process.env.ProgramFiles || 'C:\\Program Files', 'PowerShell', '7', 'pwsh.exe')
    : 'pwsh';
  const legacy = process.platform === 'win32'
    ? join(process.env.SystemRoot || 'C:\\Windows', 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe')
    : '';
  const candidates = requested === 'powershell' ? [legacy]
    : requested === 'pwsh' ? [modern] : [modern, legacy];
  const check = (async () => {
    const failures: string[] = [];
    for (const executable of candidates.filter(Boolean)) {
      const marker = `apocrypha-shell-${randomUUID()}`;
      const ready = await new Promise<boolean>((resolve) => {
        const child = spawn(executable, [...POWERSHELL_ARGS, `Write-Output '${marker}'; exit 17`], {
          windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
        });
        let output = '';
        child.stdout.on('data', (chunk: Buffer) => { output = (output + chunk.toString('utf8')).slice(0, 1_024); });
        child.stderr.resume();
        const timer = setTimeout(() => { child.kill(); resolve(false); }, 5_000);
        child.once('error', () => { clearTimeout(timer); resolve(false); });
        child.once('close', (code) => { clearTimeout(timer); resolve(code === 17 && output.trim() === marker); });
      });
      if (ready) return executable;
      failures.push(executable);
    }
    throw new Error(`No verified ${requested || 'PowerShell'} interpreter. Startup output/exit check failed: ${failures.join(', ')}.`);
  })();
  shellChecks.set(requested, check);
  void check.catch(() => shellChecks.delete(requested));
  return check;
}

/**
 * Screen a command before it is ever shown for approval.
 *
 * A denied command is not offered to the operator at all. The point is that consent should not be
 * the only thing standing between a stray token and `format C:` — an approval dialog that appears
 * a hundred times a session gets clicked through, and the hundred-and-first is the bad one.
 */
export function screenCommand(command: string, policy: ShellPolicy): void {
  if (!policy.allowed) throw new ShellDenied('command execution is disabled for this service (APOCRYPHA_WORK_SHELL=off)');
  // Match the command as written. Flattening newlines to spaces first would look like tidying and
  // would in fact disarm every rule that anchors on a command boundary, since a newline IS one:
  // "git status\nrm -rf build" would arrive as one long argument list and match nothing.
  const normalised = command.replace(/\r\n?/g, '\n');
  for (const pattern of policy.denyPatterns) {
    // Patterns are module-level literals; a stray /g would make .test stateful across calls.
    pattern.lastIndex = 0;
    if (pattern.test(normalised)) throw new ShellDenied(`command matches a standing deny rule (${pattern.source.slice(0, 60)})`);
  }
}

export async function runShellTool(
  name: string,
  args: Record<string, unknown>,
  ctx: ToolContext,
  policy: ShellPolicy,
): Promise<ToolResult> {
  if (name !== 'run_command') throw new Error(`unknown shell tool: ${name}`);
  ctx.signal.throwIfAborted();
  const command = String(args.command ?? '').trim();
  if (!command) throw new Error('command must not be empty');
  screenCommand(command, policy);

  const { path: cwd } = await ctx.workspace.resolveExisting(String(args.cwd ?? '.'), 'read');
  const requested = String(args.shell ?? '');
  if (!['', 'pwsh', 'powershell', 'bash'].includes(requested)) throw new Error(`unsupported shell: ${requested}`);
  const [file, argv] = requested === 'bash'
    ? ['bash', ['-lc', command]]
    : [await verifiedPowerShell(requested), [...POWERSHELL_ARGS, command]];
  ctx.signal.throwIfAborted();

  return await new Promise<ToolResult>((resolve, reject) => {
    const child = spawn(file, argv as string[], {
      cwd,
      windowsHide: true,
      // The command is the approval unit and is passed as a single argument to the interpreter,
      // so no additional quoting layer is introduced here.
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    let bytes = 0;
    let truncated = false;
    const chunks: string[] = [];
    const absorb = (data: Buffer): void => {
      bytes += data.byteLength;
      if (bytes > MAX_OUTPUT_BYTES) {
        if (!truncated) { truncated = true; chunks.push('\n… output truncated'); }
        return;
      }
      chunks.push(data.toString('utf8'));
    };
    child.stdout.on('data', absorb);
    child.stderr.on('data', absorb);

    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      reject(new Error(`command exceeded ${Math.round(policy.timeoutMs / 1_000)}s and was killed`));
    }, policy.timeoutMs);

    const onAbort = (): void => { child.kill('SIGKILL'); };
    ctx.signal.addEventListener('abort', onAbort, { once: true });

    child.on('error', (error) => {
      clearTimeout(timer);
      ctx.signal.removeEventListener('abort', onAbort);
      reject(new Error(`failed to start ${file}: ${error.message}`));
    });

    child.on('close', (code, signal) => {
      clearTimeout(timer);
      ctx.signal.removeEventListener('abort', onAbort);
      const output = chunks.join('').trim();
      const status = signal ? `killed by ${signal}` : `exit ${code}`;
      if (ctx.signal.aborted || signal || code !== 0) {
        reject(new Error(`${ctx.signal.aborted ? 'command cancelled' : 'command failed'} (${status})\n${output || '(no output)'}`));
        return;
      }
      resolve({
        summary: `${command.slice(0, 80)}${command.length > 80 ? '…' : ''} — ${status}`,
        content: `$ ${command}\n(${ctx.workspace.describe(cwd)}, ${status}, shell: ${file})\n\n${output || '(no output)'}`,
      });
    });
  });
}
