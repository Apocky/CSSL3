import type { NextPage } from 'next';
import { useEffect, useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';

import AdminLayout from '../../components/AdminLayout';
import {
  buildWorkbenchActionCards,
  commandTarget,
  estimateContextLoad,
  extractContextChips,
  isTesseraEvent,
  shortId,
  statusColor,
  workbenchSummary,
  type ContextChip,
  type InspectorTab,
  type PermissionMode,
  type WorkbenchActionCard,
  type WorkbenchMode,
  type WorkbenchSnapshot,
} from '../../lib/admin-workbench';
import { authFetch } from '../../lib/browser-auth';
import type {
  JsonRecord,
  LazarusApproval,
  LazarusEvent,
  LazarusFleetConfig,
  LazarusHealth,
  LazarusModelMode,
  LazarusRun,
  LazarusRunner,
  LazarusTask,
  LazarusToolSpec,
} from '../../lib/lazarus/types';

type Role = 'you' | 'assistant' | 'system';
type LoadState = 'loading' | 'ready' | 'error';

interface Msg {
  id: string;
  role: Role;
  text: string;
  at: number;
  pending?: boolean;
  model?: string;
}

interface Conversation {
  id: string;
  title: string;
  updatedAt: number;
  messages: Msg[];
}

interface AttachmentDraft {
  id: string;
  name: string;
  size: number;
  type: string;
  text?: string;
}

interface BridgeStatus {
  online: boolean;
  model_ready?: boolean;
  provider?: 'deepseek' | 'anthropic' | 'none';
  model?: string | null;
}

interface WorkbenchData {
  state: LoadState;
  health: LazarusHealth | null;
  tasks: LazarusTask[];
  runs: LazarusRun[];
  runners: LazarusRunner[];
  approvals: LazarusApproval[];
  tools: LazarusToolSpec[];
  fleet: LazarusFleetConfig[];
  events: LazarusEvent[];
  stub: boolean;
  lastUpdated: string | null;
}

interface ConfirmAction {
  title: string;
  body: string;
  confirmLabel: string;
  tone: 'neutral' | 'warn' | 'danger';
  run: () => Promise<void>;
}

const STORAGE_KEY = 'apocky-admin-chat-conversations-v2';
const LEGACY_STORAGE_KEY = 'apocky-admin-chat-conversations-v1';
const ROLE_LABEL: Record<Role, string> = { you: 'You', assistant: 'Assistant', system: 'System' };
const ROLE_COLOR: Record<Role, string> = { you: '#ffffff', assistant: '#7dd3fc', system: '#f87171' };
const DEFAULT_REPO_PATH = 'C:\\Users\\Apocky\\source\\repos\\CSSLv3';

const COMMANDS: Array<{ command: string; label: string; detail: string }> = [
  { command: '/queue', label: 'Queue Lazarus task', detail: 'Create a real queued task from this prompt.' },
  { command: '/tools', label: 'Show tools', detail: 'Open connector and tool catalog.' },
  { command: '/approvals', label: 'Review approvals', detail: 'Open approval gates.' },
  { command: '/trace', label: 'Open run trace', detail: 'Inspect recent runs and events.' },
  { command: '/diagnostics', label: 'Diagnostics', detail: 'Inspect bridge/model/persistence state.' },
  { command: '/compact', label: 'Compact local context', detail: 'Keep recent turns and insert a deterministic marker.' },
];

const RESOURCES = ['@Lazarus', '@Tessera', '@Akashic', '@MNEME', '@Deploy', '@Logs'];
const CONTEXTS = ['#task', '#run', '#tool', '#url', '#file', '#production'];

function id(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function blankConversation(): Conversation {
  return { id: id('chat'), title: 'New session', updatedAt: Date.now(), messages: [] };
}

function safeStoredConversations(value: string | null): Conversation[] {
  if (!value) return [blankConversation()];
  try {
    const parsed = JSON.parse(value) as unknown;
    if (!Array.isArray(parsed)) return [blankConversation()];
    const conversations = parsed.filter((item): item is Conversation => {
      if (typeof item !== 'object' || item === null) return false;
      const record = item as Record<string, unknown>;
      return typeof record.id === 'string' && typeof record.title === 'string' && typeof record.updatedAt === 'number' && Array.isArray(record.messages);
    });
    return conversations.length > 0 ? conversations : [blankConversation()];
  } catch {
    return [blankConversation()];
  }
}

function toApiMessages(messages: Msg[]): Array<{ role: 'user' | 'assistant' | 'system'; content: string }> {
  return messages
    .filter((message) => !message.pending && message.text.trim())
    .map((message) => ({
      role: message.role === 'you' ? 'user' : message.role,
      content: message.text,
    }));
}

function titleFrom(text: string): string {
  const firstLine = text.replace(/\s+/g, ' ').trim();
  if (!firstLine) return 'New session';
  return firstLine.length > 48 ? `${firstLine.slice(0, 45)}...` : firstLine;
}

function fileSummary(file: AttachmentDraft): string {
  const kb = Math.max(1, Math.round(file.size / 1024));
  return `${file.name} (${kb} KB${file.type ? `, ${file.type}` : ''})`;
}

function formatStatus(status: BridgeStatus | null): string {
  if (!status) return 'checking';
  if (!status.model_ready) return 'model offline';
  return `${status.provider ?? 'model'} · ${status.model ?? 'ready'}`;
}

function attachmentChips(attachments: AttachmentDraft[]): ContextChip[] {
  return attachments.map((file) => ({ id: file.id, kind: 'file', label: `#${file.name}`, source: '+' }));
}

function buildUserText(input: string, attachments: AttachmentDraft[], chips: ContextChip[], mode: WorkbenchMode, permission: PermissionMode, summary: string): string {
  const base = input.trim();
  const contextLines = [
    `Mode: ${mode}`,
    `Permission: ${permission}`,
    `Lazarus: ${summary}`,
    chips.length > 0 ? `Context: ${chips.map((chip) => chip.label).join(', ')}` : 'Context: none selected',
  ];
  const renderedAttachments = attachments.map((file) => {
    if (file.text) return `File: ${fileSummary(file)}\n\n${file.text}`;
    return `File: ${fileSummary(file)}\nBinary or unreadable content was attached; use filename and metadata only.`;
  }).join('\n\n---\n\n');
  const attachmentBlock = renderedAttachments ? `\n\nAttached context:\n\n${renderedAttachments}` : '';
  return `Workbench context:\n${contextLines.join('\n')}\n\n${base || 'Review the attached context and recommend the next action.'}${attachmentBlock}`;
}

function initialWorkbenchData(): WorkbenchData {
  return {
    state: 'loading',
    health: null,
    tasks: [],
    runs: [],
    runners: [],
    approvals: [],
    tools: [],
    fleet: [],
    events: [],
    stub: true,
    lastUpdated: null,
  };
}

const Chat: NextPage = () => {
  const [conversations, setConversations] = useState<Conversation[]>([blankConversation()]);
  const [activeId, setActiveId] = useState<string>('');
  const [search, setSearch] = useState('');
  const [input, setInput] = useState('');
  const [attachments, setAttachments] = useState<AttachmentDraft[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [queueing, setQueueing] = useState(false);
  const [status, setStatus] = useState<BridgeStatus | null>(null);
  const [adminAuthorized, setAdminAuthorized] = useState(false);
  const [workbench, setWorkbench] = useState<WorkbenchData>(initialWorkbenchData());
  const [mode, setMode] = useState<WorkbenchMode>('ask');
  const [permissionMode, setPermissionMode] = useState<PermissionMode>('approval-gated');
  const [modelMode, setModelMode] = useState<LazarusModelMode>('deepseek-v4-pro');
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>('context');
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [confirmAction, setConfirmAction] = useState<ConfirmAction | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const abortRef = useRef<AbortController | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const loaded = safeStoredConversations(localStorage.getItem(STORAGE_KEY) ?? localStorage.getItem(LEGACY_STORAGE_KEY));
    setConversations(loaded);
    setActiveId(loaded[0]?.id ?? '');
  }, []);

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(conversations.slice(0, 40)));
  }, [conversations]);

  useEffect(() => {
    if (!adminAuthorized) return undefined;
    const fetchStatus = () => {
      authFetch('/api/admin/bridge?action=status', { cache: 'no-store' })
        .then((response) => response.json())
        .then((json: BridgeStatus) => setStatus(json))
        .catch(() => setStatus({ online: false, model_ready: false, provider: 'none', model: null }));
    };
    fetchStatus();
    const timer = setInterval(fetchStatus, 10_000);
    return () => clearInterval(timer);
  }, [adminAuthorized]);

  async function loadWorkbench(): Promise<void> {
    try {
      const [health, tasks, runs, runners, approvals, tools, fleet, events] = await Promise.all([
        authFetch('/api/admin/lazarus/health', { cache: 'no-store' }).then((response) => response.json()),
        authFetch('/api/admin/lazarus/tasks', { cache: 'no-store' }).then((response) => response.json()),
        authFetch('/api/admin/lazarus/runs', { cache: 'no-store' }).then((response) => response.json()),
        authFetch('/api/admin/lazarus/runners', { cache: 'no-store' }).then((response) => response.json()),
        authFetch('/api/admin/lazarus/approvals', { cache: 'no-store' }).then((response) => response.json()),
        authFetch('/api/admin/lazarus/tools', { cache: 'no-store' }).then((response) => response.json()),
        authFetch('/api/admin/lazarus/fleet', { cache: 'no-store' }).then((response) => response.json()),
        authFetch('/api/admin/lazarus/events', { cache: 'no-store' }).then((response) => response.json()),
      ]);
      setWorkbench({
        state: 'ready',
        health: health.ok ? health : null,
        tasks: tasks.tasks ?? [],
        runs: runs.runs ?? [],
        runners: runners.runners ?? [],
        approvals: approvals.approvals ?? [],
        tools: tools.tools ?? [],
        fleet: fleet.fleet ?? [],
        events: events.events ?? [],
        stub: Boolean(health.stub || tasks.stub || runs.stub || runners.stub || approvals.stub || tools.stub || fleet.stub || events.stub),
        lastUpdated: new Date().toLocaleTimeString(),
      });
    } catch (err) {
      setWorkbench((prev) => ({ ...prev, state: 'error' }));
      setNotice(`workbench load failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  useEffect(() => {
    if (!adminAuthorized) return undefined;
    void loadWorkbench();
    const timer = setInterval(() => void loadWorkbench(), 10_000);
    return () => clearInterval(timer);
  }, [adminAuthorized]);

  const activeConversation = conversations.find((conversation) => conversation.id === activeId) ?? conversations[0] ?? blankConversation();
  const messages = activeConversation.messages;
  const filteredConversations = useMemo(() => {
    const query = search.trim().toLowerCase();
    return [...conversations]
      .sort((a, b) => b.updatedAt - a.updatedAt)
      .filter((conversation) => !query || conversation.title.toLowerCase().includes(query));
  }, [conversations, search]);
  const lastUserMessage = useMemo(() => {
    for (let index = messages.length - 1; index >= 0; index -= 1) {
      const message = messages[index];
      if (message?.role === 'you') return message;
    }
    return null;
  }, [messages]);
  const lastUserIndex = useMemo(() => {
    for (let index = messages.length - 1; index >= 0; index -= 1) {
      if (messages[index]?.role === 'you') return index;
    }
    return -1;
  }, [messages]);

  const typedContextChips = useMemo(() => extractContextChips(input), [input]);
  const attachedContextChips = useMemo(() => attachmentChips(attachments), [attachments]);
  const allContextChips = useMemo(() => [...typedContextChips, ...attachedContextChips], [attachedContextChips, typedContextChips]);
  const attachmentTextChars = useMemo(() => attachments.reduce((sum, file) => sum + (file.text?.length ?? 0), 0), [attachments]);
  const contextLoad = useMemo(() => estimateContextLoad(messages, input, attachmentTextChars), [attachmentTextChars, input, messages]);
  const snapshot: WorkbenchSnapshot = useMemo(() => ({
    health: workbench.health,
    tasks: workbench.tasks,
    runs: workbench.runs,
    approvals: workbench.approvals,
    tools: workbench.tools,
    events: workbench.events,
  }), [workbench.approvals, workbench.events, workbench.health, workbench.runs, workbench.tasks, workbench.tools]);
  const summary = useMemo(() => workbenchSummary(snapshot), [snapshot]);
  const pendingApprovals = useMemo(() => workbench.approvals.filter((approval) => approval.status === 'pending'), [workbench.approvals]);
  const activeTasks = useMemo(() => workbench.tasks.filter((task) => ['queued', 'leased', 'running', 'blocked'].includes(task.status)).slice(0, 8), [workbench.tasks]);
  const activeRuns = useMemo(() => workbench.runs.filter((run) => ['leased', 'running', 'blocked'].includes(run.status)).slice(0, 8), [workbench.runs]);
  const tesseraEvents = useMemo(() => workbench.events.filter(isTesseraEvent), [workbench.events]);
  const toolGroups = useMemo(() => {
    const groups = new Map<string, number>();
    for (const tool of workbench.tools) groups.set(tool.group, (groups.get(tool.group) ?? 0) + 1);
    return Array.from(groups.entries()).sort((a, b) => a[0].localeCompare(b[0]));
  }, [workbench.tools]);
  const actionCards = useMemo(
    () => buildWorkbenchActionCards(snapshot, Boolean(input.trim() || lastUserMessage?.text)),
    [input, lastUserMessage?.text, snapshot],
  );

  useEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
  }, [messages]);

  function updateActiveMessages(nextMessages: Msg[]): void {
    setConversations((prev) => prev.map((conversation) => {
      if (conversation.id !== activeConversation.id) return conversation;
      const firstUser = nextMessages.find((message) => message.role === 'you');
      return {
        ...conversation,
        title: firstUser ? titleFrom(firstUser.text) : conversation.title,
        updatedAt: Date.now(),
        messages: nextMessages,
      };
    }));
  }

  function startNewChat(): void {
    const conversation = blankConversation();
    setConversations((prev) => [conversation, ...prev]);
    setActiveId(conversation.id);
    setInput('');
    setAttachments([]);
    setNotice(null);
  }

  function deleteConversation(conversationId: string): void {
    setConversations((prev) => {
      const remaining = prev.filter((conversation) => conversation.id !== conversationId);
      const next = remaining.length > 0 ? remaining : [blankConversation()];
      if (conversationId === activeId) setActiveId(next[0]?.id ?? '');
      return next;
    });
  }

  async function requestAssistant(conversationMessages: Msg[], pendingId: string): Promise<void> {
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setSubmitting(true);
    try {
      const lastUser = conversationMessages.filter((message) => message.role === 'you').at(-1);
      const response = await authFetch('/api/admin/bridge?action=send', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        signal: controller.signal,
        body: JSON.stringify({
          text: lastUser?.text ?? '',
          messages: toApiMessages(conversationMessages),
        }),
      });
      const json = await response.json() as { response?: unknown; model?: unknown; error?: unknown };
      if (!response.ok) throw new Error(typeof json.error === 'string' ? json.error : `chat request failed (${response.status})`);
      const text = typeof json.response === 'string' ? json.response : 'The model returned no text.';
      updateActiveMessages(conversationMessages.map((message) =>
        message.id === pendingId
          ? { ...message, role: 'assistant', text, pending: false, model: typeof json.model === 'string' ? json.model : undefined }
          : message,
      ));
    } catch (err) {
      const text = err instanceof DOMException && err.name === 'AbortError'
        ? 'Request cancelled.'
        : err instanceof Error
          ? err.message
          : String(err);
      updateActiveMessages(conversationMessages.map((message) =>
        message.id === pendingId ? { ...message, role: 'system', text, pending: false } : message,
      ));
    } finally {
      if (abortRef.current === controller) abortRef.current = null;
      setSubmitting(false);
    }
  }

  async function queuePromptAsTask(promptText: string): Promise<void> {
    const cleanPrompt = promptText.trim();
    if (!cleanPrompt || queueing) return;
    setQueueing(true);
    setNotice(null);
    try {
      const chips = extractContextChips(cleanPrompt);
      const metadata: JsonRecord = {
        source: 'admin-chat-workbench',
        conversation_id: activeConversation.id,
        mode,
        permission_mode: permissionMode,
        context_labels: chips.map((chip) => chip.label),
      };
      const response = await authFetch('/api/admin/lazarus/tasks', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          title: titleFrom(cleanPrompt),
          prompt: cleanPrompt,
          repo_path: DEFAULT_REPO_PATH,
          model_mode: modelMode,
          cost_ceiling_usd: 2,
          sensorium_enabled: true,
          playtest_enabled: true,
          metadata,
        }),
      });
      const json = await response.json() as { task?: LazarusTask; error?: string };
      if (!response.ok || !json.task) throw new Error(json.error ?? 'task create failed');
      setNotice(`queued ${json.task.id}`);
      setInput('');
      await loadWorkbench();
      setInspectorTab('trace');
    } catch (err) {
      setNotice(err instanceof Error ? err.message : String(err));
    } finally {
      setQueueing(false);
    }
  }

  function queueWithConfirmation(promptText: string): void {
    const cleanPrompt = promptText.trim();
    if (!cleanPrompt) return;
    setConfirmAction({
      title: 'Queue Lazarus task',
      body: `Create a queued task in ${DEFAULT_REPO_PATH} using ${modelMode}. This does not execute browser tools directly; it creates a real Lazarus work item.`,
      confirmLabel: 'Queue task',
      tone: 'warn',
      run: () => queuePromptAsTask(cleanPrompt),
    });
  }

  function compactConversation(): void {
    if (messages.length <= 8) {
      setNotice('conversation is already compact');
      return;
    }
    const compacted = messages.slice(0, -6).filter((message) => !message.pending);
    const kept = messages.slice(-6);
    const note: Msg = {
      id: id('compact'),
      role: 'system',
      at: Date.now(),
      text: `Local compaction marker: ${compacted.length} older messages were removed from visible context. Last durable topic: ${titleFrom(compacted.at(-1)?.text ?? activeConversation.title)}.`,
    };
    updateActiveMessages([note, ...kept]);
    setNotice('local context compacted');
  }

  async function handleCommand(): Promise<boolean> {
    const target = commandTarget(input);
    if (!target) return false;
    if (target === 'queue-task') {
      queueWithConfirmation(input.replace(/^\/queue\s*/i, '') || lastUserMessage?.text || '');
      return true;
    }
    if (target === 'compact') {
      compactConversation();
      setInput('');
      return true;
    }
    if (target === 'new-chat') {
      startNewChat();
      return true;
    }
    setInspectorTab(target);
    setPaletteOpen(false);
    setInput('');
    return true;
  }

  async function submitCurrent(): Promise<void> {
    if (submitting || queueing) return;
    if (await handleCommand()) return;
    const basePrompt = input.trim();
    if (mode === 'queue') {
      queueWithConfirmation(basePrompt);
      return;
    }
    const text = buildUserText(basePrompt, attachments, allContextChips, mode, permissionMode, summary);
    const now = Date.now();
    const cleanMessages = messages.filter((message) => !message.pending);
    const yourMsg: Msg = { id: id('user'), role: 'you', text, at: now };
    const pendingId = id('assistant');
    const pendingResponse: Msg = { id: pendingId, role: 'assistant', text: 'Thinking...', at: now, pending: true };
    const conversationMessages = [...cleanMessages, yourMsg, pendingResponse];
    updateActiveMessages(conversationMessages);
    setInput('');
    setAttachments([]);
    await requestAssistant(conversationMessages, pendingId);
  }

  async function send(event: React.FormEvent): Promise<void> {
    event.preventDefault();
    await submitCurrent();
  }

  async function regenerate(): Promise<void> {
    if (submitting || lastUserIndex < 0) return;
    const conversationMessages = messages.slice(0, lastUserIndex + 1).filter((message) => !message.pending);
    const pendingId = id('regen');
    const pendingResponse: Msg = { id: pendingId, role: 'assistant', text: 'Thinking...', at: Date.now(), pending: true };
    const nextMessages = [...conversationMessages, pendingResponse];
    updateActiveMessages(nextMessages);
    await requestAssistant(nextMessages, pendingId);
  }

  async function attachFiles(files: FileList | null): Promise<void> {
    if (!files) return;
    const next = await Promise.all(Array.from(files).slice(0, 6).map(async (file): Promise<AttachmentDraft> => {
      const base = { id: id('file'), name: file.name, size: file.size, type: file.type };
      if (file.type.startsWith('text/') || /\.(md|txt|json|ts|tsx|js|jsx|py|rs|csl|css|html)$/i.test(file.name)) {
        return { ...base, text: (await file.text()).slice(0, 32_000) };
      }
      return base;
    }));
    setAttachments((prev) => [...prev, ...next].slice(-8));
    if (fileRef.current) fileRef.current.value = '';
  }

  async function copyText(text: string): Promise<void> {
    await navigator.clipboard.writeText(text);
    setNotice('copied');
  }

  async function copyConversation(): Promise<void> {
    const transcript = messages
      .filter((message) => !message.pending)
      .map((message) => `${ROLE_LABEL[message.role]}: ${message.text}`)
      .join('\n\n');
    await navigator.clipboard.writeText(transcript);
    setNotice('conversation copied');
  }

  function insertToken(token: string): void {
    setInput((prev) => `${prev}${prev && !prev.endsWith(' ') ? ' ' : ''}${token} `);
  }

  function runActionCard(card: WorkbenchActionCard): void {
    if (card.target === 'queue-task') {
      queueWithConfirmation(input.trim() || lastUserMessage?.text || '');
      return;
    }
    if (card.target === 'copy-prompt') {
      void copyText(input.trim() || lastUserMessage?.text || '');
      return;
    }
    setInspectorTab(card.target);
  }

  function decideApproval(approval: LazarusApproval, decision: 'approved' | 'denied'): void {
    setConfirmAction({
      title: `${decision === 'approved' ? 'Approve' : 'Deny'} ${approval.gate}`,
      body: `${approval.reason}\n\nRun: ${approval.run_id}`,
      confirmLabel: decision === 'approved' ? 'Approve gate' : 'Deny gate',
      tone: decision === 'approved' ? 'warn' : 'danger',
      run: async () => {
        const response = await authFetch('/api/admin/lazarus/approvals', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ action: 'decide', approval_id: approval.id, decision, decided_by: 'admin-workbench' }),
        });
        const json = await response.json() as { error?: string };
        if (!response.ok) throw new Error(json.error ?? 'approval decision failed');
        setNotice(`${decision} ${approval.gate}`);
        await loadWorkbench();
      },
    });
  }

  async function runConfirmAction(): Promise<void> {
    const action = confirmAction;
    if (!action) return;
    setConfirmAction(null);
    try {
      await action.run();
    } catch (err) {
      setNotice(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <AdminLayout title="Workbench" onAdminCheck={(check) => setAdminAuthorized(check.authorized)}>
      <style jsx>{`
        .workbench-shell { display: grid; grid-template-columns: minmax(220px, 280px) minmax(0, 1fr) minmax(300px, 380px); gap: 0.75rem; min-height: calc(100dvh - 112px); }
        .panel { border: 1px solid #1f1f2a; border-radius: 8px; background: rgba(10, 10, 16, 0.46); min-width: 0; }
        .left-rail { display: grid; grid-template-rows: auto auto minmax(0, 1fr); padding: 0.65rem; gap: 0.6rem; }
        .conversation { display: grid; grid-template-rows: auto minmax(0, 1fr) auto; min-height: calc(100dvh - 112px); }
        .top-bar { display: flex; align-items: center; justify-content: space-between; gap: 0.55rem; flex-wrap: wrap; padding: 0.55rem 0.65rem; border-bottom: 1px solid #1f1f2a; }
        .top-bar-group { display: flex; gap: 0.4rem; align-items: center; flex-wrap: wrap; min-width: 0; }
        .select, .field { height: 32px; border: 1px solid #2a2a3a; border-radius: 6px; background: rgba(20, 20, 30, 0.78); color: #dbe7f3; font: inherit; font-size: 0.76rem; padding: 0 0.5rem; outline: none; }
        .chip { display: inline-flex; align-items: center; gap: 0.25rem; min-height: 24px; border: 1px solid #2a2a3a; border-radius: 999px; padding: 0 0.45rem; color: #cdd6e4; font-size: 0.68rem; white-space: nowrap; }
        .icon-btn { width: 34px; height: 34px; border: 1px solid #2a2a3a; border-radius: 6px; background: rgba(20, 20, 30, 0.75); color: #dbe7f3; cursor: pointer; }
        .text-btn { min-height: 32px; border: 1px solid #2a2a3a; border-radius: 6px; background: rgba(20, 20, 30, 0.75); color: #dbe7f3; cursor: pointer; padding: 0 0.65rem; font: inherit; font-size: 0.75rem; }
        .text-btn:disabled, .icon-btn:disabled { opacity: 0.45; cursor: not-allowed; }
        .send-btn { min-height: 38px; min-width: 42px; border: 0; border-radius: 6px; background: #7dd3fc; color: #050507; cursor: pointer; font-weight: 800; }
        .send-btn:disabled { opacity: 0.45; cursor: not-allowed; }
        .session-row { width: 100%; border: 1px solid transparent; border-radius: 6px; background: transparent; color: #cdd6e4; padding: 0.55rem; text-align: left; cursor: pointer; font: inherit; min-width: 0; }
        .session-row.active { border-color: rgba(124, 211, 252, 0.32); background: rgba(124, 211, 252, 0.1); color: #7dd3fc; }
        .inspector-tabs { display: grid; grid-template-columns: repeat(5, 1fr); border-bottom: 1px solid #1f1f2a; }
        .tab { min-height: 38px; border: 0; border-right: 1px solid #1f1f2a; background: transparent; color: #7a7a8c; cursor: pointer; font: inherit; font-size: 0.68rem; }
        .tab.active { color: #7dd3fc; background: rgba(124, 211, 252, 0.08); }
        .inspector-body { padding: 0.7rem; overflow: auto; max-height: calc(100dvh - 150px); }
        .list-card { border-top: 1px solid #1f1f2a; padding: 0.55rem 0; }
        .message-card { margin: 0 auto 1.25rem; max-width: 860px; }
        .message-body { color: #dbe7f3; white-space: pre-wrap; word-break: break-word; line-height: 1.62; }
        .composer { max-width: 860px; width: 100%; margin: 0 auto; padding: 0.65rem 0.75rem 0.75rem; }
        .composer-box { border: 1px solid #2a2a3a; border-radius: 10px; background: rgba(20, 20, 30, 0.82); padding: 0.5rem; }
        .palette { border: 1px solid #2a2a3a; border-radius: 8px; background: rgba(10, 10, 16, 0.96); padding: 0.55rem; margin-bottom: 0.45rem; box-shadow: 0 18px 40px rgba(0,0,0,0.35); }
        .palette-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 0.45rem; }
        .palette-item { border: 1px solid #1f1f2a; border-radius: 6px; background: rgba(20, 20, 30, 0.6); color: #dbe7f3; min-height: 54px; cursor: pointer; text-align: left; padding: 0.45rem; font: inherit; }
        .action-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(170px, 1fr)); gap: 0.45rem; margin-top: 0.65rem; }
        .action-card { border: 1px solid #2a2a3a; border-radius: 8px; background: rgba(20, 20, 30, 0.62); color: #dbe7f3; text-align: left; padding: 0.6rem; cursor: pointer; font: inherit; }
        .confirm-backdrop { position: fixed; inset: 0; background: rgba(0,0,0,0.58); display: grid; place-items: center; z-index: 40; padding: 1rem; }
        .confirm-dialog { width: min(460px, 100%); border: 1px solid #2a2a3a; border-radius: 8px; background: #101018; padding: 1rem; box-shadow: 0 24px 70px rgba(0,0,0,0.5); }
        @media (max-width: 1180px) { .workbench-shell { grid-template-columns: minmax(0, 1fr); } .left-rail, .inspector-body { max-height: 360px; } .conversation { min-height: 720px; } }
        @media (max-width: 720px) { .palette-grid { grid-template-columns: minmax(0, 1fr); } .top-bar { align-items: stretch; } .top-bar-group { width: 100%; } .select, .field { flex: 1; min-width: 0; } .inspector-tabs { grid-template-columns: repeat(3, 1fr); } }
      `}</style>

      <div className="workbench-shell">
        <aside className="panel left-rail" aria-label="Sessions and queued work">
          <div style={{ display: 'flex', gap: '0.45rem' }}>
            <button type="button" className="text-btn" onClick={startNewChat} style={{ flex: 1 }}>+ session</button>
            <button type="button" className="icon-btn" onClick={() => void loadWorkbench()} title="Refresh workbench state" aria-label="Refresh workbench state">R</button>
          </div>
          <input className="field" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search sessions" style={{ width: '100%' }} />
          <div style={{ overflow: 'auto', display: 'grid', gap: '0.35rem', alignContent: 'start' }}>
            {filteredConversations.map((conversation) => (
              <div key={conversation.id} style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1fr) auto', gap: '0.25rem', alignItems: 'center' }}>
                <button type="button" onClick={() => setActiveId(conversation.id)} className={`session-row ${conversation.id === activeConversation.id ? 'active' : ''}`}>
                  <span style={{ display: 'block', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{conversation.title}</span>
                  <span style={{ color: '#5a5a6a', fontSize: '0.66rem' }}>{new Date(conversation.updatedAt).toLocaleDateString()}</span>
                </button>
                <button type="button" className="icon-btn" onClick={() => deleteConversation(conversation.id)} aria-label="Delete session" title="Delete session">x</button>
              </div>
            ))}
            <div style={{ marginTop: '0.4rem', borderTop: '1px solid #1f1f2a', paddingTop: '0.55rem' }}>
              <div style={{ color: '#7a7a8c', fontSize: '0.68rem', textTransform: 'uppercase', letterSpacing: '0.12em', marginBottom: '0.35rem' }}>Active work</div>
              {activeTasks.length === 0 && <div style={{ color: '#5a5a6a', fontSize: '0.78rem' }}>No queued work</div>}
              {activeTasks.slice(0, 5).map((task) => (
                <button key={task.id} type="button" className="session-row" onClick={() => setInspectorTab('trace')}>
                  <span style={{ color: statusColor(task.status), fontSize: '0.72rem' }}>{task.status}</span>
                  <span style={{ display: 'block', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{task.title}</span>
                </button>
              ))}
            </div>
          </div>
        </aside>

        <section className="panel conversation" aria-label="Chat workbench">
          <div className="top-bar">
            <div className="top-bar-group">
              <span className="chip">repo · CSSLv3</span>
              <span className="chip">source · Lazarus APIs</span>
              <span className="chip" style={{ color: status?.model_ready ? '#34d399' : '#f87171' }}>{formatStatus(status)}</span>
              <span className="chip">context {contextLoad.percent}% · {contextLoad.chars.toLocaleString()} chars</span>
            </div>
            <div className="top-bar-group">
              <select className="select" value={mode} onChange={(event) => setMode(event.target.value as WorkbenchMode)} aria-label="Workbench mode">
                <option value="ask">Ask</option>
                <option value="plan">Plan</option>
                <option value="queue">Queue</option>
              </select>
              <select className="select" value={permissionMode} onChange={(event) => setPermissionMode(event.target.value as PermissionMode)} aria-label="Permission mode">
                <option value="ask">Ask first</option>
                <option value="queue-only">Queue only</option>
                <option value="approval-gated">Approval gated</option>
              </select>
              <select className="select" value={modelMode} onChange={(event) => setModelMode(event.target.value as LazarusModelMode)} aria-label="Model route">
                <option value="deepseek-v4-pro">deepseek-v4-pro</option>
                <option value="deepseek-v4-flash">deepseek-v4-flash</option>
                <option value="reviewer">reviewer</option>
              </select>
            </div>
          </div>

          {notice && (
            <div style={{ margin: '0.65rem auto 0', maxWidth: 860, width: 'calc(100% - 1.5rem)', border: '1px solid #2a2a3a', borderRadius: 6, padding: '0.5rem 0.65rem', color: notice.includes('failed') || notice.includes('error') ? '#f87171' : '#7dd3fc', fontSize: '0.78rem' }}>
              {notice}
            </div>
          )}

          <div ref={scrollRef} style={{ overflow: 'auto', padding: messages.length === 0 ? '8vh 0.75rem 1rem' : '1rem 0.75rem' }}>
            {messages.length === 0 ? (
              <div style={{ maxWidth: 820, margin: '0 auto' }}>
                <h2 style={{ margin: '0 0 0.8rem', fontSize: '1.55rem', color: '#e6e6f0' }}>Welcome back, Apocky</h2>
                <div style={{ display: 'grid', gap: '0.45rem' }}>
                  {[
                    'Summarize @Lazarus #task state and name the next highest-leverage move.',
                    'Plan a safe work slice for the admin workbench without fake tool execution.',
                    'Review #production deployment risk before pushing the next change.',
                    '/queue Implement the next verified admin workbench task with tests and deploy evidence.',
                  ].map((suggestion) => (
                    <button key={suggestion} type="button" className="action-card" onClick={() => setInput(suggestion)}>
                      {suggestion}
                    </button>
                  ))}
                </div>
              </div>
            ) : (
              <div>
                {messages.map((message, index) => (
                  <article key={message.id} className="message-card">
                    <div style={{ display: 'flex', alignItems: 'center', gap: '0.45rem', marginBottom: 5, flexWrap: 'wrap' }}>
                      <strong style={{ fontSize: '0.74rem', color: ROLE_COLOR[message.role] }}>{ROLE_LABEL[message.role]}</strong>
                      {message.model && <span style={{ color: '#5a5a6a', fontSize: '0.68rem' }}>{message.model}</span>}
                      {!message.pending && <button type="button" className="text-btn" onClick={() => void copyText(message.text)}>copy</button>}
                    </div>
                    <div className="message-body" style={{ color: message.role === 'system' ? '#f87171' : '#dbe7f3' }}>{message.text}</div>
                    {message.role === 'assistant' && !message.pending && index === messages.length - 1 && (
                      <div className="action-grid">
                        {actionCards.map((card) => (
                          <button key={card.id} type="button" className="action-card" onClick={() => runActionCard(card)}>
                            <div style={{ display: 'flex', justifyContent: 'space-between', gap: '0.5rem' }}>
                              <strong>{card.label}</strong>
                              <span style={{ color: card.risk === 'high' ? '#f87171' : card.risk === 'medium' ? '#fbbf24' : '#7a7a8c', fontSize: '0.68rem' }}>{card.risk}</span>
                            </div>
                            <div style={{ color: '#7a7a8c', fontSize: '0.72rem', marginTop: 4 }}>{card.detail}</div>
                          </button>
                        ))}
                      </div>
                    )}
                  </article>
                ))}
              </div>
            )}
          </div>

          <form onSubmit={send} className="composer">
            {paletteOpen && (
              <div className="palette">
                <div className="palette-grid">
                  {COMMANDS.map((item) => (
                    <button key={item.command} type="button" className="palette-item" onClick={() => { setInput((prev) => `${item.command}${prev.trim() && !prev.trim().startsWith('/') ? ` ${prev.trim()}` : ''}`); setPaletteOpen(false); }}>
                      <strong>{item.label}</strong>
                      <div style={{ color: '#7a7a8c', fontSize: '0.7rem', marginTop: 3 }}>{item.detail}</div>
                    </button>
                  ))}
                </div>
                <div style={{ display: 'flex', gap: '0.35rem', flexWrap: 'wrap', marginTop: '0.55rem' }}>
                  {[...CONTEXTS, ...RESOURCES].map((token) => (
                    <button key={token} type="button" className="text-btn" onClick={() => insertToken(token)}>{token}</button>
                  ))}
                </div>
              </div>
            )}
            {allContextChips.length > 0 && (
              <div style={{ display: 'flex', gap: '0.35rem', flexWrap: 'wrap', marginBottom: '0.45rem' }}>
                {allContextChips.map((chip) => (
                  <span key={chip.id} className="chip">
                    {chip.label}
                    {chip.source === '+' && <button type="button" onClick={() => setAttachments((prev) => prev.filter((file) => file.id !== chip.id))} style={{ border: 0, background: 'transparent', color: '#7a7a8c', cursor: 'pointer' }} aria-label={`Remove ${chip.label}`}>x</button>}
                  </span>
                ))}
              </div>
            )}
            <div className="composer-box">
              <input ref={fileRef} type="file" multiple style={{ display: 'none' }} onChange={(event) => void attachFiles(event.target.files)} />
              <div style={{ display: 'grid', gridTemplateColumns: 'auto auto minmax(0, 1fr) auto', alignItems: 'end', gap: '0.45rem' }}>
                <button type="button" className="icon-btn" onClick={() => setPaletteOpen((value) => !value)} title="Open commands and connectors" aria-label="Open commands and connectors">+</button>
                <button type="button" className="icon-btn" onClick={() => fileRef.current?.click()} title="Attach files" aria-label="Attach files">#</button>
                <textarea
                  value={input}
                  onChange={(event) => setInput(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' && !event.shiftKey) {
                      event.preventDefault();
                      void submitCurrent();
                    }
                    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
                      event.preventDefault();
                      setPaletteOpen((value) => !value);
                    }
                  }}
                  placeholder="Describe a task or ask a question"
                  disabled={submitting || queueing}
                  rows={3}
                  style={{ width: '100%', border: 'none', background: 'transparent', color: '#e6e6f0', outline: 'none', resize: 'vertical', minHeight: 58, maxHeight: 220, fontFamily: 'inherit', fontSize: '0.92rem' }}
                />
                {submitting ? (
                  <button type="button" className="send-btn" onClick={() => abortRef.current?.abort()} title="Stop request" aria-label="Stop request">x</button>
                ) : (
                  <button type="submit" className="send-btn" disabled={queueing || (!input.trim() && attachments.length === 0)} title={mode === 'queue' ? 'Queue task' : 'Send'} aria-label={mode === 'queue' ? 'Queue task' : 'Send'}>{mode === 'queue' ? 'Q' : 'Send'}</button>
                )}
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between', gap: '0.5rem', flexWrap: 'wrap', marginTop: '0.45rem', color: '#5a5a6a', fontSize: '0.68rem' }}>
                <span>{permissionMode} · {mode} · tools {permissionMode === 'queue-only' ? 'queue only' : 'approval gated'}</span>
                <span>{workbench.lastUpdated ? `state ${workbench.lastUpdated}` : workbench.state}</span>
              </div>
            </div>
          </form>
        </section>

        <aside className="panel" aria-label="Workbench inspector">
          <div className="inspector-tabs">
            {(['context', 'tools', 'approvals', 'trace', 'diagnostics'] as InspectorTab[]).map((tab) => (
              <button key={tab} type="button" className={`tab ${inspectorTab === tab ? 'active' : ''}`} onClick={() => setInspectorTab(tab)}>
                {tab}
              </button>
            ))}
          </div>
          <div className="inspector-body">
            {inspectorTab === 'context' && (
              <InspectorSection title="Context">
                <MetricLine label="Lazarus" value={summary} />
                <MetricLine label="Mode" value={`${mode} · ${permissionMode}`} />
                <MetricLine label="Model" value={modelMode} />
                <MetricLine label="Load" value={`${contextLoad.percent}% · ${contextLoad.chars.toLocaleString()} chars`} />
                <div style={{ marginTop: '0.65rem', display: 'flex', gap: '0.35rem', flexWrap: 'wrap' }}>
                  {allContextChips.length === 0 ? <span style={{ color: '#7a7a8c', fontSize: '0.78rem' }}>No context chips yet</span> : allContextChips.map((chip) => <span key={chip.id} className="chip">{chip.label}</span>)}
                </div>
              </InspectorSection>
            )}

            {inspectorTab === 'tools' && (
              <InspectorSection title="Connectors">
                <Connector label="Lazarus" state={workbench.state === 'ready' ? 'online' : workbench.state} detail={`${workbench.tools.length} tools · ${pendingApprovals.length} approvals`} />
                <Connector label="Tessera" state={tesseraEvents.length > 0 ? 'trace' : 'idle'} detail={`${tesseraEvents.length} bridge events`} />
                <Connector label="Akashic diagnostics" state="docked" detail="Inspector only; no floating overlay." />
                <Connector label="Deploy" state="available" detail="Use Lazarus task queue until deploy tool execution is wired." />
                <div style={{ marginTop: '0.7rem', display: 'flex', gap: '0.35rem', flexWrap: 'wrap' }}>
                  {toolGroups.map(([group, count]) => <span key={group} className="chip">{group}:{count}</span>)}
                </div>
                {workbench.tools.slice(0, 10).map((tool) => (
                  <div key={tool.name} className="list-card">
                    <div style={{ color: tool.approval_gate ? '#fbbf24' : '#7dd3fc', fontSize: '0.78rem' }}>{tool.name}</div>
                    <div style={{ color: '#7a7a8c', fontSize: '0.72rem' }}>{tool.description}</div>
                    {tool.approval_gate && <span className="chip" style={{ marginTop: 5, color: '#fbbf24' }}>{tool.approval_gate}</span>}
                  </div>
                ))}
              </InspectorSection>
            )}

            {inspectorTab === 'approvals' && (
              <InspectorSection title="Approvals">
                {pendingApprovals.length === 0 && <p style={{ color: '#7a7a8c', fontSize: '0.82rem' }}>No pending gates</p>}
                {pendingApprovals.map((approval) => (
                  <div key={approval.id} className="list-card">
                    <code style={{ color: '#fbbf24' }}>{approval.gate}</code>
                    <div style={{ color: '#cdd6e4', fontSize: '0.78rem', marginTop: 4 }}>{approval.reason}</div>
                    <div style={{ color: '#7a7a8c', fontSize: '0.7rem', marginTop: 3 }}>{shortId(approval.run_id)}</div>
                    <div style={{ display: 'flex', gap: '0.4rem', marginTop: '0.5rem' }}>
                      <button type="button" className="text-btn" onClick={() => decideApproval(approval, 'approved')} style={{ flex: 1 }}>approve</button>
                      <button type="button" className="text-btn" onClick={() => decideApproval(approval, 'denied')} style={{ flex: 1, color: '#f87171' }}>deny</button>
                    </div>
                  </div>
                ))}
              </InspectorSection>
            )}

            {inspectorTab === 'trace' && (
              <InspectorSection title="Run Trace">
                {activeRuns.length === 0 && <p style={{ color: '#7a7a8c', fontSize: '0.82rem' }}>No active runs</p>}
                {activeRuns.map((run) => (
                  <div key={run.id} className="list-card">
                    <div style={{ display: 'flex', justifyContent: 'space-between', gap: '0.5rem' }}>
                      <code style={{ color: '#7dd3fc' }}>{shortId(run.id)}</code>
                      <span style={{ color: statusColor(run.status), fontSize: '0.72rem' }}>{run.status}</span>
                    </div>
                    <div style={{ color: '#7a7a8c', fontSize: '0.72rem' }}>{run.model_mode} · ${run.cost_usd_estimate.toFixed(4)}</div>
                    {workbench.events.filter((event) => event.run_id === run.id).slice(0, 4).map((event) => (
                      <div key={event.id} style={{ marginTop: '0.4rem', color: event.level === 'error' ? '#f87171' : '#cdd6e4', fontSize: '0.72rem' }}>
                        <code>{event.kind}</code>
                        <div style={{ color: '#7a7a8c' }}>{event.message}</div>
                      </div>
                    ))}
                  </div>
                ))}
                {tesseraEvents.slice(0, 5).map((event) => (
                  <div key={`tessera-${event.id}`} className="list-card">
                    <code style={{ color: '#c084fc' }}>{event.kind}</code>
                    <div style={{ color: '#7a7a8c', fontSize: '0.72rem' }}>{event.message}</div>
                  </div>
                ))}
              </InspectorSection>
            )}

            {inspectorTab === 'diagnostics' && (
              <InspectorSection title="Diagnostics">
                <MetricLine label="Bridge" value={formatStatus(status)} />
                <MetricLine label="Persistence" value={workbench.stub ? 'in-memory' : 'Supabase'} />
                <MetricLine label="Runners" value={`${workbench.runners.filter((runner) => runner.status === 'online').length} online`} />
                <MetricLine label="Fleet" value={workbench.fleet[0] ? `${workbench.fleet[0].privacy_class} · $${workbench.fleet[0].max_cost_usd_per_run}` : 'none'} />
                <MetricLine label="Execution" value="chat queues work; runner events prove execution" />
                <div style={{ marginTop: '0.65rem', display: 'flex', gap: '0.4rem', flexWrap: 'wrap' }}>
                  <button type="button" className="text-btn" onClick={() => void regenerate()} disabled={submitting || lastUserIndex < 0}>regenerate</button>
                  <button type="button" className="text-btn" onClick={() => void copyConversation()} disabled={messages.length === 0}>copy session</button>
                  <button type="button" className="text-btn" onClick={compactConversation} disabled={messages.length <= 8}>compact</button>
                </div>
              </InspectorSection>
            )}
          </div>
        </aside>
      </div>

      {confirmAction && (
        <div className="confirm-backdrop" role="presentation">
          <div className="confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="confirm-title">
            <h2 id="confirm-title" style={{ margin: 0, color: confirmAction.tone === 'danger' ? '#f87171' : confirmAction.tone === 'warn' ? '#fbbf24' : '#e6e6f0', fontSize: '1rem' }}>{confirmAction.title}</h2>
            <p style={{ color: '#cdd6e4', whiteSpace: 'pre-wrap', fontSize: '0.82rem', lineHeight: 1.55 }}>{confirmAction.body}</p>
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '0.5rem' }}>
              <button type="button" className="text-btn" onClick={() => setConfirmAction(null)}>cancel</button>
              <button type="button" className="text-btn" onClick={() => void runConfirmAction()} style={{ color: confirmAction.tone === 'danger' ? '#f87171' : '#fbbf24' }}>{confirmAction.confirmLabel}</button>
            </div>
          </div>
        </div>
      )}
    </AdminLayout>
  );
};

function InspectorSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section>
      <h2 style={{ margin: '0 0 0.65rem', color: '#e6e6f0', fontSize: '0.95rem' }}>{title}</h2>
      {children}
    </section>
  );
}

function MetricLine({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ display: 'grid', gridTemplateColumns: '92px minmax(0, 1fr)', gap: '0.55rem', padding: '0.35rem 0', borderBottom: '1px solid rgba(31, 31, 42, 0.7)' }}>
      <span style={{ color: '#7a7a8c', fontSize: '0.68rem', textTransform: 'uppercase', letterSpacing: '0.1em' }}>{label}</span>
      <span style={{ color: '#dbe7f3', fontSize: '0.76rem', overflowWrap: 'anywhere' }}>{value}</span>
    </div>
  );
}

function Connector({ label, state, detail }: { label: string; state: string; detail: string }) {
  return (
    <div className="list-card">
      <div style={{ display: 'flex', justifyContent: 'space-between', gap: '0.5rem' }}>
        <strong style={{ color: '#dbe7f3', fontSize: '0.8rem' }}>{label}</strong>
        <span style={{ color: state === 'online' || state === 'available' || state === 'docked' ? '#34d399' : '#7dd3fc', fontSize: '0.7rem' }}>{state}</span>
      </div>
      <div style={{ color: '#7a7a8c', fontSize: '0.72rem', marginTop: 3 }}>{detail}</div>
    </div>
  );
}

export function _testExportsAreFunctions(): boolean {
  return typeof Chat === 'function';
}

export default Chat;