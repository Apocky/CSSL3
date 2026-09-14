import type { ConsentPrompt } from '@/components/work/ConsentCard';
import type { RiskTier, ToolCallOutcome, WorkEvent } from './client';

export interface LiveState {
  readonly phase: string;
  readonly prompt: string | null;
  readonly answer: string;
  readonly steps: readonly ToolCallOutcome[];
  readonly pendingStep: ToolCallOutcome | null;
  readonly consent: ConsentPrompt | null;
  readonly error: string | null;
  readonly usage: { totalTokens?: number; elapsedS?: number } | null;
  readonly terminal: boolean;
  readonly lastSeq: number;
}

export const IDLE: LiveState = {
  phase: 'idle',
  prompt: null,
  answer: '',
  steps: [],
  pendingStep: null,
  consent: null,
  error: null,
  usage: null,
  terminal: true,
  lastSeq: 0,
};

function text(data: Record<string, unknown>, key: string): string {
  const value = data[key];
  return typeof value === 'string' ? value : '';
}

/**
 * Fold one server event into the live view.
 *
 * Events are replayed from the channel buffer whenever the stream reconnects, so this has to be
 * idempotent on sequence number: anything at or below `lastSeq` has already been applied.
 */
export function reduceLive(state: LiveState, event: WorkEvent): LiveState {
  if (event.seq <= state.lastSeq) return state;
  const seq = event.seq;
  const data = event.data;

  switch (event.kind) {
    case 'session':
      return { ...IDLE, lastSeq: seq, prompt: text(data, 'prompt'), phase: 'queued', terminal: false };

    case 'token':
      return { ...state, lastSeq: seq, answer: state.answer + text(data, 'delta') };

    case 'phase': {
      const phase = text(data, 'phase');
      const terminal = data.terminal === true;
      return {
        ...state,
        lastSeq: seq,
        phase,
        terminal: terminal || state.terminal,
        usage: (data.usage as LiveState['usage']) ?? state.usage,
      };
    }

    case 'tool_request':
      return {
        ...state,
        lastSeq: seq,
        pendingStep: {
          id: text(data, 'id'),
          name: text(data, 'name'),
          ok: true,
          elapsedMs: 0,
          summary: text(data, 'summary'),
          content: '',
        },
      };

    case 'tool_result': {
      const outcome = data as unknown as ToolCallOutcome;
      return { ...state, lastSeq: seq, pendingStep: null, steps: [...state.steps, outcome] };
    }

    case 'consent_request':
      return {
        ...state,
        lastSeq: seq,
        phase: 'awaiting_consent',
        consent: {
          id: text(data, 'id'),
          tool: text(data, 'tool'),
          risk: (text(data, 'risk') || 'execute') as RiskTier,
          summary: text(data, 'summary'),
          detail: text(data, 'detail'),
        },
      };

    case 'consent_resolved':
      return { ...state, lastSeq: seq, consent: null };

    case 'usage':
      return { ...state, lastSeq: seq, usage: data as LiveState['usage'] };

    case 'error':
      return { ...state, lastSeq: seq, error: text(data, 'message') || 'The task failed.', terminal: true };

    default:
      return { ...state, lastSeq: seq };
  }
}

export function phaseLabel(state: LiveState): string {
  if (state.error) return 'failed';
  if (state.terminal) return state.phase === 'idle' ? 'ready' : state.phase;
  switch (state.phase) {
    case 'queued': return 'starting';
    case 'thinking': return 'thinking';
    case 'tool': return 'running a step';
    case 'awaiting_consent': return 'waiting on you';
    default: return state.phase;
  }
}
