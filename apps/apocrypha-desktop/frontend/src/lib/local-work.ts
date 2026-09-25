export const WORK_EVENT = 'apocrypha://work-event';
export const WORK_STREAM_EVENT = 'apocrypha://work-stream';

export type Phase = 'queued' | 'thinking' | 'awaiting_consent' | 'tool' | 'writing' | 'done' | 'failed' | 'cancelled';
export type ConsentDecision = 'allow' | 'allow_session' | 'deny';
export type Connection = 'offline' | 'connecting' | 'connected' | 'reconnecting' | 'gap';
export type Sampling = Record<string, number | boolean | string | string[]>;

export interface ToolOutcome {
  id: string;
  name: string;
  ok: boolean;
  elapsedMs: number;
  summary: string;
  content: string;
  error?: string;
  denied?: boolean;
  diff?: { path: string; added: number; removed: number; patch: string };
}

export interface WorkSession {
  id: string;
  title: string;
  createdAt: string;
  lastActiveAt: string;
  standingGrants: string[];
}

export interface WorkTurn {
  id: string;
  sessionId: string;
  prompt: string;
  phase: Phase;
  startedAt: string;
  endedAt?: string;
  error?: string;
  toolCalls: ToolOutcome[];
  output: string;
  usage?: Record<string, number>;
}

export interface ConsentRequest {
  id: string;
  turnId: string;
  tool: string;
  risk: string;
  summary: string;
  detail: string;
}

export interface WorkEvent {
  seq: number;
  at: string;
  kind: 'session' | 'phase' | 'token' | 'tool_request' | 'tool_result' | 'consent_request' | 'consent_resolved' | 'error' | 'usage';
  data: Record<string, unknown>;
}

export interface WorkEnvelope { session_id: string; epoch: number; event: WorkEvent }
export interface StreamEnvelope { session_id: string; epoch: number; status: Connection; message: string }
export interface LocalSession { session: WorkSession; turns: WorkTurn[]; epoch: number; after_seq?: number }

export interface WorkState {
  session: WorkSession | null;
  epoch: number;
  lastSeq: number;
  turns: WorkTurn[];
  replayTurnId: string | null;
  replayingSaved: boolean;
  consent: ConsentRequest | null;
  pendingTool: { id: string; name: string; args: Record<string, unknown> } | null;
  connection: Connection;
  notice: string;
}

export const EMPTY_WORK: WorkState = {
  session: null, epoch: 0, lastSeq: 0, turns: [], replayTurnId: null, replayingSaved: false,
  consent: null, pendingTool: null, connection: 'offline', notice: '',
};

export type WorkAction =
  | { type: 'open'; snapshot: LocalSession }
  | { type: 'event'; envelope: WorkEnvelope }
  | { type: 'stream'; envelope: StreamEnvelope }
  | { type: 'notice'; message: string };

export function isActive(turn: WorkTurn | undefined): boolean {
  return !!turn && !['done', 'failed', 'cancelled'].includes(turn.phase);
}

function text(data: Record<string, unknown>, key: string): string {
  return typeof data[key] === 'string' ? data[key] as string : '';
}

export function reduceWork(state: WorkState, action: WorkAction): WorkState {
  if (action.type === 'notice') return { ...state, notice: action.message };
  if (action.type === 'open') {
    return {
      ...EMPTY_WORK, session: action.snapshot.session, turns: action.snapshot.turns,
      epoch: action.snapshot.epoch, lastSeq: action.snapshot.after_seq ?? 0, connection: 'connecting',
      replayTurnId: action.snapshot.turns.at(-1)?.id ?? null,
    };
  }
  const envelope = action.envelope;
  if (envelope.session_id !== state.session?.id || envelope.epoch !== state.epoch) return state;
  if (action.type === 'stream') return { ...state, connection: action.envelope.status, notice: action.envelope.message };
  const event = action.envelope.event;
  if (!Number.isSafeInteger(event.seq) || event.seq <= state.lastSeq) return state;
  if (event.seq !== state.lastSeq + 1) return { ...state, connection: 'gap', notice: 'An event is missing. Reload the task to recover its history.' };
  const data = event.data;
  let next: WorkState = { ...state, lastSeq: event.seq };
  const turnIndex = state.turns.findIndex((turn) => turn.id === state.replayTurnId);
  const last = state.turns[turnIndex];
  if (event.kind === 'session') {
    const id = text(data, 'turnId') || text(data, 'turn_id');
    if (!id) return { ...next, notice: 'The host sent a task event without a turn ID.' };
    const index = state.turns.findIndex((item) => item.id === id);
    const saved = state.turns[index];
    if (saved && !isActive(saved)) return { ...next, replayTurnId: id, replayingSaved: true, pendingTool: null, consent: null };
    const turn: WorkTurn = {
      id, sessionId: envelope.session_id, prompt: text(data, 'prompt'), phase: 'queued',
      startedAt: event.at, output: '', toolCalls: [],
    };
    const turns = [...state.turns];
    if (index < 0) turns.push(turn);
    else turns[index] = turn;
    return { ...next, turns, replayTurnId: id, replayingSaved: false, pendingTool: null, consent: null };
  }
  if (!last || state.replayingSaved) return next;
  let turn = last;
  switch (event.kind) {
    case 'token': turn = { ...last, output: last.output + text(data, 'delta') }; break;
    case 'phase': {
      const phase = text(data, 'phase') as Phase;
      if (['queued', 'thinking', 'awaiting_consent', 'tool', 'writing', 'done', 'failed', 'cancelled'].includes(phase)) {
        turn = {
          ...last, phase, ...(data.terminal === true ? { endedAt: event.at } : {}),
          ...(data.usage && typeof data.usage === 'object' ? { usage: data.usage as Record<string, number> } : {}),
        };
        if (!isActive(turn)) next = { ...next, consent: null, pendingTool: null };
      }
      break;
    }
    case 'tool_request':
      next.pendingTool = { id: text(data, 'id'), name: text(data, 'name'), args: (data.args as Record<string, unknown>) ?? {} };
      break;
    case 'tool_result': {
      const outcome = data as unknown as ToolOutcome;
      const tools = last.toolCalls.filter((item) => item.id !== outcome.id);
      turn = { ...last, toolCalls: [...tools, outcome] };
      next.pendingTool = null;
      break;
    }
    case 'consent_request':
      next.consent = {
        id: text(data, 'id'), turnId: text(data, 'turnId') || last.id, tool: text(data, 'tool'),
        risk: text(data, 'risk'), summary: text(data, 'summary'), detail: text(data, 'detail'),
      };
      turn = { ...last, phase: 'awaiting_consent' };
      break;
    case 'consent_resolved':
      if (text(data, 'id') === state.consent?.id) next.consent = null;
      break;
    case 'error':
      turn = { ...last, phase: 'failed', error: text(data, 'message') || 'The task failed.', endedAt: event.at };
      next.consent = null;
      next.pendingTool = null;
      break;
    case 'usage': turn = { ...last, usage: data as Record<string, number> }; break;
  }
  if (turn !== last) next.turns = state.turns.map((item, index) => index === turnIndex ? turn : item);
  return next;
}