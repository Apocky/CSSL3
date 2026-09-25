import { invoke } from '@tauri-apps/api/core';
import type { ConsentDecision, LocalSession, Sampling, WorkSession } from './local-work.ts';

export interface Workspace {
  roots: { label: string; path: string; writable: boolean }[];
  tools: { name: string; risk: string; description: string }[];
}

export interface LocalBootstrap {
  online: boolean;
  host: { connection_id: string; service: string; endpoint: string; state_dir: string } | null;
  health: {
    status: string;
    service: string;
    engine: Record<string, unknown>;
    policy: Record<string, unknown>;
    mcp: Record<string, unknown>;
    presets: { id: string; label: string; profile: Sampling }[];
    sampling: Sampling;
    active_turns: number;
  } | null;
  workspace: Workspace | null;
  sessions: WorkSession[];
  notice: string;
}

export const EMPTY_BOOTSTRAP: LocalBootstrap = {
  online: false, host: null, health: null, workspace: null, sessions: [], notice: '',
};

type Command = 'local_bootstrap' | 'local_new_session' | 'local_open_session' | 'local_send' | 'local_cancel' | 'local_consent' | 'local_request';
type Invoke = <Result>(command: Command, args?: Record<string, unknown>) => Promise<Result>;

export function createLocalIpc(nativeInvoke: Invoke = invoke) {
  return {
    bootstrap: () => nativeInvoke<LocalBootstrap>('local_bootstrap'),
    newSession: (title: string) => nativeInvoke<LocalSession>('local_new_session', { title }),
    openSession: (sessionId: string) => nativeInvoke<LocalSession>('local_open_session', { sessionId }),
    send: (sessionId: string, prompt: string, preset?: string, sampling?: Sampling) => nativeInvoke<{ turn_id: string }>('local_send', {
      sessionId, options: { prompt, preset: preset ?? null, sampling: sampling ?? null },
    }),
    cancel: (sessionId: string) => nativeInvoke<{ cancelled: boolean }>('local_cancel', { sessionId }),
    consent: (sessionId: string, epoch: number, requestId: string, decision: ConsentDecision) => nativeInvoke<{ resolved: boolean }>('local_consent', {
      sessionId, epoch, requestId, decision,
    }),
    subscribe: (sessionId: string, epoch: number) => nativeInvoke<{ subscribed: boolean }>('local_request', {
      request: { operation: 'subscribe', session_id: sessionId, epoch },
    }),
    detach: (epoch: number) => nativeInvoke<{ detached: boolean }>('local_request', { request: { operation: 'detach', epoch } }),
    rename: (sessionId: string, title: string) => nativeInvoke<{ ok: boolean }>('local_request', {
      request: { operation: 'rename', session_id: sessionId, title },
    }),
  };
}

export const localIpc = createLocalIpc();
export type LocalIpc = ReturnType<typeof createLocalIpc>;

export async function attachTask(api: LocalIpc, snapshot: LocalSession, accept: (snapshot: LocalSession) => void): Promise<void> {
  accept(snapshot);
  await api.subscribe(snapshot.session.id, snapshot.epoch);
}