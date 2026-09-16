// MCP over stdio -- other people's tools, in this agent's loop.
//
// The file and shell tools are ours: we wrote them, we know their risk tier, they cannot vanish
// mid-session. An MCP server is none of those things. It is a child process someone else wrote that
// may be missing, may hang on the handshake, may print junk to stdout, and may die in the middle of
// a turn. Every design choice below follows from that: nothing here may take the service down, and
// a broken server must degrade to "that one tool is unavailable" rather than "no coder today".
//
// Wire format is newline-delimited JSON-RPC 2.0 on the child's stdin/stdout. Servers are expected to
// log to stderr; a line on stdout that does not parse as JSON is skipped rather than fatal, because
// a single chatty server should not poison the channel.
//
// Config is the standard shape, so an existing mcp.json can be pointed at unchanged:
//   { "mcpServers": { "aegis": { "command": "node", "args": ["..."], "env": {} } } }

import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { readFileSync } from 'node:fs';
import type { ToolDefinition } from '../types';
import { log } from '../log';

/** Tool names reaching the engine must be [a-zA-Z0-9_-]{1,64}; llama.cpp rejects the rest. */
const NAME_MAX = 64;
const PREFIX = 'mcp__';
const HANDSHAKE_TIMEOUT_MS = 20_000;
const CALL_TIMEOUT_MS = 120_000;

interface ServerSpec {
  readonly command: string;
  readonly args: readonly string[];
  readonly env: Readonly<Record<string, string>>;
}

interface RpcResponse {
  readonly id?: number | string;
  readonly result?: Record<string, unknown>;
  readonly error?: { code?: number; message?: string };
}

/** `mcp__<server>__<tool>`, truncated from the SERVER side so the tool name stays readable. */
export function qualify(server: string, tool: string): string {
  const safe = (value: string): string => value.replace(/[^a-zA-Z0-9_-]/g, '_');
  const full = `${PREFIX}${safe(server)}__${safe(tool)}`;
  if (full.length <= NAME_MAX) return full;
  const room = NAME_MAX - PREFIX.length - 2 - safe(tool).length;
  // A tool name alone longer than the budget is unusable either way; keep the tail, which is the
  // part that identifies the operation.
  if (room < 1) return full.slice(0, NAME_MAX);
  return `${PREFIX}${safe(server).slice(0, room)}__${safe(tool)}`;
}

/**
 * Engines reject schemas they cannot walk. Keep the subset every provider agrees on and drop the
 * rest -- a tool with a slightly loose schema still works, a tool that 400s the request does not.
 */
function sanitizeSchema(input: unknown): Record<string, unknown> {
  if (!input || typeof input !== 'object' || Array.isArray(input)) {
    return { type: 'object', properties: {} };
  }
  const source = input as Record<string, unknown>;
  const out: Record<string, unknown> = { type: 'object' };
  if (source.properties && typeof source.properties === 'object') out.properties = source.properties;
  else out.properties = {};
  if (Array.isArray(source.required)) out.required = source.required;
  if (typeof source.description === 'string') out.description = source.description;
  return out;
}

class McpServer {
  readonly name: string;
  private readonly spec: ServerSpec;
  private child: ChildProcessWithoutNullStreams | null = null;
  private buffer = '';
  private nextId = 1;
  private readonly pending = new Map<number | string, { resolve: (value: RpcResponse) => void; reject: (error: Error) => void; timer: NodeJS.Timeout }>();
  /** Set once the process dies or the handshake fails. Every later call fails fast against it. */
  private dead: string | null = null;
  tools: ToolDefinition[] = [];
  /** Original MCP tool name, keyed by the qualified name the engine sees. */
  readonly originalName = new Map<string, string>();

  constructor(name: string, spec: ServerSpec) {
    this.name = name;
    this.spec = spec;
  }

  private fail(reason: string): void {
    if (this.dead) return;
    this.dead = reason;
    for (const [, entry] of this.pending) {
      clearTimeout(entry.timer);
      entry.reject(new Error(reason));
    }
    this.pending.clear();
    log('warn', 'work.mcp.server_down', { server: this.name, reason });
  }

  private onStdout(chunk: Buffer): void {
    this.buffer += chunk.toString('utf8');
    const lines = this.buffer.split('\n');
    this.buffer = lines.pop() ?? '';
    for (const line of lines) {
      const text = line.trim();
      if (!text) continue;
      let message: RpcResponse;
      // A server that logs to stdout is common and is not a protocol violation worth dying over.
      try { message = JSON.parse(text) as RpcResponse; } catch { continue; }
      if (message.id === undefined) continue; // notification; nothing waits on it
      const entry = this.pending.get(message.id);
      if (!entry) continue;
      clearTimeout(entry.timer);
      this.pending.delete(message.id);
      entry.resolve(message);
    }
  }

  private async rpc(method: string, params: Record<string, unknown>, timeoutMs: number): Promise<Record<string, unknown>> {
    if (this.dead) throw new Error(`mcp server ${this.name} is unavailable: ${this.dead}`);
    const child = this.child;
    if (!child) throw new Error(`mcp server ${this.name} is not started`);
    const id = this.nextId++;
    const payload = `${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`;
    const response = await new Promise<RpcResponse>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`mcp ${this.name}.${method} timed out after ${timeoutMs}ms`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      child.stdin.write(payload, (error) => { if (error) { clearTimeout(timer); this.pending.delete(id); reject(error); } });
    });
    if (response.error) throw new Error(response.error.message ?? `mcp ${this.name}.${method} failed`);
    return response.result ?? {};
  }

  private notify(method: string, params: Record<string, unknown>): void {
    this.child?.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method, params })}\n`);
  }

  /** Spawn, handshake and list tools. Never throws: a failed server is a degraded server. */
  async start(): Promise<void> {
    try {
      // shell:true is what makes `npx` and other .cmd shims spawnable on Windows, where the bare
      // name is not an executable. This process already runs with full operator authority.
      this.child = spawn(this.spec.command, [...this.spec.args], {
        stdio: ['pipe', 'pipe', 'pipe'],
        env: { ...process.env, ...this.spec.env },
        shell: process.platform === 'win32',
      }) as ChildProcessWithoutNullStreams;
    } catch (error) {
      this.fail(error instanceof Error ? error.message : String(error));
      return;
    }
    this.child.on('error', (error) => this.fail(error.message));
    this.child.on('exit', (code) => this.fail(`exited with code ${code ?? 'null'}`));
    this.child.stdout.on('data', (chunk: Buffer) => this.onStdout(chunk));
    this.child.stderr.resume(); // drain, or a chatty server fills the pipe and blocks itself

    try {
      await this.rpc('initialize', {
        protocolVersion: '2024-11-05',
        capabilities: {},
        clientInfo: { name: 'apocrypha-work', version: '1' },
      }, HANDSHAKE_TIMEOUT_MS);
      this.notify('notifications/initialized', {});
      const listed = await this.rpc('tools/list', {}, HANDSHAKE_TIMEOUT_MS);
      const tools = Array.isArray(listed.tools) ? listed.tools : [];
      for (const entry of tools) {
        if (!entry || typeof entry !== 'object') continue;
        const tool = entry as { name?: unknown; description?: unknown; inputSchema?: unknown; annotations?: { readOnlyHint?: unknown } };
        if (typeof tool.name !== 'string' || !tool.name) continue;
        const qualified = qualify(this.name, tool.name);
        this.originalName.set(qualified, tool.name);
        this.tools.push({
          name: qualified,
          // MCP has no risk tier. readOnlyHint is the only signal a server gives, and its absence
          // means "assume this does something", so the default is the most restrictive tier.
          risk: tool.annotations?.readOnlyHint === true ? 'read' : 'execute',
          description: typeof tool.description === 'string' ? tool.description.slice(0, 1024) : `${this.name} tool ${tool.name}`,
          parameters: sanitizeSchema(tool.inputSchema),
        });
      }
      log('info', 'work.mcp.server_ready', { server: this.name, tools: this.tools.length });
    } catch (error) {
      this.fail(error instanceof Error ? error.message : String(error));
    }
  }

  async call(qualified: string, args: Record<string, unknown>, timeoutMs: number): Promise<{ ok: boolean; content: string }> {
    const original = this.originalName.get(qualified);
    if (!original) return { ok: false, content: `unknown tool ${qualified}` };
    const result = await this.rpc('tools/call', { name: original, arguments: args }, timeoutMs);
    // MCP returns typed content parts. Flatten to text -- the engine reads text, and a tool that
    // returns only an image has nothing to say to a coder in a terminal anyway.
    const parts = Array.isArray(result.content) ? result.content : [];
    const text = parts
      .map((part) => {
        if (!part || typeof part !== 'object') return '';
        const typed = part as { type?: unknown; text?: unknown };
        if (typed.type === 'text' && typeof typed.text === 'string') return typed.text;
        return `[${String(typed.type ?? 'content')}]`;
      })
      .filter(Boolean)
      .join('\n');
    return { ok: result.isError !== true, content: text || '(no content)' };
  }

  stop(): void {
    this.dead ??= 'stopped';
    this.child?.kill();
  }
}

export class McpHub {
  private readonly servers = new Map<string, McpServer>();
  /** Qualified tool name -> owning server. */
  private readonly routes = new Map<string, McpServer>();

  static parseConfig(raw: string): Map<string, ServerSpec> {
    const out = new Map<string, ServerSpec>();
    const parsed = JSON.parse(raw) as { mcpServers?: Record<string, unknown> };
    const entries = parsed.mcpServers ?? {};
    for (const [name, value] of Object.entries(entries)) {
      if (!value || typeof value !== 'object') continue;
      const spec = value as { command?: unknown; args?: unknown; env?: unknown; disabled?: unknown };
      if (spec.disabled === true) continue;
      if (typeof spec.command !== 'string' || !spec.command) continue;
      out.set(name, {
        command: spec.command,
        args: Array.isArray(spec.args) ? spec.args.filter((a): a is string => typeof a === 'string') : [],
        env: spec.env && typeof spec.env === 'object' ? (spec.env as Record<string, string>) : {},
      });
    }
    return out;
  }

  /**
   * Start every configured server. Returns the tools that came up.
   *
   * Servers start concurrently: one server taking the full handshake timeout should not add its
   * delay to every server behind it.
   */
  async start(configPath: string): Promise<readonly ToolDefinition[]> {
    let raw: string;
    try { raw = readFileSync(configPath, 'utf8'); }
    catch { log('info', 'work.mcp.no_config', { path: configPath }); return []; }

    let specs: Map<string, ServerSpec>;
    try { specs = McpHub.parseConfig(raw); }
    catch (error) { log('warn', 'work.mcp.bad_config', { path: configPath, error: error instanceof Error ? error.message : String(error) }); return []; }

    await Promise.all([...specs].map(async ([name, spec]) => {
      const server = new McpServer(name, spec);
      this.servers.set(name, server);
      await server.start();
    }));

    const tools: ToolDefinition[] = [];
    for (const server of this.servers.values()) {
      for (const tool of server.tools) {
        // First server to claim a qualified name keeps it. Collisions only happen when two servers
        // share a prefix after truncation, which is rare and never worth dropping both.
        if (this.routes.has(tool.name)) continue;
        this.routes.set(tool.name, server);
        tools.push(tool);
      }
    }
    return tools;
  }

  owns(name: string): boolean { return this.routes.has(name); }

  async call(name: string, args: Record<string, unknown>, timeoutMs = CALL_TIMEOUT_MS): Promise<{ ok: boolean; content: string }> {
    const server = this.routes.get(name);
    if (!server) return { ok: false, content: `no mcp server owns ${name}` };
    try {
      return await server.call(name, args, timeoutMs);
    } catch (error) {
      return { ok: false, content: error instanceof Error ? error.message : String(error) };
    }
  }

  stop(): void { for (const server of this.servers.values()) server.stop(); }
}
