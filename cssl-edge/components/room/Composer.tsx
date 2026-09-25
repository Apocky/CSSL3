// The composer, laid out the way the leading chat products do it (researched 2026-09-25: ChatGPT,
// Claude, Gemini, Copilot): a + inside the input pill on the left opens Camera / Photos / Files,
// then tools; a model picker sits beside a round send button; picked tools and files show as
// removable chips above the text; Enter sends and Shift+Enter breaks the line (on touch screens
// Enter breaks the line and only the button sends). Locked options stay visible with a lock and
// say why, rather than disappearing.

import { useEffect, useRef, useState, type ChangeEvent, type KeyboardEvent } from 'react';

import {
  CameraIcon, ChevronIcon, CloseIcon, FileIcon, GlobeIcon, ImageGenIcon, LockIcon, PhotoIcon, PlusIcon, SendIcon,
} from './Icons';
import Tip from './Tip';
import type { EngineLane, PendingAttachment, RoomName, RoomTool } from './types';
import styles from './Room.module.css';

const TOOL_LABEL: Record<RoomTool, string> = { image: 'Create image', web: 'Web search' };

export default function Composer({
  room, signedIn, premium, premiumReady, lane, onLane, tools, onTools, attachments, onAttach, onRemoveAttachment, onSend, error, busy,
}: {
  readonly room: RoomName;
  readonly signedIn: boolean;
  readonly premium: boolean;
  readonly premiumReady: boolean;
  readonly lane: EngineLane;
  readonly onLane: (lane: EngineLane) => void;
  readonly tools: readonly RoomTool[];
  readonly onTools: (tools: RoomTool[]) => void;
  readonly attachments: readonly PendingAttachment[];
  readonly onAttach: (files: File[]) => void;
  readonly onRemoveAttachment: (key: string) => void;
  readonly onSend: (body: string) => Promise<boolean>;
  readonly error: string | null;
  readonly busy: boolean;
}): JSX.Element {
  const [text, setText] = useState('');
  const [sending, setSending] = useState(false);
  const [menu, setMenu] = useState<'none' | 'plus' | 'model'>('none');
  const ref = useRef<HTMLTextAreaElement | null>(null);
  const wrap = useRef<HTMLDivElement | null>(null);
  const camera = useRef<HTMLInputElement | null>(null);
  const photos = useRef<HTMLInputElement | null>(null);
  const files = useRef<HTMLInputElement | null>(null);

  // Grow with the text up to about six lines, then scroll inside the box.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, 168)}px`;
  }, [text]);

  useEffect(() => {
    if (menu === 'none') return undefined;
    const close = (event: MouseEvent | TouchEvent) => {
      if (wrap.current && !wrap.current.contains(event.target as Node)) setMenu('none');
    };
    const esc = (event: globalThis.KeyboardEvent) => { if (event.key === 'Escape') setMenu('none'); };
    document.addEventListener('mousedown', close);
    document.addEventListener('touchstart', close);
    document.addEventListener('keydown', esc);
    return () => {
      document.removeEventListener('mousedown', close);
      document.removeEventListener('touchstart', close);
      document.removeEventListener('keydown', esc);
    };
  }, [menu]);

  const uploading = attachments.some((a) => a.state === 'uploading');
  const canSend = text.trim() !== '' && !sending && !uploading;

  const submit = async () => {
    const body = text.trim();
    if (!body || sending || uploading) return;
    setSending(true);
    const ok = await onSend(body);
    setSending(false);
    if (ok) setText('');
    ref.current?.focus();
  };

  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    const touch = typeof window !== 'undefined' && window.matchMedia('(pointer: coarse)').matches;
    if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing && !touch) {
      e.preventDefault();
      void submit();
    }
  };

  const picked = (event: ChangeEvent<HTMLInputElement>) => {
    const list = Array.from(event.target.files ?? []);
    event.target.value = '';
    setMenu('none');
    if (list.length > 0) onAttach(list);
  };

  const toggleTool = (tool: RoomTool) => {
    onTools(tools.includes(tool) ? tools.filter((t) => t !== tool) : [...tools, tool]);
    setMenu('none');
  };

  const attachHint = signedIn ? null : 'Sign in to attach files';
  const premiumHint = !premium
    ? (signedIn ? 'Apocrypha+ is part of the Premium plan' : 'Sign in with a Premium plan to use Apocrypha+')
    : !premiumReady ? 'Apocrypha+ is not connected right now' : null;
  const toolHint = premiumHint ?? (lane !== 'flagship' ? 'Tools run on Apocrypha+: switch the model to Apocrypha+' : null);

  const modelName = lane === 'flagship' ? 'Apocrypha+' : 'Local';

  return <div className={styles.composer} ref={wrap}>
    <input ref={camera} type="file" accept="image/*" capture="environment" hidden onChange={picked} />
    <input ref={photos} type="file" accept="image/*" multiple hidden onChange={picked} />
    <input ref={files} type="file" multiple hidden onChange={picked} />

    {menu === 'plus' ? <div className={styles.menu} role="menu" aria-label="Add to your message">
      <MenuItem icon={<CameraIcon />} label="Camera" hint={attachHint ?? 'Take a photo and attach it'} disabled={!signedIn} onClick={() => camera.current?.click()} />
      <MenuItem icon={<PhotoIcon />} label="Photos" hint={attachHint ?? 'Attach photos from your library'} disabled={!signedIn} onClick={() => photos.current?.click()} />
      <MenuItem icon={<FileIcon />} label="Files" hint={attachHint ?? 'Attach files: Apocrypha reads text files; other files arrive by name'} disabled={!signedIn} onClick={() => files.current?.click()} />
      <div className={styles.menuRule} role="separator" />
      <MenuItem icon={<ImageGenIcon />} label={TOOL_LABEL.image} hint={toolHint ?? 'Ask Apocrypha to make an image with this message'} locked={toolHint !== null} checked={tools.includes('image')} onClick={() => toggleTool('image')} />
      <MenuItem icon={<GlobeIcon />} label={TOOL_LABEL.web} hint={toolHint ?? 'Let Apocrypha search the web for this message'} locked={toolHint !== null} checked={tools.includes('web')} onClick={() => toggleTool('web')} />
    </div> : null}

    {menu === 'model' ? <div className={`${styles.menu} ${styles.menuRight}`} role="menu" aria-label="Choose who answers">
      <MenuItem
        label="Apocrypha+" sub="Premium · the flagship"
        hint={premiumHint ?? 'Apocrypha+: Premium, the flagship'}
        locked={premiumHint !== null} checked={lane === 'flagship'}
        onClick={() => { onLane('flagship'); setMenu('none'); }}
      />
      <MenuItem
        label="Local" sub="Free · the model on Apocky's machine"
        hint="Free: answered by the local model on Apocky's machine"
        checked={lane === 'local'}
        onClick={() => { onLane('local'); onTools([]); setMenu('none'); }}
      />
    </div> : null}

    {attachments.length + tools.length > 0 ? <div className={styles.chips}>
      {tools.map((tool) => <span key={tool} className={styles.chip}>
        {tool === 'image' ? <ImageGenIcon size={14} /> : <GlobeIcon size={14} />} {TOOL_LABEL[tool]}
        <Tip label={`Remove ${TOOL_LABEL[tool]}`}><button type="button" className={styles.chipX} aria-label={`Remove ${TOOL_LABEL[tool]}`} onClick={() => onTools(tools.filter((t) => t !== tool))}><CloseIcon size={12} /></button></Tip>
      </span>)}
      {attachments.map((a) => <span key={a.key} className={`${styles.chip} ${a.state === 'failed' ? styles.chipBad : ''}`} title={a.error ?? a.name}>
        <FileIcon size={14} /> <span className={styles.chipName}>{a.name}</span>
        {a.state === 'uploading' ? <span className={styles.chipState}>uploading…</span> : a.state === 'failed' ? <span className={styles.chipState}>failed</span> : null}
        <Tip label={`Remove ${a.name}`}><button type="button" className={styles.chipX} aria-label={`Remove ${a.name}`} onClick={() => onRemoveAttachment(a.key)}><CloseIcon size={12} /></button></Tip>
      </span>)}
    </div> : null}

    <form className={styles.pill} onSubmit={(e) => { e.preventDefault(); void submit(); }}>
      <Tip label="Add photos, files and tools" align="start">
        <button type="button" className={`${styles.iconBtn} ${menu === 'plus' ? styles.iconOn : ''}`} aria-label="Add photos, files and tools" aria-haspopup="menu" aria-expanded={menu === 'plus'} onClick={() => setMenu(menu === 'plus' ? 'none' : 'plus')}>
          <PlusIcon />
        </button>
      </Tip>
      <label htmlFor="room-say" className={styles.srOnly}>Message</label>
      <textarea
        id="room-say"
        ref={ref}
        className={styles.textarea}
        rows={1}
        value={text}
        maxLength={4000}
        placeholder={room.startsWith('l:') || room === 'lobby' ? 'Message the lobby' : 'Message Apocrypha'}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={onKey}
        autoComplete="off"
      />
      <Tip label={`Answered by ${modelName}. Choose who answers`} align="end">
        <button type="button" className={styles.modelBtn} aria-label={`Model: ${modelName}. Choose who answers`} aria-haspopup="menu" aria-expanded={menu === 'model'} onClick={() => setMenu(menu === 'model' ? 'none' : 'model')}>
          {modelName} <ChevronIcon size={14} />
        </button>
      </Tip>
      <Tip label={uploading ? 'Waiting for uploads to finish' : busy ? 'Apocrypha is answering; send another when you like' : 'Send (Enter)'} align="end">
        <button type="submit" className={styles.sendBtn} disabled={!canSend} aria-label="Send">
          <SendIcon />
        </button>
      </Tip>
    </form>
    {error ? <p className={styles.error} role="alert">{error}</p> : null}
  </div>;
}

function MenuItem({ icon, label, sub, hint, disabled = false, locked = false, checked, onClick }: {
  readonly icon?: JSX.Element;
  readonly label: string;
  readonly sub?: string;
  readonly hint: string;
  readonly disabled?: boolean;
  readonly locked?: boolean;
  readonly checked?: boolean;
  readonly onClick: () => void;
}): JSX.Element {
  const off = disabled || locked;
  return <Tip label={hint} align="start" side="top">
    <button
      type="button"
      role={checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
      aria-checked={checked}
      aria-disabled={off}
      className={`${styles.menuItem} ${off ? styles.menuItemOff : ''} ${checked ? styles.menuItemOn : ''}`}
      onClick={() => { if (!off) onClick(); }}
    >
      {icon ? <span className={styles.menuIcon}>{icon}</span> : null}
      <span className={styles.menuText}><span>{label}</span>{sub ? <small>{sub}</small> : null}</span>
      {locked ? <LockIcon size={15} /> : checked ? <span className={styles.menuCheck} aria-hidden="true">✓</span> : null}
    </button>
  </Tip>;
}
