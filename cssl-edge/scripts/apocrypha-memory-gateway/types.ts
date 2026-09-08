export const ADAPTER_NAMES = [
  'mempalace',
  'brainmonsoon',
  'anamnesis',
  'graphify',
  'mneme',
  'metaharness',
] as const;

export type AdapterName = typeof ADAPTER_NAMES[number];

export interface SearchRequest {
  operation: 'search';
  read_only: true;
  query: string;
  limit: number;
  tenant_id: string;
  principal_id: string;
  capability: string;
  memory_manifest_hash?: string;
}

export interface GatewayRecord {
  id: string;
  text: string;
  metadata: Record<string, string | number | boolean | null>;
}

export type AdapterReadiness = 'ready' | 'configured' | 'unconfigured' | 'unavailable';

export interface AdapterProbe {
  state: AdapterReadiness;
  detail: string;
}

export interface ReadOnlyAdapter {
  readonly name: AdapterName;
  search(request: SearchRequest, signal: AbortSignal): Promise<unknown>;
  probe(signal: AbortSignal): Promise<AdapterProbe>;
}

export interface GatewayLimits {
  bodyBytes: number;
  queryBytes: number;
  responseBytes: number;
  recordChars: number;
  totalChars: number;
  maxRecords: number;
  timeoutMs: number;
}

export interface GatewayConfig {
  host: '127.0.0.1' | '::1';
  port: number;
  token: string;
  allowedTenants: ReadonlySet<string>;
  allowedPrincipals: ReadonlySet<string>;
  allowedCapabilities: ReadonlySet<string>;
  limits: GatewayLimits;
  native: {
    ownerId?: string;
    federatorExecutable?: string;
    federatorConfig?: string;
    mempalaceDb?: string;
    privacyPartition?: string;
    graphExecutable?: string;
    graphPath?: string;
    graphCsl?: string;
    graphNil?: string;
    graphCssl?: string;
  };
  upstreams: Partial<Record<AdapterName, { url: string; token?: string; healthUrl?: string }>>;
}

export interface GatewayRuntime {
  startedAt: string;
  requests: number;
  rejected: number;
  lastError: { code: string; adapter?: AdapterName; at: string } | null;
}
