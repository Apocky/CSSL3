import type {
  LazarusApproval,
  LazarusEvent,
  LazarusHealth,
  LazarusRun,
  LazarusTask,
  LazarusToolSpec,
} from './lazarus/types';

export type WorkbenchMode = 'ask' | 'plan' | 'queue';
export type PermissionMode = 'ask' | 'queue-only' | 'approval-gated';
export type InspectorTab = 'context' | 'tools' | 'approvals' | 'trace' | 'diagnostics';
export type ContextKind = 'task' | 'run' | 'tool' | 'url' | 'file' | 'resource' | 'text';

export interface ContextChip {
  id: string;
  kind: ContextKind;
  label: string;
  source: '#' | '@' | '+';
}

export interface WorkbenchActionCard {
  id: string;
  label: string;
  detail: string;
  target: InspectorTab | 'queue-task' | 'copy-prompt';
  risk: 'none' | 'low' | 'medium' | 'high';
  mutates: boolean;
}

export interface WorkbenchSnapshot {
  health: LazarusHealth | null;
  tasks: LazarusTask[];
  runs: LazarusRun[];
  approvals: LazarusApproval[];
  tools: LazarusToolSpec[];
  events: LazarusEvent[];
}

const RESOURCE_ALIASES: Record<string, string> = {
  lazarus: 'Lazarus',
  tessera: 'Tessera',
  akashic: 'Akashic',
  mneme: 'MNEME',
  deploy: 'Deploy',
  logs: 'Logs',
  terminal: 'Terminal',
};

function normalizeToken(raw: string): string {
  return raw.trim().replace(/[),.;]+$/g, '');
}

function chipKindForHashToken(token: string): ContextKind {
  const lower = token.toLowerCase();
  if (lower.startsWith('#task')) return 'task';
  if (lower.startsWith('#run')) return 'run';
  if (lower.startsWith('#tool')) return 'tool';
  if (lower.startsWith('#http') || lower.startsWith('#url')) return 'url';
  if (lower.includes('/') || lower.includes('\\') || lower.includes('.')) return 'file';
  return 'text';
}

export function extractContextChips(input: string): ContextChip[] {
  const seen = new Set<string>();
  const chips: ContextChip[] = [];
  const matches = input.matchAll(/(^|\s)([#@][A-Za-z0-9_./:\\-]+)/g);
  for (const match of matches) {
    const raw = normalizeToken(match[2] ?? '');
    if (!raw || seen.has(raw.toLowerCase())) continue;
    seen.add(raw.toLowerCase());
    if (raw.startsWith('#')) {
      chips.push({
        id: raw,
        kind: chipKindForHashToken(raw),
        label: raw,
        source: '#',
      });
    } else if (raw.startsWith('@')) {
      const name = raw.slice(1);
      const label = RESOURCE_ALIASES[name.toLowerCase()] ?? name;
      chips.push({
        id: raw,
        kind: 'resource',
        label: `@${label}`,
        source: '@',
      });
    }
  }
  return chips;
}

export function estimateContextLoad(messages: Array<{ text: string; pending?: boolean }>, input: string, attachmentTextChars: number): { chars: number; percent: number } {
  const messageChars = messages.filter((message) => !message.pending).reduce((sum, message) => sum + message.text.length, 0);
  const chars = messageChars + input.length + attachmentTextChars;
  const percent = Math.min(100, Math.round((chars / 120_000) * 100));
  return { chars, percent };
}

export function statusColor(status: string): string {
  if (['completed', 'online', 'approved', 'succeeded'].includes(status)) return '#34d399';
  if (['queued', 'leased', 'running', 'pending'].includes(status)) return '#7dd3fc';
  if (['blocked', 'waiting_approval'].includes(status)) return '#fbbf24';
  return '#f87171';
}

export function shortId(id: string): string {
  return id.length > 18 ? `${id.slice(0, 10)}...${id.slice(-6)}` : id;
}

export function isTesseraEvent(event: LazarusEvent): boolean {
  return event.kind.startsWith('tessera.') || event.kind.startsWith('lr.') || event.kind.startsWith('goal.');
}

export function workbenchSummary(snapshot: WorkbenchSnapshot): string {
  const queued = snapshot.health?.queued_count ?? snapshot.tasks.filter((task) => task.status === 'queued').length;
  const activeRuns = snapshot.health?.active_run_count ?? snapshot.runs.filter((run) => ['leased', 'running', 'blocked'].includes(run.status)).length;
  const approvals = snapshot.health?.pending_approval_count ?? snapshot.approvals.filter((approval) => approval.status === 'pending').length;
  const runners = snapshot.health?.online_runner_count ?? 0;
  return `${queued} queued · ${activeRuns} active · ${approvals} approvals · ${runners} runners`;
}

export function buildWorkbenchActionCards(snapshot: WorkbenchSnapshot, hasPrompt: boolean): WorkbenchActionCard[] {
  const pendingApprovals = snapshot.approvals.filter((approval) => approval.status === 'pending').length;
  const activeRuns = snapshot.runs.filter((run) => ['leased', 'running', 'blocked'].includes(run.status)).length;
  const tesseraEvents = snapshot.events.filter(isTesseraEvent).length;
  const cards: WorkbenchActionCard[] = [];

  if (hasPrompt) {
    cards.push({
      id: 'queue-task',
      label: 'Queue Lazarus task',
      detail: 'Creates a real queued task from the current prompt and attached context.',
      target: 'queue-task',
      risk: 'medium',
      mutates: true,
    });
  }
  if (pendingApprovals > 0) {
    cards.push({
      id: 'review-approvals',
      label: 'Review approvals',
      detail: `${pendingApprovals} gate${pendingApprovals === 1 ? '' : 's'} waiting for admin decision.`,
      target: 'approvals',
      risk: 'high',
      mutates: false,
    });
  }
  if (activeRuns > 0) {
    cards.push({
      id: 'open-trace',
      label: 'Open run trace',
      detail: `${activeRuns} run${activeRuns === 1 ? '' : 's'} currently leased, running, or blocked.`,
      target: 'trace',
      risk: 'none',
      mutates: false,
    });
  }
  if (snapshot.tools.length > 0) {
    cards.push({
      id: 'show-tools',
      label: 'Show tool catalog',
      detail: `${snapshot.tools.length} registered tools with approval-gate metadata.`,
      target: 'tools',
      risk: 'low',
      mutates: false,
    });
  }
  if (tesseraEvents > 0) {
    cards.push({
      id: 'open-tessera',
      label: 'Open Tessera trace',
      detail: `${tesseraEvents} Tessera bridge event${tesseraEvents === 1 ? '' : 's'} in recent Lazarus history.`,
      target: 'trace',
      risk: 'none',
      mutates: false,
    });
  }
  cards.push({
    id: 'open-diagnostics',
    label: 'Open diagnostics',
    detail: 'Inspect model, environment, persistence, and connector state without leaving chat.',
    target: 'diagnostics',
    risk: 'none',
    mutates: false,
  });

  return cards;
}

export function commandTarget(command: string): InspectorTab | 'queue-task' | 'compact' | 'new-chat' | null {
  const trimmed = command.trim().toLowerCase();
  if (trimmed.startsWith('/queue')) return 'queue-task';
  if (trimmed.startsWith('/tools')) return 'tools';
  if (trimmed.startsWith('/approvals')) return 'approvals';
  if (trimmed.startsWith('/trace') || trimmed.startsWith('/runs')) return 'trace';
  if (trimmed.startsWith('/diagnostics') || trimmed.startsWith('/debug')) return 'diagnostics';
  if (trimmed.startsWith('/context')) return 'context';
  if (trimmed.startsWith('/compact')) return 'compact';
  if (trimmed.startsWith('/new')) return 'new-chat';
  return null;
}