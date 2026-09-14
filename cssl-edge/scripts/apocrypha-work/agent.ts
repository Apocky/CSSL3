import { randomUUID } from 'node:crypto';
import { EngineError, type EngineLike, type EngineMessage } from './engine';
import { FILE_TOOLS, runFileTool, type ToolContext, type ToolResult } from './tools/files';
import { SHELL_TOOLS, ShellDenied, runShellTool, screenCommand } from './tools/shell';
import { log } from './log';
import type {
  ConsentDecision,
  ConsentRequest,
  ToolCallOutcome,
  ToolCallRequest,
  ToolDefinition,
  WorkConfig,
  WorkEvent,
  WorkSession,
  WorkTurn,
} from './types';
import type { Workspace } from './workspace';

export interface PendingConsent {
  readonly request: ConsentRequest;
  resolve: (decision: ConsentDecision) => void;
}

function buildSystemPrompt(workspace: Workspace, config: WorkConfig): string {
  const roots = workspace.list()
    .map((root) => `  ${root.label} = ${root.path}${root.writable ? '' : '  (read-only)'}`)
    .join('\n');
  return [
    'You are Apocrypha in Work mode: a software engineering agent running entirely on the operator\'s own machine.',
    '',
    'Workspace roots you may touch (nothing outside them exists for you):',
    roots,
    '',
    'How to work:',
    '- Read before you write. Use list_dir and search to find the real file rather than guessing a path.',
    '- Make the smallest correct change. Prefer edit_file over write_file on files that already exist.',
    '- Match the surrounding code: its naming, its idiom, its comment density. Do not add narrating comments.',
    '- After changing code, run the project\'s own checks with run_command when they exist. Report failures honestly;',
    '  never claim something passed that you did not observe pass.',
    '- If a task is blocked, say exactly what blocks it. Do not leave a silent TODO or a half-measure.',
    '- When you are done, summarise what changed, file by file, and what you verified.',
    '',
    'Consent:',
    `- Reads run immediately. ${config.autoApprove.includes('write') ? '' : 'Writes and '}commands are shown to the operator for approval before they run.`,
    '- A denied call is a real answer, not an error. Adapt your plan; do not retry the same call in a loop.',
    '',
    'You have a hard budget of ' + String(config.maxToolIterations) + ' tool steps for this turn. Spend them deliberately.',
  ].join('\n');
}

function summariseForConsent(call: ToolCallRequest): { summary: string; detail: string } {
  if (call.name === 'run_command') {
    const command = String(call.args.command ?? '');
    return { summary: `Run: ${command.slice(0, 120)}`, detail: command };
  }
  if (call.name === 'write_file') {
    const content = String(call.args.content ?? '');
    return {
      summary: `Write ${String(call.args.path ?? '?')} (${Buffer.byteLength(content, 'utf8')} bytes)`,
      detail: content.slice(0, 4_000),
    };
  }
  if (call.name === 'edit_file') {
    return {
      summary: `Edit ${String(call.args.path ?? '?')}`,
      detail: `- ${String(call.args.old_text ?? '').slice(0, 1_500)}\n+ ${String(call.args.new_text ?? '').slice(0, 1_500)}`,
    };
  }
  return { summary: `${call.name}`, detail: JSON.stringify(call.args).slice(0, 2_000) };
}

export class WorkAgent {
  readonly tools: readonly ToolDefinition[];
  private readonly config: WorkConfig;
  private readonly workspace: Workspace;
  private readonly engine: EngineLike;

  constructor(config: WorkConfig, workspace: Workspace, engine: EngineLike) {
    this.config = config;
    this.workspace = workspace;
    this.engine = engine;
    this.tools = [...FILE_TOOLS, ...(config.shellAllowed ? SHELL_TOOLS : [])];
  }

  private riskOf(name: string): ToolDefinition['risk'] {
    return this.tools.find((tool) => tool.name === name)?.risk ?? 'execute';
  }

  private async execute(call: ToolCallRequest, ctx: ToolContext): Promise<ToolResult> {
    if (call.name === 'run_command') {
      return await runShellTool(call.name, call.args, ctx, {
        allowed: this.config.shellAllowed,
        denyPatterns: this.config.shellDenyPatterns,
        timeoutMs: this.config.toolTimeoutMs,
      });
    }
    return await runFileTool(call.name, call.args, ctx);
  }

  /**
   * Run one turn to completion, emitting events as it goes.
   *
   * `requestConsent` returns the operator's decision for a single call. It is given the whole
   * request so the UI can show the exact command or the exact diff — approving "edit_file" in the
   * abstract is not consent to any particular edit.
   */
  async run(
    session: WorkSession,
    turn: WorkTurn,
    history: readonly EngineMessage[],
    emit: (event: Omit<WorkEvent, 'seq' | 'at'>) => void,
    requestConsent: (request: ConsentRequest) => Promise<ConsentDecision>,
    signal: AbortSignal,
  ): Promise<void> {
    const started = Date.now();
    const messages: EngineMessage[] = [
      { role: 'system', content: buildSystemPrompt(this.workspace, this.config) },
      ...history,
      { role: 'user', content: turn.prompt },
    ];
    const ctx: ToolContext = { workspace: this.workspace, signal };

    for (let iteration = 0; iteration < this.config.maxToolIterations; iteration += 1) {
      if (signal.aborted) { turn.phase = 'cancelled'; emit({ kind: 'phase', data: { phase: 'cancelled' } }); return; }

      turn.phase = 'thinking';
      emit({ kind: 'phase', data: { phase: 'thinking', iteration } });

      const reply = await this.engine.complete(messages, this.tools, (delta) => {
        turn.output += delta;
        emit({ kind: 'token', data: { delta } });
      }, signal);

      if (reply.usage.totalTokens !== undefined) {
        turn.usage = { ...reply.usage, elapsedS: (Date.now() - started) / 1_000 };
        emit({ kind: 'usage', data: { ...turn.usage } });
      }

      if (reply.toolCalls.length === 0) {
        // No tool calls AND nothing said is not a finished task, it is a broken turn — usually a
        // template or engine misconfiguration (thinking left on, or tool support not compiled in).
        // Reporting it as success is how a silent failure becomes a believed answer.
        if (!reply.content.trim()) {
          turn.phase = 'failed';
          turn.error = reply.finishReason === 'length'
            ? 'The model spent its whole output budget without producing an answer. Check that thinking is disabled and that the engine was started with --jinja.'
            : `The model returned nothing (finish_reason: ${reply.finishReason}).`;
          turn.endedAt = new Date().toISOString();
          log('error', 'work.turn.empty_completion', {
            finish_reason: reply.finishReason,
            completion_tokens: reply.usage.completionTokens,
            iteration,
          });
          emit({ kind: 'error', data: { message: turn.error, code: 'EMPTY_COMPLETION' } });
          return;
        }
        turn.phase = 'done';
        turn.endedAt = new Date().toISOString();
        emit({ kind: 'phase', data: { phase: 'done', iterations: iteration + 1 } });
        return;
      }

      messages.push({
        role: 'assistant',
        content: reply.content,
        tool_calls: reply.toolCalls.map((call) => ({
          id: call.id,
          type: 'function' as const,
          function: { name: call.name, arguments: JSON.stringify(call.args) },
        })),
      });

      for (const call of reply.toolCalls) {
        if (signal.aborted) { turn.phase = 'cancelled'; emit({ kind: 'phase', data: { phase: 'cancelled' } }); return; }
        const outcome = await this.settle(session, turn, call, ctx, emit, requestConsent);
        turn.toolCalls.push(outcome);
        messages.push({
          role: 'tool',
          tool_call_id: call.id,
          content: outcome.ok
            ? outcome.content.slice(0, 24_000)
            : `ERROR: ${outcome.error ?? 'tool failed'}`,
        });
      }
    }

    turn.phase = 'failed';
    turn.error = `reached the ${this.config.maxToolIterations}-step budget without finishing`;
    turn.endedAt = new Date().toISOString();
    emit({ kind: 'error', data: { message: turn.error, code: 'ITERATION_BUDGET' } });
  }

  private async settle(
    session: WorkSession,
    turn: WorkTurn,
    call: ToolCallRequest,
    ctx: ToolContext,
    emit: (event: Omit<WorkEvent, 'seq' | 'at'>) => void,
    requestConsent: (request: ConsentRequest) => Promise<ConsentDecision>,
  ): Promise<ToolCallOutcome> {
    const risk = this.riskOf(call.name);
    const { summary, detail } = summariseForConsent(call);
    emit({ kind: 'tool_request', data: { id: call.id, name: call.name, risk, summary, args: call.args } });

    const preapproved = this.config.autoApprove.includes(risk) || session.standingGrants.includes(call.name);
    if (!preapproved) {
      // Screen before asking. A command that can never be approved should not be put in front of
      // the operator as though clicking "allow" were an option.
      if (call.name === 'run_command') {
        try {
          screenCommand(String(call.args.command ?? ''), {
            allowed: this.config.shellAllowed,
            denyPatterns: this.config.shellDenyPatterns,
            timeoutMs: this.config.toolTimeoutMs,
          });
        } catch (error) {
          if (error instanceof ShellDenied) {
            const outcome: ToolCallOutcome = {
              id: call.id, name: call.name, ok: false, elapsedMs: 0, denied: true,
              summary: `blocked: ${summary}`, content: '', error: error.message,
            };
            emit({ kind: 'tool_result', data: { ...outcome } });
            return outcome;
          }
          throw error;
        }
      }

      turn.phase = 'awaiting_consent';
      const request: ConsentRequest = {
        id: randomUUID(), turnId: turn.id, tool: call.name, risk, summary, detail,
        createdAt: new Date().toISOString(),
      };
      emit({ kind: 'consent_request', data: { ...request } });
      const decision = await requestConsent(request);
      emit({ kind: 'consent_resolved', data: { id: request.id, decision } });
      if (decision === 'allow_session' && !session.standingGrants.includes(call.name)) {
        session.standingGrants.push(call.name);
      }
      if (decision === 'deny') {
        const outcome: ToolCallOutcome = {
          id: call.id, name: call.name, ok: false, elapsedMs: 0, denied: true,
          summary: `declined: ${summary}`, content: '',
          error: 'The operator declined this action. Do not retry it; choose a different approach or ask what they want instead.',
        };
        emit({ kind: 'tool_result', data: { ...outcome } });
        return outcome;
      }
    }

    turn.phase = 'tool';
    emit({ kind: 'phase', data: { phase: 'tool', tool: call.name } });
    const started = Date.now();
    try {
      const result = await this.execute(call, ctx);
      const outcome: ToolCallOutcome = {
        id: call.id, name: call.name, ok: true, elapsedMs: Date.now() - started,
        summary: result.summary, content: result.content, diff: result.diff,
      };
      emit({ kind: 'tool_result', data: { ...outcome } });
      return outcome;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      log('warn', 'work.tool.failed', { tool: call.name, error: message });
      const outcome: ToolCallOutcome = {
        id: call.id, name: call.name, ok: false, elapsedMs: Date.now() - started,
        summary: `failed: ${summary}`, content: '',
        error: error instanceof EngineError ? `${error.code}: ${message}` : message,
      };
      emit({ kind: 'tool_result', data: { ...outcome } });
      return outcome;
    }
  }
}
