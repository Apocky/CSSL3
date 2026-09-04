import type { MemoryType, Role } from '../mneme/types';

export type BrainConnectorState = 'live' | 'degraded' | 'retired';

export interface BrainMemory {
  readonly id: string;
  readonly type: MemoryType;
  readonly csl: string;
  readonly paraphrase: string;
  readonly topic_key: string | null;
  readonly search_queries: readonly string[];
  readonly source_msg_ids: readonly string[];
  readonly created_at: string;
}

export interface BrainMessage {
  readonly id: string;
  readonly session_id: string;
  readonly role: Role;
  readonly content: string;
  readonly ts: string;
  readonly source_only: boolean;
}

export interface BrainSnapshot {
  readonly schema_version: 'apocky.owner-brain.snapshot.v1';
  readonly status: 'live';
  readonly connectors: {
    readonly mneme_storage: BrainConnectorState;
    readonly source_projection: BrainConnectorState;
    readonly local_apocv4: BrainConnectorState;
  };
  readonly memories: readonly BrainMemory[];
  readonly messages: readonly BrainMessage[];
  readonly counts: {
    readonly memories: number;
    readonly messages: number;
    readonly source_links: number;
  };
  readonly limits: {
    readonly memories: number;
    readonly recent_messages: number;
    readonly source_messages: number;
  };
  readonly served_by: string;
  readonly ts: string;
}

export interface BrainRuntimeStatus {
  readonly schema_version: 'apocky.owner-brain.runtime-status.v1';
  readonly status: 'live' | 'degraded';
  readonly reason_code: string | null;
  readonly observed_at: string;
  readonly latency_ms: number | null;
  readonly upstream_status: number | null;
  readonly served_by: string;
  readonly ts: string;
}

export interface BrainRuntimeTurn {
  readonly schema_version: 'apocky.owner-brain.turn.v1';
  readonly status: 'completed';
  readonly text: string;
  readonly session_id: string;
  readonly request_id: string;
  readonly model_id: string;
  readonly response_digest: string;
  readonly memory: {
    readonly status: string;
    readonly records_used: number;
    readonly refs: readonly unknown[];
  } | null;
  readonly served_by: string;
  readonly ts: string;
}
